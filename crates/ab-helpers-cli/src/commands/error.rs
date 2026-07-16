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
