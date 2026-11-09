//! Users.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub name: String,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub instance_role: String,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.instance_role == "admin"
    }
}

#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub username: &'a str,
    pub name: &'a str,
    pub instance_role: &'a str,
    pub password_hash: Option<&'a str>,
}

/// Inserts a user. Fails with a unique violation on `users_email_uidx` / `users_username_uidx`
/// if the email or username is taken (case-insensitive).
pub async fn create(executor: impl PgExecutor<'_>, user: &NewUser<'_>) -> sqlx::Result<User> {
    sqlx::query_as!(
        User,
        "INSERT INTO users (id, email, username, name, instance_role, password_hash)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, email, username, name, locale, timezone, instance_role, disabled, created_at",
        crate::new_id(),
        user.email,
        user.username,
        user.name,
        user.instance_role,
        user.password_hash,
    )
    .fetch_one(executor)
    .await
}

pub async fn get(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as!(
        User,
        "SELECT id, email, username, name, locale, timezone, instance_role, disabled, created_at
         FROM users WHERE id = $1",
        id
    )
    .fetch_optional(executor)
    .await
}

/// A user and their password hash, looked up by email or username (case-insensitive).
pub async fn find_for_login(
    executor: impl PgExecutor<'_>,
    login: &str,
) -> sqlx::Result<Option<(User, Option<String>)>> {
    let row = sqlx::query!(
        "SELECT id, email, username, name, locale, timezone, instance_role, disabled, created_at,
                password_hash
         FROM users WHERE lower(email) = lower($1) OR lower(username) = lower($1)
         LIMIT 1",
        login
    )
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|r| {
        let user = User {
            id: r.id,
            email: r.email,
            username: r.username,
            name: r.name,
            locale: r.locale,
            timezone: r.timezone,
            instance_role: r.instance_role,
            disabled: r.disabled,
            created_at: r.created_at,
        };
        (user, r.password_hash)
    }))
}

/// `true` once any admin exists.
pub async fn admin_exists(executor: impl PgExecutor<'_>) -> sqlx::Result<bool> {
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM users WHERE instance_role = 'admin') AS "exists!""#
    )
    .fetch_one(executor)
    .await?;
    Ok(exists)
}
