//! Stable played-event identities, independent of engine analysis runs.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::json_errors::{Json, Path};
use anyhow::{Context, Result};
use axum::{Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct StoredMove {
    pub id: Uuid,
    pub match_uuid: Uuid,
    pub game_no: i32,
    /// One-based actual source event, including explicit cube actions.
    pub move_no: i32,
    #[schema(value_type=String, examples("checker", "cube", "resign"))]
    pub kind: String,
    pub player: i32,
    pub notation: String,
    /// Checker notes use this canonical position occurrence; cube notes use id.
    pub position_uuid: Option<Uuid>,
    pub board: Vec<i32>,
    pub dice: Vec<i32>,
    pub position_id: String,
    pub gnubg_match_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GameDecisions {
    pub game_id: Uuid,
    pub game_no: i32,
    pub items: Vec<StoredMove>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/matches/{id}/games/{game_no}/decisions",
            get(game_decisions),
        )
        .route("/api/moves/{id}", get(get_move))
}

/// Lazily catalog once under the immutable source's match lock. The completion
/// marker also records empty games without relying on the presence of rows.
pub async fn ensure_catalog(pool: &PgPool, match_id: Uuid) -> Result<bool> {
    // Completed catalogs must not fetch the potentially large SGF artifact or
    // serialize ordinary game navigation behind a write lock.
    match sqlx::query_scalar::<_, bool>("SELECT moves_cataloged FROM matches WHERE id=$1")
        .bind(match_id)
        .fetch_optional(pool)
        .await?
    {
        None => return Ok(false),
        Some(true) => return Ok(true),
        Some(false) => {}
    }
    let mut tx = pool.begin().await?;
    let source: Option<(String, bool)> =
        sqlx::query_as("SELECT sgf,moves_cataloged FROM matches WHERE id=$1 FOR UPDATE")
            .bind(match_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((source, complete)) = source else {
        return Ok(false);
    };
    if complete {
        tx.commit().await?;
        return Ok(true);
    }
    let (imported, moves) =
        tokio::task::spawn_blocking(move || crate::import::sgf::parse_sgf_source_moves(&source))
            .await??;
    for game in imported.games {
        sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(match_uuid,game_no) DO NOTHING")
            .bind(match_id).bind(game.game_no).bind(game.position_count).bind(game.white_score).bind(game.black_score).bind(game.crawford).execute(&mut *tx).await?;
    }
    let positions: Vec<(Uuid, i32, i32)> =
        sqlx::query_as("SELECT id,game_no,ply FROM positions WHERE match_uuid=$1")
            .bind(match_id)
            .fetch_all(&mut *tx)
            .await?;
    let positions: HashMap<_, _> = positions
        .into_iter()
        .map(|(id, game, ply)| ((game, ply), id))
        .collect();
    let moves = moves
        .into_iter()
        .map(|event| {
            let position_uuid = event
                .checker_ply
                .map(|ply| {
                    positions
                        .get(&(event.game_no, ply))
                        .copied()
                        .context("Source checker position is missing from the library")
                })
                .transpose()?;
            Ok((event, position_uuid))
        })
        .collect::<Result<Vec<_>>>()?;
    for batch in moves.chunks(500) {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO match_moves(match_uuid,game_no,move_no,kind,player,notation,position_uuid,board,dice,position_id,gnubg_match_id) ",
        );
        query.push_values(batch, |mut row, (event, position_uuid)| {
            row.push_bind(match_id)
                .push_bind(event.game_no)
                .push_bind(event.move_no)
                .push_bind(event.kind)
                .push_bind(event.player)
                .push_bind(&event.notation)
                .push_bind(*position_uuid)
                .push_bind(&event.board)
                .push_bind(event.dice.to_vec())
                .push_bind(&event.position_id)
                .push_bind(&event.match_id);
        });
        query.push(" ON CONFLICT(match_uuid,game_no,move_no) DO NOTHING");
        query.build().execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE matches SET moves_cataloged=true WHERE id=$1")
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

#[utoipa::path(get,path="/api/matches/{id}/games/{game_no}/decisions",params(("id"=Uuid,Path),("game_no"=i32,Path)),responses((status=200,body=GameDecisions),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Library")]
async fn game_decisions(
    State(state): State<AppState>,
    Path((id, game_no)): Path<(Uuid, i32)>,
) -> ApiResult<Json<GameDecisions>> {
    crate::play::require_review_available(&state.pool, id).await?;
    if !ensure_catalog(&state.pool, id).await? {
        return Err(ApiError::missing());
    }
    let game_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=$2")
            .bind(id)
            .bind(game_no)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    let game_id = game_id.ok_or_else(ApiError::missing)?;
    let items = sqlx::query_as(
        "SELECT * FROM match_moves WHERE match_uuid=$1 AND game_no=$2 ORDER BY move_no",
    )
    .bind(id)
    .bind(game_no)
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(GameDecisions {
        game_id,
        game_no,
        items,
    }))
}

#[utoipa::path(get,path="/api/moves/{id}",params(("id"=Uuid,Path)),responses((status=200,body=StoredMove),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Library")]
async fn get_move(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StoredMove>> {
    let event: Option<StoredMove> = sqlx::query_as("SELECT * FROM match_moves WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let event = event.ok_or_else(ApiError::missing)?;
    crate::play::require_review_available(&state.pool, event.match_uuid).await?;
    Ok(Json(event))
}

#[derive(OpenApi)]
#[openapi(
    paths(game_decisions, get_move),
    components(schemas(StoredMove, GameDecisions))
)]
pub struct MoveCatalogDoc;
