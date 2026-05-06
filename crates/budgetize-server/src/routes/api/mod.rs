use axum::Router;

use crate::startup::AppState;

// Add feature-specific route modules here, e.g.:
// mod budget;

pub fn get_api_routes(_app_state: &AppState) -> Router<AppState> {
    Router::new()
    // .nest("/budget", get_budget_routes(app_state))
}
