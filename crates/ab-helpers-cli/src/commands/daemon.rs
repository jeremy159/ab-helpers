use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::InterestService;
use anyhow::Context;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

#[derive(Debug, Serialize, Deserialize, Default)]
struct DaemonState {
    kia_interest_last_run: Option<DateTime<Utc>>,
    mortgage_interest_last_run: Option<DateTime<Utc>>,
}

fn state_path(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("daemon-state.json")
}

fn load_state(data_dir: &str) -> DaemonState {
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

fn save_state(data_dir: &str, state: &DaemonState) {
    let path = state_path(data_dir);
    tracing::trace!(path = %path.display(), "saving daemon state");

    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(?err, dir = %parent.display(), "failed to create state directory");
            return;
        }
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
fn tick_was_missed(cron_5field: &str, last_run: DateTime<Utc>) -> bool {
    let expr = format!("0 {cron_5field}"); // prepend seconds field
    let Ok(schedule) = Schedule::from_str(&expr) else {
        tracing::warn!(expr = %expr, "failed to parse cron expression for missed-tick check");
        return false;
    };
    let next = schedule.after(&last_run).next();
    let now = Utc::now();
    tracing::trace!(last_run = %last_run, next = ?next, now = %now, "tick_was_missed check");
    next.map_or(false, |next| next < now)
}

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let tz: Tz = settings
        .scheduler
        .timezone
        .parse()
        .context("invalid timezone in scheduler config")?;
    tracing::info!(%tz, "daemon initializing");

    let data_dir = settings.actual.cache_dir.clone();
    let state = Arc::new(Mutex::new(load_state(&data_dir)));

    // --- Missed-tick catch-up on startup ---
    {
        let s = state.lock().await;
        let kia_cron = settings.scheduler.kia_interest_cron.clone();
        let mort_cron = settings.scheduler.mortgage_interest_cron.clone();
        tracing::debug!(kia_cron = %kia_cron, mort_cron = %mort_cron, "checking for missed ticks");

        let kia_missed = s
            .kia_interest_last_run
            .map_or(true, |t| tick_was_missed(&kia_cron, t));
        let mort_missed = s
            .mortgage_interest_last_run
            .map_or(true, |t| tick_was_missed(&mort_cron, t));

        drop(s); // release lock before async calls

        if kia_missed {
            tracing::info!("kia interest tick was missed — running now");
            run_kia(&settings, &state, &data_dir).await;
        } else {
            tracing::info!("kia interest tick is current, no catch-up needed");
        }
        if mort_missed {
            tracing::info!("mortgage interest tick was missed — running now");
            run_mortgage(&settings, &state, &data_dir).await;
        } else {
            tracing::info!("mortgage interest tick is current, no catch-up needed");
        }
    }
    tracing::debug!("missed-tick catch-up complete");

    // --- Cron scheduler ---
    let mut scheduler = JobScheduler::new().await?;

    {
        let settings_kia = settings.clone();
        let state_kia = Arc::clone(&state);
        let data_dir_kia = data_dir.clone();
        let kia_expr = format!("0 {}", settings.scheduler.kia_interest_cron);
        tracing::debug!(expr = %kia_expr, "scheduling kia interest job");

        let kia_job = Job::new_async_tz(&kia_expr, tz, move |_uuid, _lock| {
            let s = settings_kia.clone();
            let st = Arc::clone(&state_kia);
            let dd = data_dir_kia.clone();
            Box::pin(async move {
                tracing::info!("scheduler: running kia interest");
                run_kia(&s, &st, &dd).await;
            })
        })?;
        scheduler.add(kia_job).await?;
    }

    {
        let settings_mort = settings.clone();
        let state_mort = Arc::clone(&state);
        let data_dir_mort = data_dir.clone();
        let mort_expr = format!("0 {}", settings.scheduler.mortgage_interest_cron);
        tracing::debug!(expr = %mort_expr, "scheduling mortgage interest job");

        let mort_job = Job::new_async_tz(&mort_expr, tz, move |_uuid, _lock| {
            let s = settings_mort.clone();
            let st = Arc::clone(&state_mort);
            let dd = data_dir_mort.clone();
            Box::pin(async move {
                tracing::info!("scheduler: running mortgage interest");
                run_mortgage(&s, &st, &dd).await;
            })
        })?;
        scheduler.add(mort_job).await?;
    }

    tracing::info!("daemon started");
    scheduler.start().await?;

    // Block forever — the scheduler runs background tasks.
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
    scheduler.shutdown().await?;
    tracing::info!("daemon stopped");

    Ok(ExitCode::SUCCESS)
}

async fn run_kia(settings: &Settings, state: &Arc<Mutex<DaemonState>>, data_dir: &str) {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(outcome) => {
            tracing::info!(?outcome, "kia interest applied");
            let mut s = state.lock().await;
            s.kia_interest_last_run = Some(Utc::now());
            save_state(data_dir, &s);
        }
        Err(err) => {
            tracing::error!(?err, "kia interest failed — will retry next scheduled tick");
        }
    }
}

async fn run_mortgage(settings: &Settings, state: &Arc<Mutex<DaemonState>>, data_dir: &str) {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.mortgage.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(outcome) => {
            tracing::info!(?outcome, "mortgage interest applied");
            let mut s = state.lock().await;
            s.mortgage_interest_last_run = Some(Utc::now());
            save_state(data_dir, &s);
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "mortgage interest failed — will retry next scheduled tick"
            );
        }
    }
}
