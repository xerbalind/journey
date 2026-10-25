use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Internal server error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.error_page().into_response()
    }
}

impl AppError {
    fn error_page(&self) -> (StatusCode, String) {
        let (status, msg) = match self {
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Internal Server Error"),
            ),
        };

        (status, msg)
    }
}
