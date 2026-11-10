//! laneway: the service layer. REST, MCP and websockets all call these functions, so
//! validation, permission checks and recorded actions happen in exactly one place.

pub mod accounts;
pub mod boards;
pub mod crypto;
pub mod position;
pub mod settings;

/// Errors the service layer returns. The API maps each to a status code and a stable error code.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid input: {0:?}")]
    Invalid(accounts::Invalid),
    #[error("wrong login or password")]
    InvalidCredentials,
    #[error("setup token is wrong or was already used")]
    InvalidSetupToken,
    #[error("setup is already done")]
    SetupDone,
    #[error("email is already taken")]
    EmailTaken,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("not found")]
    NotFound,
    #[error("the column still has cards")]
    ColumnNotEmpty,
    #[error("the board already has a label with that name")]
    LabelTaken,
    #[error(transparent)]
    Auth(#[from] laneway_auth::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
