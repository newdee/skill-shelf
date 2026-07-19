use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// HTTP-facing error: a status code plus a message rendered as `{"error": ...}`.
pub struct ApiError {
    pub code: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            code: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            code: StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            code: StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.code, Json(json!({ "error": self.message }))).into_response()
    }
}

/// Map config write errors onto HTTP status codes.
impl From<crate::config::ConfigError> for ApiError {
    fn from(e: crate::config::ConfigError) -> Self {
        use crate::config::ConfigError::*;
        match e {
            BadRequest(m) => ApiError::bad_request(m),
            NotFound(m) => ApiError::not_found(m),
            Internal(m) => ApiError::internal(m),
        }
    }
}

/// Map core errors onto HTTP status codes.
impl From<skill_shelf_core::Error> for ApiError {
    fn from(e: skill_shelf_core::Error) -> Self {
        use skill_shelf_core::Error;
        let code = match &e {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            code,
            message: e.to_string(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
