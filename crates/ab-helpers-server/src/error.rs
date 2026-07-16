use ab_helpers_domain::db::DbError;
use axum::{
    extract::{FromRequest, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub type ABHelpersResult<T> = Result<T, AppError>;
pub type HttpJsonResult<T> = Result<AppJson<T>, AppError>;

#[derive(FromRequest, Debug)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct AppJson<T>(pub T);

impl<T> IntoResponse for AppJson<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[derive(thiserror::Error)]
pub enum AppError {
    #[error("The request body contained invalid JSON")]
    JsonRejection(#[from] JsonRejection),
    #[error("Resource does not exist")]
    ResourceNotFound,
    #[error("Resource already exists")]
    ResourceAlreadyExists,
    #[error("Error with Database interaction")]
    DbError(#[from] DbError),
    #[error("Actual account `{0}` not found")]
    ActualAccountNotFound(String),
    #[error("Multiple Actual accounts match `{name}`: {}", .matches.join(", "))]
    ActualAccountAmbiguous { name: String, matches: Vec<String> },
    #[error("Actual integration error: {0}")]
    Actual(#[from] actual::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = ?self, "error encountered");

        #[derive(Serialize)]
        struct ErrorResponse {
            message: String,
        }

        let (status, message) = match self {
            AppError::JsonRejection(rejection) => (rejection.status(), rejection.body_text()),
            AppError::ResourceNotFound => {
                (StatusCode::NOT_FOUND, "Resource does not exist".to_owned())
            }
            AppError::ResourceAlreadyExists => {
                (StatusCode::CONFLICT, "Resource already exists".to_owned())
            }
            AppError::ActualAccountNotFound(_) => {
                (StatusCode::NOT_FOUND, "Account does not exist".to_owned())
            }
            AppError::ActualAccountAmbiguous { .. } => (
                StatusCode::BAD_REQUEST,
                "Account name matches multiple accounts".to_owned(),
            ),
            AppError::Actual(_) => (
                StatusCode::BAD_GATEWAY,
                "Actual integration failed".to_owned(),
            ),
            AppError::DbError(err) => match err {
                DbError::NotFound => (StatusCode::NOT_FOUND, "Resource does not exist".to_owned()),
                DbError::AlreadyExists => {
                    (StatusCode::CONFLICT, "Resource already exists".to_owned())
                }
                DbError::DataIntegrityError(_) => (
                    StatusCode::BAD_REQUEST,
                    "Data is corrupted or invalid".to_owned(),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Something went wrong".to_owned(),
                ),
            },
            AppError::Unexpected(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong".to_owned(),
            ),
        };

        (status, AppJson(ErrorResponse { message })).into_response()
    }
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{:?}", cause)?;
        current = cause.source();
    }
    Ok(())
}
