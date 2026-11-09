//! First-launch setup token.

use sqlx::PgExecutor;

/// Replaces any existing setup token with this one.
pub async fn replace_token(executor: impl PgExecutor<'_>, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query!(
        "WITH gone AS (DELETE FROM setup_tokens) INSERT INTO setup_tokens (token_hash) VALUES ($1)",
        token_hash
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Deletes the token if it matches. `true` = it matched (so it can't be used twice).
pub async fn consume_token(executor: impl PgExecutor<'_>, token_hash: &[u8]) -> sqlx::Result<bool> {
    let done =
        sqlx::query!("DELETE FROM setup_tokens WHERE token_hash = $1", token_hash).execute(executor).await?;
    Ok(done.rows_affected() == 1)
}

pub async fn clear(executor: impl PgExecutor<'_>) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM setup_tokens").execute(executor).await?;
    Ok(())
}
