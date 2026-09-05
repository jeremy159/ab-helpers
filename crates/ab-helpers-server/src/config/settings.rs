use std::path::{Path, PathBuf};

use ab_helpers_domain::InterestPeriod;
use chrono_tz::Tz;
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
    pub scheduler: SchedulerSettings,
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
pub struct KiaSettings {
    pub account_id: String,
    pub weekly_rate: f64,
    pub payee_name: String,
    pub round: bool,
}

impl KiaSettings {
    pub fn interest_config(&self) -> InterestConfig {
        InterestConfig {
            account_id: self.account_id.clone(),
            rate: self.weekly_rate,
            payee_name: self.payee_name.clone(),
            round: self.round,
            period: InterestPeriod::Weekly,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MortgageSettings {
    pub account_id: String,
    pub monthly_rate: f64,
    pub payee_name: String,
    pub round: bool,
}

impl MortgageSettings {
    pub fn interest_config(&self) -> InterestConfig {
        InterestConfig {
            account_id: self.account_id.clone(),
            rate: self.monthly_rate,
            payee_name: self.payee_name.clone(),
            round: self.round,
            period: InterestPeriod::Monthly,
        }
    }
}

pub struct InterestConfig {
    pub account_id: String,
    pub rate: f64,
    pub payee_name: String,
    pub round: bool,
    pub period: InterestPeriod,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerSettings {
    pub kia_interest_cron: String,
    pub mortgage_interest_cron: String,
    pub timezone: Tz,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActualSettings {
    pub server_url: String,
    pub password: Secret<String>,
    pub sync_id: Secret<String>,
    /// Local cache directory for the Actual client. Empty → falls back to
    /// the XDG data dir (`$XDG_DATA_HOME/ab-helpers/cache`, or
    /// `~/.local/share/ab-helpers/cache` if unset) - see
    /// `default_cache_dir()`.
    #[serde(default)]
    pub cache_dir: String,
    /// Override the `node` binary. Defaults to `node` on `PATH`.
    #[serde(default = "default_node_bin")]
    pub node_bin: String,
    /// Path to the bridge script. Empty → use `crates/actual/bridge/index.js`
    /// relative to the workspace root if running from inside a full checkout
    /// of this repo (dev), otherwise fall back to a self-installed managed
    /// copy under the XDG data dir (see `actual::ensure_managed_bridge_script`).
    #[serde(default)]
    pub bridge_script: String,
    pub kia: KiaSettings,
    pub mortgage: MortgageSettings,
}

fn default_node_bin() -> String {
    "node".to_string()
}

impl ActualSettings {
    /// Resolves the full bridge configuration. Falls back from an explicit
    /// `bridge_script`, to a working dev checkout, to a self-installed
    /// managed copy (may run `npm ci`, hence `async`).
    pub async fn bridge_config(&self) -> anyhow::Result<actual::BridgeConfig> {
        let node_bin = PathBuf::from(&self.node_bin);

        let bridge_script = if self.bridge_script.is_empty() {
            let dev_path = resource_root().join("crates/actual/bridge/index.js");
            let dev_node_modules_installed = dev_path
                .parent()
                .map(|p| p.join("node_modules").is_dir())
                .unwrap_or(false);
            if dev_path.is_file() && dev_node_modules_installed {
                dev_path
            } else {
                let node_bin_for_install = node_bin.clone();
                tokio::task::spawn_blocking(move || {
                    actual::ensure_managed_bridge_script(&node_bin_for_install)
                })
                .await
                .map_err(|e| anyhow::anyhow!("managed bridge install task panicked: {e}"))??
            }
        } else {
            PathBuf::from(&self.bridge_script)
        };

        let cache_dir = if self.cache_dir.is_empty() {
            default_cache_dir()
        } else {
            PathBuf::from(&self.cache_dir)
        };

        Ok(actual::BridgeConfig {
            node_bin,
            bridge_script,
            server_url: self.server_url.clone(),
            password: self.password.clone(),
            sync_id: self.sync_id.clone(),
            cache_dir,
        })
    }

    pub async fn client(&self) -> anyhow::Result<actual::Client> {
        Ok(actual::Client::new(self.bridge_config().await?))
    }
}

/// Base directory for resolving the bundled Node bridge script (dev only) and
/// (always) the default cache dir. Prefers the workspace root when running
/// inside the source tree; otherwise the directory of the running executable.
/// `bridge_script` self-installs a managed copy when not running from inside
/// a full checkout (see `ActualSettings::bridge_config`); in Docker / an
/// installed CLI, `cache_dir` should still be set explicitly instead of
/// relying on this.
fn resource_root() -> PathBuf {
    if let Ok(mut p) = std::env::current_dir() {
        loop {
            if p.join("Cargo.toml").is_file() && p.join("crates").is_dir() {
                return p;
            }
            if !p.pop() {
                break;
            }
        }
    }
    exe_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Default location for the Actual local data cache when `cache_dir` is unset.
/// Resolves to `$XDG_DATA_HOME/ab-helpers/cache` (or `~/.local/share/...`) so
/// every CLI invocation reads/writes the same place regardless of the working
/// directory - this also holds the daemon's idempotency state, so it must not
/// live under a clearable cache dir. Docker overrides it via
/// `ABH_ACTUAL__CACHE_DIR`.
fn default_cache_dir() -> PathBuf {
    user_data_dir()
        .map(|d| d.join("cache"))
        .unwrap_or_else(|| resource_root().join("cache"))
}

/// Directory of the running executable, if resolvable.
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// `$XDG_DATA_HOME/ab-helpers` or `~/.local/share/ab-helpers`.
fn user_data_dir() -> Option<PathBuf> {
    xdg_dir("XDG_DATA_HOME", ".local/share")
}

fn xdg_dir(env_var: &str, home_suffix: &str) -> Option<PathBuf> {
    if let Some(x) = std::env::var_os(env_var)
        && !x.is_empty()
    {
        return Some(PathBuf::from(x).join("ab-helpers"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(home_suffix).join("ab-helpers"))
}

/// Locate the repo's `crates/ab-helpers-server/configuration` dir by walking up
/// from the current directory (dev only).
fn repo_config_dir() -> Option<PathBuf> {
    let mut p = std::env::current_dir().ok()?;
    loop {
        let candidate = p.join("crates/ab-helpers-server/configuration");
        if candidate.join("base.toml").is_file() {
            return Some(candidate);
        }
        if !p.pop() {
            return None;
        }
    }
}

/// Source `configuration/` dir to seed XDG config from (used by `abh init`):
/// the dir next to the executable, else the repo dir.
pub fn default_config_source_dir() -> Option<PathBuf> {
    if let Some(dir) = exe_dir().map(|d| d.join("configuration"))
        && dir.join("base.toml").is_file()
    {
        return Some(dir);
    }
    repo_config_dir()
}

/// `$XDG_CONFIG_HOME/ab-helpers` or `~/.config/ab-helpers`.
pub fn user_config_dir() -> Option<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

fn environment() -> Environment {
    std::env::var("ABH_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse ABH_ENVIRONMENT.")
}

struct Layer {
    path: PathBuf,
    required: bool,
}

/// Ordered config files to layer (later overrides earlier), before
/// `ABH_`-prefixed env vars are applied on top. First match wins:
///
/// 1. `ABH_CONFIG_FILE`: a single explicit file.
/// 2. `ABH_CONFIG_DIR`: `base.toml` + `<ABH_ENVIRONMENT>.toml` in that dir.
/// 3. `<exe>/configuration/base.toml`: dir next to the binary (Docker).
/// 4. `~/.config/ab-helpers/{base,config}.toml`: installed CLI.
/// 5. The repo `configuration/` dir, found by walking up from the CWD (dev).
fn config_layers() -> Result<Vec<Layer>, config::ConfigError> {
    if let Some(f) = non_empty_var("ABH_CONFIG_FILE") {
        return Ok(vec![Layer {
            path: PathBuf::from(f),
            required: true,
        }]);
    }
    if let Some(d) = non_empty_var("ABH_CONFIG_DIR") {
        return Ok(env_dir_layers(&PathBuf::from(d)));
    }
    if let Some(dir) = exe_dir().map(|d| d.join("configuration"))
        && dir.join("base.toml").is_file()
    {
        return Ok(env_dir_layers(&dir));
    }
    if let Some(dir) = user_config_dir()
        && (dir.join("base.toml").is_file() || dir.join("config.toml").is_file())
    {
        return Ok(vec![
            Layer {
                path: dir.join("base.toml"),
                required: false,
            },
            Layer {
                path: dir.join("config.toml"),
                required: false,
            },
        ]);
    }
    if let Some(dir) = repo_config_dir() {
        return Ok(env_dir_layers(&dir));
    }
    Err(config::ConfigError::Message(format!(
        "no configuration found. Set ABH_CONFIG_FILE or ABH_CONFIG_DIR, run `abh init` \
         to create {}, or invoke from the project directory.",
        user_config_dir()
            .map(|d| d.join("config.toml").display().to_string())
            .unwrap_or_else(|| "~/.config/ab-helpers/config.toml".into()),
    )))
}

/// `base.toml` (required) + `<env>.toml` (optional) for a server-style dir.
fn env_dir_layers(dir: &Path) -> Vec<Layer> {
    vec![
        Layer {
            path: dir.join("base.toml"),
            required: true,
        },
        Layer {
            path: dir.join(format!("{}.toml", environment().as_str())),
            required: false,
        },
    ]
}

fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// Lets a secret be supplied via a `..._FILE` env var pointing at a file
/// (the Docker/Postgres convention, e.g. for secrets mounted from Docker
/// secrets) instead of the value being set directly in an env var.
///
/// Implemented as a `config::Format` so it can plug into the same
/// layering/merging machinery as the TOML files: rather than actually
/// parsing anything, it just reads the whole file, trims trailing
/// whitespace, and stores that as the value at one fixed dotted config key
/// (`self.0`, e.g. `"database.password"`). See `SECRET_FILE_VARS` for how
/// env vars map to these keys.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawSecret(pub(crate) &'static str);

impl config::Format for RawSecret {
    fn parse(
        &self,
        uri: Option<&String>,
        text: &str,
    ) -> Result<config::Map<String, config::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let mut map = config::Map::new();
        map.insert(
            self.0.to_string(),
            config::Value::new(uri, text.trim_end().to_string()),
        );
        Ok(map)
    }
}

impl config::FileStoredFormat for RawSecret {
    fn file_extensions(&self) -> &'static [&'static str] {
        &["txt"]
    }
}

/// The secret fields that accept a `..._FILE` env var indirection, paired
/// with their dotted config path.
const SECRET_FILE_VARS: &[(&str, &str)] = &[
    ("ABH_DATABASE__PASSWORD_FILE", "database.password"),
    ("ABH_ACTUAL__PASSWORD_FILE", "actual.password"),
    ("ABH_ACTUAL__SYNC_ID_FILE", "actual.sync_id"),
];

/// Resolve which `_FILE` secret sources to add: for each field in
/// `SECRET_FILE_VARS` whose `_FILE` var is set, its file path. Errors if both
/// the direct var and the `_FILE` var are set for the same field (ambiguous -
/// same convention the official Postgres/MySQL Docker images use).
pub(crate) fn resolve_secret_file_paths(
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Vec<(&'static str, String)>, config::ConfigError> {
    let mut resolved = Vec::new();
    for (file_var, dotted_key) in SECRET_FILE_VARS {
        let direct_var = file_var
            .strip_suffix("_FILE")
            .expect("SECRET_FILE_VARS entries are declared with a _FILE suffix");
        match (lookup(direct_var), lookup(file_var)) {
            (Some(_), Some(_)) => {
                return Err(config::ConfigError::Message(format!(
                    "both {direct_var} and {file_var} are set; unset one"
                )));
            }
            (_, Some(path)) => resolved.push((*dotted_key, path)),
            _ => {}
        }
    }
    Ok(resolved)
}

impl Settings {
    pub fn build() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();
        for layer in config_layers()? {
            builder = builder.add_source(config::File::from(layer.path).required(layer.required));
        }
        builder = builder.add_source(
            config::Environment::with_prefix("ABH")
                .prefix_separator("_")
                .separator("__"),
        );
        for (dotted_key, path) in resolve_secret_file_paths(non_empty_var)? {
            builder =
                builder.add_source(config::File::new(&path, RawSecret(dotted_key)).required(true));
        }
        builder.build()?.try_deserialize::<Self>()
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
