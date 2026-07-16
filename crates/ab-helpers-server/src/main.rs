use ab_helpers_server::{
    config,
    startup::Application,
    telemetry::{get_subscriber, init_subscriber},
};
use anyhow::Context;
use db_postgres::get_connection_pool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = get_subscriber("ab_helpers=debug,tower_http=debug".into());
    init_subscriber(subscriber);

    let configuration = config::Settings::build()?;
    let db_conn_pool = get_connection_pool(configuration.database.with_db());
    let application = Application::build(configuration, db_conn_pool)
        .await
        .context("failed to build application")?;

    application
        .run()
        .await
        .context("failed to run application")?;

    Ok(())
}
