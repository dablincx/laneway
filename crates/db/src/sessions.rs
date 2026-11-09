//! Login sessions. Only token hashes are stored.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use sqlx::types::ipnetwork::IpNetwork;
use uuid::Uuid;

use crate::users::User;

#[derive(Debug, Clone)]
pub struct NewSession<'a> {
    pub user_id: Uuid,
    pub token_hash: &'a [u8],
    pub user_agent: Option<&'a str>,
    pub ip: Option<IpAddr>,
    pub expires_at: DateTime<Utc>,
}

pub async fn create(executor: impl PgExecutor<'_>, session: &NewSession<'_>) -> sqlx::Result<Uuid> {
    sqlx::query_scalar!(
        "INSERT INTO sessions (id, user_id, token_hash, user_agent, ip, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        crate::new_id(),
        session.user_id,
        session.token_hash,
        session.user_agent,
        session.ip.map(IpNetwork::from),
        session.expires_at,
    )
    .fetch_one(executor)
    .await
}

/// The user behind a live session token hash. Expired sessions and disabled users don't count.
/// Also bumps `last_seen_at` (at most once a minute, to keep writes down).
pub async fn user_for_token(
    executor: impl PgExecutor<'_>,
    token_hash: &[u8],
) -> sqlx::Result<Option<(Uuid, User)>> {
    let row = sqlx::query!(
        "WITH s AS (
             SELECT id, user_id, last_seen_at FROM sessions WHERE token_hash = $1 AND expires_at > now()
         ), bump AS (
             UPDATE sessions SET last_seen_at = now()
             WHERE id IN (SELECT id FROM s WHERE last_seen_at < now() - interval '1 minute')
         )
         SELECT s.id AS session_id, u.id, u.email, u.username, u.name, u.locale, u.timezone,
                u.instance_role, u.disabled, u.created_at
         FROM s JOIN users u ON u.id = s.user_id
         WHERE NOT u.disabled",
        token_hash
    )
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|r| {
        (
            r.session_id,
            User {
                id: r.id,
                email: r.email,
                username: r.username,
                name: r.name,
                locale: r.locale,
                timezone: r.timezone,
                instance_role: r.instance_role,
                disabled: r.disabled,
                created_at: r.created_at,
            },
        )
    }))
}

pub async fn delete_by_token(executor: impl PgExecutor<'_>, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", token_hash).execute(executor).await?;
    Ok(())
}

pub async fn delete_expired(executor: impl PgExecutor<'_>) -> sqlx::Result<u64> {
    let done = sqlx::query!("DELETE FROM sessions WHERE expires_at <= now()").execute(executor).await?;
    Ok(done.rows_affected())
}
