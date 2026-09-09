use serde::{Deserialize, Serialize};

pub type ActualResult<T> = Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failure to invoke the Node bridge process (binary missing, IO error,
    /// non-zero exit with malformed JSON, ...).
    #[error("bridge invocation failed: {0}")]
    Bridge(String),

    /// Failure setting up the self-installed managed bridge (creating its
    /// directory, resolving a home dir, or running `npm ci`) - distinct from
    /// `Bridge` since nothing was actually invoked yet.
    #[error("bridge install failed: {0}")]
    BridgeInstall(String),

    /// The bridge process produced output that didn't match the protocol.
    #[error("bridge protocol error: {0}")]
    BridgeProtocol(String),

    /// A structured error returned by the bridge / Actual itself.
    #[error("actual API error [{}]: {}", .0.code, .0.message)]
    Api(ApiError),

    /// JSON conversion failure on a successful response.
    #[error("response conversion failed: {0}")]
    Conversion(#[from] serde_json::Error),

    /// IO error spawning or reading from the bridge.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A bridge session is no longer usable (poisoned by a prior fatal
    /// error, already closed, or the child process is gone).
    #[error("bridge session unusable: {0}")]
    SessionClosed(String),

    /// A bridge operation didn't respond within its timeout.
    #[error("bridge operation `{operation}` timed out after {secs}s")]
    Timeout { operation: String, secs: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    /// Whether the session that produced this error is no longer usable.
    #[serde(default)]
    pub fatal: bool,
}

impl ApiError {
    pub fn is_account_not_found(&self) -> bool {
        self.code == "account-not-found"
    }
}
