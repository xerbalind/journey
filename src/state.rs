use std::sync::Arc;

use axum::extract::FromRef;
use deadpool_diesel::postgres::Pool;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    /// Wakes the route processing worker
    /// None when no ors api key is provided
    pub processing: Option<Arc<Notify>>,
}

impl FromRef<AppState> for Pool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
