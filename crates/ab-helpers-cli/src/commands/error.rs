#[derive(Debug)]
pub enum CliError {
    NotFound(String),
    Failure(anyhow::Error),
}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        CliError::Failure(e)
    }
}

/// Maps a server `AppError` to a `CliError`, translating account-not-found
/// and ambiguous-account variants to `NotFound` (exit 1) with a message
/// listing the real account names so the user can retry with a valid one.
pub fn map_app_error(err: ab_helpers_server::error::AppError) -> CliError {
    use ab_helpers_server::error::AppError;
    match err {
        AppError::ActualAccountNotFound { name, available } => {
            tracing::warn!(account = ?name, "account not found");
            let subject = match &name {
                Some(name) => format!("Account \"{name}\""),
                None => "Account".to_string(),
            };
            CliError::NotFound(not_found_message(&subject, &available))
        }
        AppError::ActualAccountAmbiguous { name, matches } => {
            tracing::warn!(account = %name, matches = %matches.join(", "), "account name is ambiguous");
            CliError::NotFound(ambiguous_message(&format!("\"{name}\""), &matches))
        }
        other => CliError::Failure(other.into()),
    }
}

/// Same as `map_app_error` but includes a `kind` label in every log message,
/// useful when multiple services share a log stream (e.g. the interest command
/// running kia and mortgage jobs). The interest commands resolve an account
/// by a configured id rather than a name a user typed, so `name` there is
/// `None` — the message points at the config key instead.
pub fn map_app_error_labelled(
    err: ab_helpers_server::error::AppError,
    kind: &str,
) -> CliError {
    use ab_helpers_server::error::AppError;
    match err {
        AppError::ActualAccountNotFound { name, available } => {
            tracing::warn!(account = ?name, %kind, "account not found");
            let subject = match &name {
                Some(name) => format!("Account \"{name}\""),
                None => format!("Configured `{kind}` account (`actual.{kind}.account_id`)"),
            };
            CliError::NotFound(not_found_message(&subject, &available))
        }
        AppError::ActualAccountAmbiguous { name, matches } => {
            tracing::warn!(account = %name, matches = %matches.join(", "), %kind, "account name is ambiguous");
            CliError::NotFound(ambiguous_message(&format!("\"{name}\""), &matches))
        }
        other => {
            tracing::error!(?other, %kind, "operation failed");
            CliError::Failure(other.into())
        }
    }
}

fn not_found_message(subject: &str, available: &[String]) -> String {
    if available.is_empty() {
        return format!("{subject} not found.");
    }
    let list = available
        .iter()
        .map(|a| format!("  {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{subject} not found.\nAvailable accounts:\n{list}")
}

fn ambiguous_message(subject: &str, matches: &[String]) -> String {
    let list = matches
        .iter()
        .map(|m| format!("  {m}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{subject} matches more than one account:\n{list}")
}
