//! Checklists on cards and their items.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use laneway_core::boards::Actor;
use laneway_core::checklists;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::error::{ApiError, ErrorBody};
use super::extract::{Body, Path};
use super::session::CurrentUser;
use crate::http::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_checklist))
        .routes(routes!(update_checklist, delete_checklist))
        .routes(routes!(add_item))
        .routes(routes!(update_item, delete_item))
        .routes(routes!(move_item))
}

fn actor(user: &CurrentUser) -> Actor {
    Actor::User(user.user.id)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Checklist {
    id: Uuid,
    card_id: Uuid,
    title: String,
    /// Sort key among the card's checklists. Checklists are returned in order.
    position: String,
}

impl From<laneway_db::checklists::Checklist> for Checklist {
    fn from(c: laneway_db::checklists::Checklist) -> Self {
        Self { id: c.id, card_id: c.card_id, title: c.title, position: c.position }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChecklistItem {
    id: Uuid,
    checklist_id: Uuid,
    text: String,
    done: bool,
    /// Sort key within the checklist. Items are returned in order.
    position: String,
}

impl From<laneway_db::checklists::Item> for ChecklistItem {
    fn from(i: laneway_db::checklists::Item) -> Self {
        Self { id: i.id, checklist_id: i.checklist_id, text: i.text, done: i.done, position: i.position }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChecklistRequest {
    /// 1 to 200 characters.
    title: String,
}

/// Adds a checklist at the end of a card's checklists.
#[utoipa::path(
    post, path = "/cards/{card_id}/checklists", tag = "checklists", request_body = ChecklistRequest,
    responses((status = CREATED, body = Checklist), (status = NOT_FOUND, body = ErrorBody))
)]
async fn create_checklist(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
    Body(body): Body<ChecklistRequest>,
) -> Result<(StatusCode, Json<Checklist>), ApiError> {
    let checklist = checklists::create(&state.pool, actor(&user), card_id, &body.title).await?;
    Ok((StatusCode::CREATED, Json(checklist.into())))
}

/// Renames a checklist.
#[utoipa::path(
    patch, path = "/checklists/{checklist_id}", tag = "checklists", request_body = ChecklistRequest,
    responses((status = OK, body = Checklist), (status = NOT_FOUND, body = ErrorBody))
)]
async fn update_checklist(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(checklist_id): Path<Uuid>,
    Body(body): Body<ChecklistRequest>,
) -> Result<Json<Checklist>, ApiError> {
    Ok(Json(checklists::rename(&state.pool, actor(&user), checklist_id, &body.title).await?.into()))
}

/// Deletes a checklist and its items.
#[utoipa::path(
    delete, path = "/checklists/{checklist_id}", tag = "checklists",
    responses((status = NO_CONTENT), (status = NOT_FOUND, body = ErrorBody))
)]
async fn delete_checklist(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(checklist_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    checklists::delete(&state.pool, actor(&user), checklist_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddItemRequest {
    /// 1 to 500 characters.
    text: String,
}

/// Adds an item at the end of a checklist.
#[utoipa::path(
    post, path = "/checklists/{checklist_id}/items", tag = "checklists", request_body = AddItemRequest,
    responses((status = CREATED, body = ChecklistItem), (status = NOT_FOUND, body = ErrorBody))
)]
async fn add_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(checklist_id): Path<Uuid>,
    Body(body): Body<AddItemRequest>,
) -> Result<(StatusCode, Json<ChecklistItem>), ApiError> {
    let item = checklists::add_item(&state.pool, actor(&user), checklist_id, &body.text).await?;
    Ok((StatusCode::CREATED, Json(item.into())))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateItemRequest {
    text: Option<String>,
    done: Option<bool>,
}

/// Changes an item's text or ticks it. Fields left out stay as they are.
#[utoipa::path(
    patch, path = "/checklist-items/{item_id}", tag = "checklists", request_body = UpdateItemRequest,
    responses((status = OK, body = ChecklistItem), (status = NOT_FOUND, body = ErrorBody))
)]
async fn update_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(item_id): Path<Uuid>,
    Body(body): Body<UpdateItemRequest>,
) -> Result<Json<ChecklistItem>, ApiError> {
    let item =
        checklists::update_item(&state.pool, actor(&user), item_id, body.text.as_deref(), body.done).await?;
    Ok(Json(item.into()))
}

/// Deletes an item.
#[utoipa::path(
    delete, path = "/checklist-items/{item_id}", tag = "checklists",
    responses((status = NO_CONTENT), (status = NOT_FOUND, body = ErrorBody))
)]
async fn delete_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(item_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    checklists::delete_item(&state.pool, actor(&user), item_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveItemRequest {
    /// New place in the checklist, 0 = top. Past the end = bottom.
    index: usize,
}

/// Moves an item within its checklist.
#[utoipa::path(
    post, path = "/checklist-items/{item_id}/move", tag = "checklists", request_body = MoveItemRequest,
    responses((status = OK, body = ChecklistItem), (status = NOT_FOUND, body = ErrorBody))
)]
async fn move_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(item_id): Path<Uuid>,
    Body(body): Body<MoveItemRequest>,
) -> Result<Json<ChecklistItem>, ApiError> {
    Ok(Json(checklists::move_item(&state.pool, actor(&user), item_id, body.index).await?.into()))
}
