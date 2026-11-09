//! Boards, columns and cards.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use laneway_core::boards::{self, Actor, BoardChanges, CardChanges, ColumnChanges, NewColumn};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::error::{ApiError, ErrorBody};
use super::extract::{Body, Path};
use super::session::CurrentUser;
use crate::http::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_boards, create_board))
        .routes(routes!(get_board, update_board))
        .routes(routes!(board_history))
        .routes(routes!(create_column))
        .routes(routes!(update_column, delete_column))
        .routes(routes!(move_column))
        .routes(routes!(create_card))
        .routes(routes!(update_card, delete_card))
        .routes(routes!(move_card))
}

fn actor(user: &CurrentUser) -> Actor {
    Actor::User(user.user.id)
}

// --- response types ---

#[derive(Debug, Serialize, ToSchema)]
pub struct Board {
    id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    name: String,
    description: String,
    /// Agents (MCP) can see and use this board.
    agent_access_enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<laneway_db::boards::Board> for Board {
    fn from(b: laneway_db::boards::Board) -> Self {
        Self {
            id: b.id,
            workspace_id: b.workspace_id,
            owner_id: b.owner_id,
            name: b.name,
            description: b.description,
            agent_access_enabled: b.agent_access_enabled,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Column {
    id: Uuid,
    board_id: Uuid,
    name: String,
    /// Sort key. Columns are returned in order; clients don't need to compare these.
    position: String,
    wip_limit: Option<i32>,
    /// Cards in a done column count as completed.
    is_done: bool,
}

impl From<laneway_db::boards::Column> for Column {
    fn from(c: laneway_db::boards::Column) -> Self {
        Self {
            id: c.id,
            board_id: c.board_id,
            name: c.name,
            position: c.position,
            wip_limit: c.wip_limit,
            is_done: c.is_done,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Card {
    id: Uuid,
    board_id: Uuid,
    column_id: Uuid,
    /// Sort key within the column. Cards are returned in order.
    position: String,
    title: String,
    /// Markdown.
    description: String,
    created_by: Uuid,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<laneway_db::boards::Card> for Card {
    fn from(c: laneway_db::boards::Card) -> Self {
        Self {
            id: c.id,
            board_id: c.board_id,
            column_id: c.column_id,
            position: c.position,
            title: c.title,
            description: c.description,
            created_by: c.created_by,
            completed_at: c.completed_at,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// A board with its columns and cards, both in display order.
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardDetail {
    board: Board,
    columns: Vec<Column>,
    cards: Vec<Card>,
}

impl From<boards::BoardView> for BoardDetail {
    fn from(v: boards::BoardView) -> Self {
        Self {
            board: v.board.into(),
            columns: v.columns.into_iter().map(Into::into).collect(),
            cards: v.cards.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HistoryEntry {
    id: Uuid,
    /// `user`, `agent`, `api_key`, `system` or `sync`.
    actor_type: String,
    actor_id: Option<Uuid>,
    /// e.g. `card.move`.
    #[serde(rename = "type")]
    kind: String,
    payload: serde_json::Value,
    created_at: DateTime<Utc>,
}

// --- boards ---

/// Boards you can see.
#[utoipa::path(get, path = "/boards", tag = "boards", responses((status = OK, body = Vec<Board>)))]
async fn list_boards(State(state): State<AppState>, user: CurrentUser) -> Result<Json<Vec<Board>>, ApiError> {
    let list = boards::list(&state.pool, actor(&user)).await?;
    Ok(Json(list.into_iter().map(Into::into).collect()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NewColumnRequest {
    name: String,
    #[serde(default)]
    is_done: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBoardRequest {
    name: String,
    #[serde(default)]
    description: String,
    /// Defaults to true.
    agent_access_enabled: Option<bool>,
    /// Starting columns, left to right. The web UI sends translated defaults.
    #[serde(default)]
    columns: Vec<NewColumnRequest>,
}

/// Creates a board in your personal workspace.
#[utoipa::path(
    post, path = "/boards", tag = "boards", request_body = CreateBoardRequest,
    responses((status = CREATED, body = BoardDetail), (status = UNPROCESSABLE_ENTITY, body = ErrorBody))
)]
async fn create_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Body(body): Body<CreateBoardRequest>,
) -> Result<(StatusCode, Json<BoardDetail>), ApiError> {
    let columns: Vec<NewColumn> =
        body.columns.into_iter().map(|c| NewColumn { name: c.name, is_done: c.is_done }).collect();
    let view = boards::create(
        &state.pool,
        actor(&user),
        &body.name,
        &body.description,
        body.agent_access_enabled.unwrap_or(true),
        &columns,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(view.into())))
}

/// A board with its columns and cards.
#[utoipa::path(
    get, path = "/boards/{board_id}", tag = "boards",
    responses((status = OK, body = BoardDetail), (status = NOT_FOUND, body = ErrorBody))
)]
async fn get_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<BoardDetail>, ApiError> {
    Ok(Json(boards::get(&state.pool, actor(&user), board_id).await?.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBoardRequest {
    name: Option<String>,
    description: Option<String>,
    agent_access_enabled: Option<bool>,
}

/// Changes a board. Fields left out stay as they are.
#[utoipa::path(
    patch, path = "/boards/{board_id}", tag = "boards", request_body = UpdateBoardRequest,
    responses((status = OK, body = Board), (status = NOT_FOUND, body = ErrorBody))
)]
async fn update_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(board_id): Path<Uuid>,
    Body(body): Body<UpdateBoardRequest>,
) -> Result<Json<Board>, ApiError> {
    let changes = BoardChanges {
        name: body.name,
        description: body.description,
        agent_access_enabled: body.agent_access_enabled,
    };
    Ok(Json(boards::update(&state.pool, actor(&user), board_id, changes).await?.into()))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct HistoryQuery {
    /// How many entries, newest first (1-500, default 50).
    limit: Option<i64>,
}

/// Recent changes on a board, newest first.
#[utoipa::path(
    get, path = "/boards/{board_id}/history", tag = "boards", params(HistoryQuery),
    responses((status = OK, body = Vec<HistoryEntry>), (status = NOT_FOUND, body = ErrorBody))
)]
async fn board_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(board_id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>, ApiError> {
    let actions = boards::history(&state.pool, actor(&user), board_id, query.limit.unwrap_or(50)).await?;
    Ok(Json(
        actions
            .into_iter()
            .map(|a| HistoryEntry {
                id: a.id,
                actor_type: a.actor_type,
                actor_id: a.actor_id,
                kind: a.kind,
                payload: a.payload,
                created_at: a.created_at,
            })
            .collect(),
    ))
}

// --- columns ---

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateColumnRequest {
    name: String,
    #[serde(default)]
    is_done: bool,
    wip_limit: Option<i32>,
}

/// Adds a column at the right end of the board.
#[utoipa::path(
    post, path = "/boards/{board_id}/columns", tag = "boards", request_body = CreateColumnRequest,
    responses((status = CREATED, body = Column), (status = NOT_FOUND, body = ErrorBody))
)]
async fn create_column(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(board_id): Path<Uuid>,
    Body(body): Body<CreateColumnRequest>,
) -> Result<(StatusCode, Json<Column>), ApiError> {
    let column =
        boards::create_column(&state.pool, actor(&user), board_id, &body.name, body.is_done, body.wip_limit)
            .await?;
    Ok((StatusCode::CREATED, Json(column.into())))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateColumnRequest {
    name: Option<String>,
    is_done: Option<bool>,
    /// A number sets the limit, `null` removes it, leaving it out keeps it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<i32>)]
    wip_limit: Option<Option<i32>>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i32>>, D::Error> {
    Option::<i32>::deserialize(d).map(Some)
}

/// Changes a column. Fields left out stay as they are.
#[utoipa::path(
    patch, path = "/columns/{column_id}", tag = "boards", request_body = UpdateColumnRequest,
    responses((status = OK, body = Column), (status = NOT_FOUND, body = ErrorBody))
)]
async fn update_column(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(column_id): Path<Uuid>,
    Body(body): Body<UpdateColumnRequest>,
) -> Result<Json<Column>, ApiError> {
    let changes = ColumnChanges { name: body.name, is_done: body.is_done, wip_limit: body.wip_limit };
    Ok(Json(boards::update_column(&state.pool, actor(&user), column_id, changes).await?.into()))
}

/// Deletes an empty column.
#[utoipa::path(
    delete, path = "/columns/{column_id}", tag = "boards",
    responses(
        (status = NO_CONTENT),
        (status = CONFLICT, description = "the column still has cards", body = ErrorBody),
        (status = NOT_FOUND, body = ErrorBody),
    )
)]
async fn delete_column(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(column_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    boards::delete_column(&state.pool, actor(&user), column_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveColumnRequest {
    /// New place among the board's columns, 0 = leftmost. Past the end = last.
    index: usize,
}

/// Moves a column.
#[utoipa::path(
    post, path = "/columns/{column_id}/move", tag = "boards", request_body = MoveColumnRequest,
    responses((status = OK, body = Column), (status = NOT_FOUND, body = ErrorBody))
)]
async fn move_column(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(column_id): Path<Uuid>,
    Body(body): Body<MoveColumnRequest>,
) -> Result<Json<Column>, ApiError> {
    Ok(Json(boards::move_column(&state.pool, actor(&user), column_id, body.index).await?.into()))
}

// --- cards ---

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCardRequest {
    title: String,
    #[serde(default)]
    description: String,
}

/// Adds a card at the bottom of a column.
#[utoipa::path(
    post, path = "/columns/{column_id}/cards", tag = "boards", request_body = CreateCardRequest,
    responses((status = CREATED, body = Card), (status = NOT_FOUND, body = ErrorBody))
)]
async fn create_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(column_id): Path<Uuid>,
    Body(body): Body<CreateCardRequest>,
) -> Result<(StatusCode, Json<Card>), ApiError> {
    let card =
        boards::create_card(&state.pool, actor(&user), column_id, &body.title, &body.description).await?;
    Ok((StatusCode::CREATED, Json(card.into())))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCardRequest {
    title: Option<String>,
    description: Option<String>,
}

/// Changes a card. Fields left out stay as they are.
#[utoipa::path(
    patch, path = "/cards/{card_id}", tag = "boards", request_body = UpdateCardRequest,
    responses((status = OK, body = Card), (status = NOT_FOUND, body = ErrorBody))
)]
async fn update_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
    Body(body): Body<UpdateCardRequest>,
) -> Result<Json<Card>, ApiError> {
    let changes = CardChanges { title: body.title, description: body.description };
    Ok(Json(boards::update_card(&state.pool, actor(&user), card_id, changes).await?.into()))
}

/// Deletes a card.
#[utoipa::path(
    delete, path = "/cards/{card_id}", tag = "boards",
    responses((status = NO_CONTENT), (status = NOT_FOUND, body = ErrorBody))
)]
async fn delete_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    boards::delete_card(&state.pool, actor(&user), card_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveCardRequest {
    /// Target column, on the same board.
    column_id: Uuid,
    /// New place in that column, 0 = top. Past the end = bottom.
    index: usize,
}

/// Moves a card within or between columns.
#[utoipa::path(
    post, path = "/cards/{card_id}/move", tag = "boards", request_body = MoveCardRequest,
    responses((status = OK, body = Card), (status = NOT_FOUND, body = ErrorBody))
)]
async fn move_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(card_id): Path<Uuid>,
    Body(body): Body<MoveCardRequest>,
) -> Result<Json<Card>, ApiError> {
    Ok(Json(boards::move_card(&state.pool, actor(&user), card_id, body.column_id, body.index).await?.into()))
}
