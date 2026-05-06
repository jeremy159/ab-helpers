use budgetize_domain::db::DbError;

use crate::error::AppError;

pub(crate) enum ErrorType {
    NotFound,
    AlreadyExists,
    Database,
}

pub(crate) fn assert_err(err: AppError, expected_err: Option<ErrorType>) {
    match expected_err {
        Some(ErrorType::NotFound) => {
            assert!(
                matches!(err, AppError::ResourceNotFound)
                    || matches!(err, AppError::DbError(DbError::NotFound))
            );
        }
        Some(ErrorType::AlreadyExists) => {
            assert!(
                matches!(err, AppError::ResourceAlreadyExists)
                    || matches!(err, AppError::DbError(DbError::AlreadyExists))
            );
        }
        Some(ErrorType::Database) => assert!(matches!(err, AppError::DbError(_))),
        None => unreachable!(),
    }
}
