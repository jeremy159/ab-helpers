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
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(data_dir: &str, state: &DaemonState) {
    let path = state_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&path, json);
    }
}

/// Returns true if the next scheduled tick after `last_run` is already in the past.
fn tick_was_missed(cron_5field: &str, last_run: DateTime<Utc>) -> bool {
    let expr = format!("0 {cron_5field}"); // prepend seconds field
    let Ok(schedule) = Schedule::from_str(&expr) else { return false };
    schedule.after(&last_run).next().map_or(false, |next| next < Utc::now())
}

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let tz: Tz = settings.scheduler.timezone.parse()
        .context("invalid timezone in scheduler config")?;

    let data_dir = settings.actual.data_dir.clone();
    let state = Arc::new(Mutex::new(load_state(&data_dir)));

    // --- Missed-tick catch-up on startup ---
    {
        let s = state.lock().await;
        let kia_cron = settings.scheduler.kia_interest_cron.clone();
        let mort_cron = settings.scheduler.mortgage_interest_cron.clone();

        let kia_missed = s.kia_interest_last_run
            .map_or(true, |t| tick_was_missed(&kia_cron, t));
        let mort_missed = s.mortgage_interest_last_run
            .map_or(true, |t| tick_was_missed(&mort_cron, t));

        drop(s); // release lock before async calls

        if kia_missed {
            tracing::info!("kia interest tick was missed — running now");
            run_kia(&settings, &state, &data_dir).await;
        }
        if mort_missed {
            tracing::info!("mortgage interest tick was missed — running now");
            run_mortgage(&settings, &state, &data_dir).await;
        }
    }

    // --- Cron scheduler ---
    let mut scheduler = JobScheduler::new().await?;

    {
        let settings_kia = settings.clone();
        let state_kia = Arc::clone(&state);
        let data_dir_kia = data_dir.clone();
        let kia_expr = format!("0 {}", settings.scheduler.kia_interest_cron);

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
            tracing::error!(?err, "mortgage interest failed — will retry next scheduled tick");
        }
    }
}
