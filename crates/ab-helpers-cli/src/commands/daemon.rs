use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use ab_helpers_server::{
    config::Settings,
    execution::{Live, PlanExecute},
    services::actual::InterestService,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

use super::error::CliError;
use super::interest::InterestKind;

#[derive(Debug, Serialize, Deserialize, Default)]
struct DaemonState {
    kia_interest_last_run: Option<DateTime<Utc>>,
    mortgage_interest_last_run: Option<DateTime<Utc>>,
}

impl DaemonState {
    fn last_run(&self, kind: InterestKind) -> Option<DateTime<Utc>> {
        match kind {
            InterestKind::Kia => self.kia_interest_last_run,
            InterestKind::Mortgage => self.mortgage_interest_last_run,
        }
    }

    fn set_last_run(&mut self, kind: InterestKind, time: DateTime<Utc>) {
        match kind {
            InterestKind::Kia => self.kia_interest_last_run = Some(time),
            InterestKind::Mortgage => self.mortgage_interest_last_run = Some(time),
        }
    }

    fn cron<'a>(&self, kind: InterestKind, settings: &'a Settings) -> &'a str {
        match kind {
            InterestKind::Kia => &settings.scheduler.kia_interest_cron,
            InterestKind::Mortgage => &settings.scheduler.mortgage_interest_cron,
        }
    }
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("daemon-state.json")
}

fn load_state(data_dir: &Path) -> DaemonState {
    let path = state_path(data_dir);
    tracing::debug!(path = %path.display(), "loading daemon state");
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("no daemon state file found, starting fresh");
            return DaemonState::default();
        }
        Err(err) => {
            tracing::warn!(?err, path = %path.display(), "failed to read daemon state file, starting fresh");
            return DaemonState::default();
        }
    };

    match serde_json::from_str(&raw) {
        Ok(state) => {
            tracing::trace!("daemon state loaded successfully");
            state
        }
        Err(err) => {
            tracing::warn!(?err, path = %path.display(), "daemon state file is corrupt, starting fresh");
            DaemonState::default()
        }
    }
}

fn save_state(data_dir: &Path, state: &DaemonState) {
    let path = state_path(data_dir);
    tracing::trace!(path = %path.display(), "saving daemon state");

    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(?err, dir = %parent.display(), "failed to create state directory");
        return;
    }

    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                tracing::warn!(?err, path = %path.display(), "failed to write daemon state file");
            }
        }
        Err(err) => {
            tracing::warn!(?err, "failed to serialize daemon state");
        }
    }
}

/// Returns true if the next scheduled tick after `last_run` is already in the past.
fn tick_was_missed(cron_5field: &str, last_run: DateTime<Utc>, tz: Tz) -> bool {
    let expr = format!("0 {cron_5field}"); // prepend seconds field
    let Ok(schedule) = Schedule::from_str(&expr) else {
        tracing::warn!(expr = %expr, "failed to parse cron expression for missed-tick check");
        return false;
    };
    let last_run_tz = last_run.with_timezone(&tz);
    let next = schedule.after(&last_run_tz).next();
    let now = Utc::now();
    tracing::trace!(last_run = %last_run, next = ?next, now = %now, "tick_was_missed check");
    next.is_some_and(|next| next < now)
}

pub async fn run(settings: Settings) -> Result<(), CliError> {
    run_inner(settings).await.map_err(CliError::Failure)
}

async fn run_inner(settings: Settings) -> anyhow::Result<()> {
    let tz = settings.scheduler.timezone;
    tracing::info!(%tz, "daemon initializing");

    let data_dir = settings.actual.bridge_config().cache_dir;
    let state = Arc::new(Mutex::new(load_state(&data_dir)));

    // --- Missed-tick catch-up on startup ---
    {
        let kinds = [InterestKind::Kia, InterestKind::Mortgage];
        for kind in kinds {
            let cron = state.lock().await.cron(kind, &settings).to_owned();
            let missed = state
                .lock()
                .await
                .last_run(kind)
                .is_none_or(|t| tick_was_missed(&cron, t, tz));
            if missed {
                tracing::info!(
                    kind = kind.label(),
                    "interest tick was missed - running now"
                );
                run_interest(kind, &settings, &state, &data_dir).await;
            } else {
                tracing::info!(
                    kind = kind.label(),
                    "interest tick is current, no catch-up needed"
                );
            }
        }
    }
    tracing::debug!("missed-tick catch-up complete");

    // --- Cron scheduler ---
    let mut scheduler = JobScheduler::new().await?;

    for kind in [InterestKind::Kia, InterestKind::Mortgage] {
        let cron_expr = format!(
            "0 {}",
            match kind {
                InterestKind::Kia => &settings.scheduler.kia_interest_cron,
                InterestKind::Mortgage => &settings.scheduler.mortgage_interest_cron,
            }
        );
        let label = kind.label();
        tracing::debug!(expr = %cron_expr, kind = label, "scheduling interest job");

        let settings = settings.clone();
        let state = Arc::clone(&state);
        let data_dir = data_dir.clone();

        let job = Job::new_async_tz(&cron_expr, tz, move |_uuid, _lock| {
            let settings = settings.clone();
            let state = Arc::clone(&state);
            let data_dir = data_dir.clone();
            Box::pin(async move {
                tracing::info!(kind = label, "scheduler: running interest");
                run_interest(kind, &settings, &state, &data_dir).await;
            })
        })?;
        scheduler.add(job).await?;
    }

    tracing::info!("daemon started");
    scheduler.start().await?;

    // Block forever: the scheduler runs background tasks.
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
    scheduler.shutdown().await?;
    tracing::info!("daemon stopped");

    Ok(())
}

async fn run_interest(
    kind: InterestKind,
    settings: &Settings,
    state: &Arc<Mutex<DaemonState>>,
    data_dir: &Path,
) {
    let label = kind.label();
    let client = Arc::new(settings.actual.client());
    let config = kind.config(settings);
    let service = InterestService::new(client, config);

    match service.run::<Live>().await {
        Ok(outcome) => {
            tracing::info!(?outcome, kind = label, "interest applied");
            let mut s = state.lock().await;
            s.set_last_run(kind, Utc::now());
            save_state(data_dir, &s);
        }
        Err(err) => {
            tracing::error!(
                ?err,
                kind = label,
                "interest failed: will retry next scheduled tick"
            );
        }
    }
}
