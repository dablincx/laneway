//! Workspaces. Only personal ones for now.

use sqlx::PgExecutor;
use uuid::Uuid;

pub async fn create_personal(
    executor: impl PgExecutor<'_>,
    owner_id: Uuid,
    name: &str,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar!(
        "INSERT INTO workspaces (id, name, owner_id, is_personal) VALUES ($1, $2, $3, true) RETURNING id",
        crate::new_id(),
        name,
        owner_id
    )
    .fetch_one(executor)
    .await
}

pub async fn personal_id(executor: impl PgExecutor<'_>, owner_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar!("SELECT id FROM workspaces WHERE owner_id = $1 AND is_personal", owner_id)
        .fetch_optional(executor)
        .await
}
