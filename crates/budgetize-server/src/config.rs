use std::path::PathBuf;

use db_postgres::{PgConnectOptions, PgSslMode};
use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;
use sqlx::ConnectOptions;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub actual: ActualSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: Secret<String>,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(self.password.expose_secret())
            .port(self.port)
            .ssl_mode(ssl_mode)
    }

    pub fn with_db(&self) -> PgConnectOptions {
        self.without_db()
            .database(&self.database_name)
            .log_statements(tracing::log::LevelFilter::Trace)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisSettings {
    pub host: String,
    pub port: u16,
}

impl RedisSettings {
    pub fn connection_string(&self) -> String {
        format!("redis://{}:{}/", self.host, self.port)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActualSettings {
    pub server_url: String,
    pub password: Secret<String>,
    pub sync_id: String,
    #[serde(default)]
    pub e2e_password: Option<Secret<String>>,
    /// Local cache directory for the Actual client. Empty → falls back to
    /// `<workspace>/.actual-data`.
    #[serde(default)]
    pub data_dir: String,
    /// Override the `node` binary. Defaults to `node` on `PATH`.
    #[serde(default = "default_node_bin")]
    pub node_bin: String,
    /// Path to the bridge script. Empty → relative to workspace root
    /// (`crates/actual/bridge/index.js`).
    #[serde(default)]
    pub bridge_script: String,
}

fn default_node_bin() -> String {
    "node".to_string()
}

impl ActualSettings {
    pub fn bridge_config(&self) -> actual::BridgeConfig {
        let workspace_root = workspace_root();
        let bridge_script = if self.bridge_script.is_empty() {
            workspace_root.join("crates/actual/bridge/index.js")
        } else {
            PathBuf::from(&self.bridge_script)
        };
        let data_dir = if self.data_dir.is_empty() {
            workspace_root.join(".actual-data")
        } else {
            PathBuf::from(&self.data_dir)
        };

        actual::BridgeConfig {
            node_bin: PathBuf::from(&self.node_bin),
            bridge_script,
            server_url: self.server_url.clone(),
            password: self.password.expose_secret().clone(),
            sync_id: self.sync_id.clone(),
            e2e_password: self
                .e2e_password
                .as_ref()
                .map(|s| s.expose_secret().clone())
                .filter(|s| !s.is_empty()),
            data_dir,
        }
    }

    pub fn client(&self) -> actual::Client {
        actual::Client::new(self.bridge_config())
    }
}

/// Best-effort resolution of the workspace root from the current working dir.
fn workspace_root() -> PathBuf {
    let mut p = std::env::current_dir().expect("Failed to determine the current directory");
    loop {
        if p.join("Cargo.toml").is_file() && p.join("crates").is_dir() {
            return p;
        }
        if !p.pop() {
            return std::env::current_dir().unwrap();
        }
    }
}

impl Settings {
    pub fn build() -> Result<Self, config::ConfigError> {
        let base_path = {
            let p = std::env::current_dir().expect("Failed to determine the current directory");
            if !p.ends_with("crates/budgetize-server") {
                p.join("crates/budgetize-server")
            } else {
                p
            }
        };
        let configuration_directory = base_path.join("configuration");

        let environment: Environment = std::env::var("BUDGETIZE_ENVIRONMENT")
            .unwrap_or_else(|_| "local".into())
            .try_into()
            .expect("Failed to parse BUDGETIZE_ENVIRONMENT.");
        let environment_filename = format!("{}.toml", environment.as_str());

        let settings = config::Config::builder()
            .add_source(config::File::from(
                configuration_directory.join("base.toml"),
            ))
            .add_source(config::File::from(
                configuration_directory.join(environment_filename),
            ))
            .add_source(
                config::Environment::with_prefix("BUDGETIZE")
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()?;

        settings.try_deserialize::<Self>()
    }
}

pub enum Environment {
    Local,
    Test,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Test => "test",
            Environment::Staging => "staging",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "test" => Ok(Self::Test),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} is not a supported environment. Use `local`, `test`, `staging`, or `production`."
            )),
        }
    }
}
