use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

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

const BRIDGE_READY_ATTEMPTS: u32 = 6;
const BRIDGE_READY_RETRY_DELAY: Duration = Duration::from_secs(10);
const BRIDGE_READY_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Blocks until the Actual server at `server_url` accepts a connection, or
/// gives up after `BRIDGE_READY_ATTEMPTS` attempts. This exists because right
/// after a container start the daemon's startup catch-up can run before the
/// Actual server/network is reachable; probing first avoids spawning the
/// bridge process against a server that isn't up yet.
async fn wait_for_bridge_ready(server_url: &str) {
    let client = match reqwest::Client::builder()
        .timeout(BRIDGE_READY_PROBE_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to build readiness-probe HTTP client, skipping startup readiness wait"
            );
            return;
        }
    };

    for attempt in 1..=BRIDGE_READY_ATTEMPTS {
        // Any response at all, even an error status, proves the server is
        // reachable; only a connection/timeout failure means "not up yet".
        match client.get(server_url).send().await {
            Ok(_) => return,
            Err(err)
                if attempt < BRIDGE_READY_ATTEMPTS && (err.is_connect() || err.is_timeout()) =>
            {
                tracing::warn!(
                    ?err,
                    attempt,
                    max_attempts = BRIDGE_READY_ATTEMPTS,
                    "Actual server not reachable yet, waiting before startup catch-up"
                );
                tokio::time::sleep(BRIDGE_READY_RETRY_DELAY).await;
            }
            Err(err) => {
                tracing::warn!(
                    ?err,
                    "giving up waiting for Actual server readiness, proceeding anyway"
                );
                return;
            }
        }
    }
}

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

    serde_json::from_str(&raw)
        .inspect(|_| tracing::trace!("daemon state loaded successfully"))
        .unwrap_or_else(|err| {
            tracing::warn!(?err, path = %path.display(), "daemon state file is corrupt, starting fresh");
            DaemonState::default()
        })
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

/// Logs the next time a job matching `cron_6field` will fire, in a
/// human-readable, timezone-aware format.
fn log_next_run(cron_6field: &str, tz: Tz, kind: &str) {
    let schedule = match Schedule::from_str(cron_6field) {
        Ok(schedule) => schedule,
        Err(err) => {
            tracing::warn!(
                ?err,
                expr = cron_6field,
                kind,
                "failed to parse cron expression"
            );
            return;
        }
    };

    let Some(next_run) = schedule.upcoming(tz).next() else {
        tracing::warn!(kind, "could not compute next scheduled run");
        return;
    };

    tracing::info!(
        kind,
        next_run = %next_run.format("%Y-%m-%d %H:%M:%S %Z"),
        "next {kind} interest run scheduled"
    );
}

pub async fn run(settings: Settings) -> Result<(), CliError> {
    run_inner(settings).await.map_err(CliError::Failure)
}

async fn run_inner(settings: Settings) -> anyhow::Result<()> {
    let tz = settings.scheduler.timezone;
    tracing::info!(%tz, "daemon initializing");

    let data_dir = settings.actual.bridge_config().await?.cache_dir;
    let state = Arc::new(Mutex::new(load_state(&data_dir)));

    // --- Missed-tick catch-up on startup ---
    {
        let kinds = [InterestKind::Kia, InterestKind::Mortgage];
        let mut missed_kinds = Vec::new();
        for kind in kinds {
            let cron = state.lock().await.cron(kind, &settings).to_owned();
            let missed = state
                .lock()
                .await
                .last_run(kind)
                .is_none_or(|t| tick_was_missed(&cron, t, tz));
            if missed {
                missed_kinds.push(kind);
            } else {
                tracing::info!(
                    kind = kind.label(),
                    "interest tick is current, no catch-up needed"
                );
            }
        }

        if !missed_kinds.is_empty() {
            wait_for_bridge_ready(&settings.actual.server_url).await;
            for kind in missed_kinds {
                tracing::info!(
                    kind = kind.label(),
                    "interest tick was missed - running now"
                );
                run_interest(kind, &settings, &state, &data_dir).await;
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
        log_next_run(&cron_expr, tz, label);

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
    let config = kind.config(settings);

    let outcome = settings
        .actual
        .with_session(move |client| async move {
            let service = InterestService::new(client, config, false);
            service.run::<Live>().await
        })
        .await;

    match outcome {
        Ok(ab_helpers_domain::LiveOutcome::Skip(reason)) => {
            match reason {
                ab_helpers_domain::InterestSkip::AccountClosed => {
                    tracing::info!(kind = label, "interest skipped: account closed");
                }
                ab_helpers_domain::InterestSkip::NoInterest { balance, cutoff } => {
                    tracing::info!(
                        %balance,
                        %cutoff,
                        kind = label,
                        "interest skipped: no interest to apply"
                    );
                }
                ab_helpers_domain::InterestSkip::AlreadyApplied {
                    payee_name,
                    date,
                    amount,
                } => {
                    tracing::info!(
                        %payee_name,
                        %date,
                        %amount,
                        kind = label,
                        "interest skipped: already applied"
                    );
                }
            }
            let mut s = state.lock().await;
            s.set_last_run(kind, Utc::now());
            save_state(data_dir, &s);
        }
        Ok(ab_helpers_domain::LiveOutcome::Applied {
            balance,
            interest,
            new_balance,
            transaction_id,
        }) => {
            tracing::info!(
                %balance,
                %interest,
                %new_balance,
                %transaction_id,
                kind = label,
                "interest applied"
            );
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
