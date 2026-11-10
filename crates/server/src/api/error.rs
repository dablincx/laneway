//! The one error format every endpoint uses:
//! `{"error": {"code": "invalid_credentials", "message": "..."}}`.
//! `code` is stable and meant for clients; `message` is for humans and may change.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use laneway_core::Error;
use laneway_core::accounts::Invalid;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable machine-readable code, e.g. `invalid_credentials`.
    pub code: String,
    /// Human-readable explanation (English for now).
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }

    pub fn unauthenticated() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthenticated", "log in first")
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody { error: ErrorDetail { code: self.code.to_owned(), message: self.message } };
        (self.status, Json(body)).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        use StatusCode as S;
        match error {
            Error::Invalid(field) => {
                let (code, message) = match field {
                    Invalid::Email => ("invalid_email", "that doesn't look like an email address"),
                    Invalid::Username => (
                        "invalid_username",
                        "usernames are 2-32 characters: letters, digits, _ . -, starting with a letter or digit",
                    ),
                    Invalid::Name => ("invalid_name", "the name must be 1-100 characters"),
                    Invalid::PasswordTooShort => {
                        ("password_too_short", "the password needs at least 10 characters")
                    }
                    Invalid::PasswordTooLong => {
                        ("password_too_long", "the password can have at most 512 characters")
                    }
                    Invalid::BoardName => ("invalid_board_name", "board names are 1-200 characters"),
                    Invalid::ColumnName => ("invalid_column_name", "column names are 1-200 characters"),
                    Invalid::CardTitle => ("invalid_card_title", "card titles are 1-500 characters"),
                    Invalid::Description => {
                        ("description_too_long", "descriptions can have at most 100000 characters")
                    }
                    Invalid::WipLimit => ("invalid_wip_limit", "the WIP limit must be at least 1"),
                    Invalid::Position => ("invalid_position", "can't place it there"),
                    Invalid::Priority => {
                        ("invalid_priority", "the priority must be low, medium, high or urgent")
                    }
                    Invalid::Dates => ("invalid_dates", "the start date is after the due date"),
                    Invalid::Estimate => ("invalid_estimate", "the estimate must be between 0 and 100000"),
                    Invalid::LabelName => ("invalid_label_name", "label names are 1-50 characters"),
                    Invalid::LabelColor => ("invalid_label_color", "that isn't one of the label colors"),
                    Invalid::ChecklistTitle => {
                        ("invalid_checklist_title", "checklist titles are 1-200 characters")
                    }
                    Invalid::ItemText => ("invalid_item_text", "checklist items are 1-500 characters"),
                };
                Self::new(S::UNPROCESSABLE_ENTITY, code, message)
            }
            Error::InvalidCredentials => {
                Self::new(S::UNAUTHORIZED, "invalid_credentials", "wrong login or password")
            }
            Error::InvalidSetupToken => {
                Self::new(S::FORBIDDEN, "invalid_setup_token", "the setup token is wrong or was already used")
            }
            Error::SetupDone => Self::new(S::CONFLICT, "setup_done", "setup is already done"),
            Error::EmailTaken => Self::new(S::CONFLICT, "email_taken", "that email is already used"),
            Error::UsernameTaken => {
                Self::new(S::CONFLICT, "username_taken", "that username is already taken")
            }
            Error::NotFound => Self::new(S::NOT_FOUND, "not_found", "not found"),
            Error::ColumnNotEmpty => {
                Self::new(S::CONFLICT, "column_not_empty", "move or delete the cards in this column first")
            }
            Error::LabelTaken => {
                Self::new(S::CONFLICT, "label_taken", "the board already has a label with that name")
            }
            Error::Auth(_) | Error::Database(_) => {
                tracing::error!("internal error: {error}");
                Self::new(S::INTERNAL_SERVER_ERROR, "internal", "something went wrong on the server")
            }
        }
    }
}
