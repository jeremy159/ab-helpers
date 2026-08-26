#[derive(Debug)]
pub enum CliError {
    NotFound,
    Failure(anyhow::Error),
}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        CliError::Failure(e)
    }
}

/// Maps a server `AppError` to a `CliError`, translating account-not-found
/// and ambiguous-account variants to `NotFound` (exit 1).
pub fn map_app_error(err: ab_helpers_server::error::AppError) -> CliError {
    use ab_helpers_server::error::AppError;
    match err {
        AppError::ActualAccountNotFound(name) => {
            tracing::warn!(account = %name, "account not found");
            CliError::NotFound
        }
        AppError::ActualAccountAmbiguous { name, matches } => {
            tracing::warn!(account = %name, matches = %matches.join(", "), "account name is ambiguous");
            CliError::NotFound
        }
        other => CliError::Failure(other.into()),
    }
}

/// Same as `map_app_error` but includes a `kind` label in every log message,
/// useful when multiple services share a log stream (e.g. the interest command
/// running kia and mortgage jobs).
pub fn map_app_error_labelled(
    err: ab_helpers_server::error::AppError,
    kind: &str,
) -> CliError {
    use ab_helpers_server::error::AppError;
    match err {
        AppError::ActualAccountNotFound(name) => {
            tracing::warn!(account = %name, %kind, "account not found");
            CliError::NotFound
        }
        AppError::ActualAccountAmbiguous { name, matches } => {
            tracing::warn!(account = %name, matches = %matches.join(", "), %kind, "account name is ambiguous");
            CliError::NotFound
        }
        other => {
            tracing::error!(?other, %kind, "operation failed");
            CliError::Failure(other.into())
        }
    }
}
