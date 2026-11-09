//! Request extractors that fail with our error format (`{"error": {"code", "message"}}`)
//! instead of axum's plain-text rejections, so clients always get a stable `code`.

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::error::ApiError;

/// Like `axum::extract::Path`. A malformed id gives 400 `invalid_request`.
#[derive(Debug, Clone, Copy)]
pub struct Path<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for Path<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(path_error(&rejection)),
        }
    }
}

/// Like `axum::Json` as a request body. Bad JSON gives 400 `invalid_request`, a wrong content
/// type 415 `unsupported_media_type`, a body that doesn't fit the schema 422 `invalid_request`.
#[derive(Debug, Clone, Copy)]
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_error(&rejection)),
        }
    }
}

fn path_error(rejection: &PathRejection) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "invalid_request", rejection.body_text())
}

fn json_error(rejection: &JsonRejection) -> ApiError {
    let code = if rejection.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
        "unsupported_media_type"
    } else {
        "invalid_request"
    };
    ApiError::new(rejection.status(), code, rejection.body_text())
}
