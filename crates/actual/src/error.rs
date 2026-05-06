use serde::{Deserialize, Serialize};

pub type ActualResult<T> = Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failure to invoke the Node bridge process (binary missing, IO error,
    /// non-zero exit with malformed JSON, ...).
    #[error("bridge invocation failed: {0}")]
    Bridge(String),

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn is_account_not_found(&self) -> bool {
        self.code == "account-not-found"
    }
}
