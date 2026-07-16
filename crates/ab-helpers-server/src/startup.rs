use anyhow::{Context, Result};

use axum::{Router, body::Body, routing::get};
use http::{Request, header::CONTENT_TYPE};
use sqlx::PgPool;
use tokio::{net::TcpListener, signal};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::error_span;

use crate::{
    config::Settings,
    routes::{get_api_routes, health_check},
};

#[derive(Clone)]
pub struct AppState {
    pub db_conn_pool: PgPool,
    pub redis_conn_pool: db_redis::RedisPool,
}

pub struct Application {
    listener: TcpListener,
    port: u16,
    app: Router,
}

impl Application {
    pub async fn build(configuration: Settings, db_conn_pool: PgPool) -> Result<Self> {
        let redis_conn_pool =
            db_redis::get_connection_pool(&configuration.redis.connection_string())
                .await
                .context("failed to get redis connection pool")?;

        let app_state = AppState {
            db_conn_pool,
            redis_conn_pool,
        };

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );

        let listener = TcpListener::bind(&address).await?;
        let port = listener
            .local_addr()
            .with_context(|| format!("failed to get local address from {address}"))?
            .port();

        let origins = std::env::var("ABH_CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect::<Vec<_>>();

        let api_routes = get_api_routes(&app_state);

        let app = Router::new()
            .route("/health_check", get(health_check))
            .nest("/api", api_routes)
            .layer(
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_headers([CONTENT_TYPE])
                    .allow_methods([
                        http::Method::GET,
                        http::Method::POST,
                        http::Method::PUT,
                        http::Method::DELETE,
                        http::Method::OPTIONS,
                        http::Method::HEAD,
                    ]),
            )
            .layer(
                TraceLayer::new_for_http().make_span_with(|request: &Request<Body>| {
                    let request_id = request
                        .extensions()
                        .get::<RequestId>()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "unknown".into());
                    error_span!(
                        "request",
                        id = %request_id,
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                }),
            )
            .layer(RequestIdLayer)
            .with_state(app_state);

        Ok(Self {
            listener,
            port,
            app,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run(self) -> Result<()> {
        tracing::debug!("listening on {}", self.listener.local_addr()?);
        axum::serve(self.listener, self.app.into_make_service())
            .with_graceful_shutdown(Self::shutdown_signal())
            .await
            .context("failed to start server")?;
        Ok(())
    }

    async fn shutdown_signal() {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        tracing::info!("signal received, starting graceful shutdown");
    }
}
