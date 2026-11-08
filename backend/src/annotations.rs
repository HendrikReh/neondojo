//! Personal plaintext Markdown notes and tags, addressed by imported target UUIDs.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use anyhow::{Result, bail};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::collections::BTreeSet;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum AnnotationKind {
    Match,
    Position,
    Game,
    Move,
}

impl AnnotationKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "match" => Ok(Self::Match),
            "position" => Ok(Self::Position),
            "game" => Ok(Self::Game),
            "move" => Ok(Self::Move),
            _ => bail!("kind must be match, position, game or move"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Position => "position",
            Self::Game => "game",
            Self::Move => "move",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Annotation {
    /// Null for an existing target without a saved annotation.
    pub id: Option<Uuid>,
    pub target_kind: AnnotationKind,
    pub target_id: Uuid,
    pub tags: Vec<String>,
    /// Plaintext Markdown; never interpreted as trusted HTML.
    pub comment: String,
    /// Zero until first save. Supply this value as expected_version when saving.
    pub version: i64,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AnnotationUpdate {
    pub tags: Vec<String>,
    pub comment: String,
    pub expected_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct AnnotationHit {
    pub id: Uuid,
    pub target_kind: AnnotationKind,
    pub target_id: Uuid,
    pub match_uuid: Uuid,
    pub filename: String,
    pub white: String,
    pub black: String,
    pub game_no: Option<i32>,
    pub ply: Option<i32>,
    pub tags: Vec<String>,
    pub comment_excerpt: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AnnotationPage {
    pub items: Vec<AnnotationHit>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Deserialize, IntoParams)]
struct SearchQuery {
    /// Literal substring, 3–200 characters. Omit only when supplying an exact tag.
    q: Option<String>,
    /// Exact normalized tag; can be combined with the text and kind filters.
    tag: Option<String>,
    kind: Option<AnnotationKind>,
    after: Option<Uuid>,
    limit: Option<i64>,
    /// Restrict all note scopes to one logical database.
    database_id: Option<Uuid>,
}

#[derive(Deserialize, IntoParams)]
struct TagQuery {
    /// Optional literal tag substring, at most 64 characters.
    q: Option<String>,
    limit: Option<i64>,
    database_id: Option<Uuid>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/matches/{id}/annotation",
            get(get_match_annotation).put(put_match_annotation),
        )
        .route(
            "/api/positions/{id}/annotation",
            get(get_position_annotation).put(put_position_annotation),
        )
        .route(
            "/api/games/{id}/annotation",
            get(get_game_annotation).put(put_game_annotation),
        )
        .route(
            "/api/moves/{id}/annotation",
            get(get_move_annotation).put(put_move_annotation),
        )
        .route("/api/annotations/search", get(search_annotations))
        .route("/api/tags", get(tags))
}

const ANNOTATION_COLUMNS: &str = "id,target_kind,target_id,tags,comment,version,updated_at";

async fn target_exists(pool: &PgPool, kind: AnnotationKind, id: Uuid) -> Result<bool> {
    let sql = match kind {
        AnnotationKind::Match => "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1)",
        AnnotationKind::Position => "SELECT EXISTS(SELECT 1 FROM positions WHERE id=$1)",
        AnnotationKind::Game => "SELECT EXISTS(SELECT 1 FROM match_games WHERE id=$1)",
        AnnotationKind::Move => "SELECT EXISTS(SELECT 1 FROM match_moves WHERE id=$1)",
    };
    Ok(sqlx::query_scalar(sql).bind(id).fetch_one(pool).await?)
}

/// Checker source events share the original position occurrence's annotation.
/// Every other played event has its own independent note.
async fn canonical_target(
    pool: &PgPool,
    kind: AnnotationKind,
    id: Uuid,
) -> Result<(AnnotationKind, Uuid)> {
    if kind == AnnotationKind::Move {
        let position: Option<Uuid> = sqlx::query_scalar(
            "SELECT position_uuid FROM match_moves WHERE id=$1 AND kind='checker'",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .flatten();
        if let Some(position) = position {
            return Ok((AnnotationKind::Position, position));
        }
    }
    Ok((kind, id))
}

/// Fetch notes for the learning/export pipeline.
/// Existing unannotated targets return empty notes and version zero.
pub async fn get_annotation(pool: &PgPool, kind: &str, id: Uuid) -> Result<Annotation> {
    let (kind, id) = canonical_target(pool, AnnotationKind::parse(kind)?, id).await?;
    if !target_exists(pool, kind, id).await? {
        bail!("Annotation target not found");
    }
    load_annotation(pool, kind, id).await
}

async fn load_annotation(pool: &PgPool, kind: AnnotationKind, id: Uuid) -> Result<Annotation> {
    let row = sqlx::query_as::<_, Annotation>(&format!(
        "SELECT {ANNOTATION_COLUMNS} FROM annotations WHERE target_kind=$1 AND target_id=$2"
    ))
    .bind(kind.as_str())
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or(Annotation {
        id: None,
        target_kind: kind,
        target_id: id,
        tags: vec![],
        comment: String::new(),
        version: 0,
        updated_at: None,
    }))
}

async fn get_target_annotation(
    state: AppState,
    kind: AnnotationKind,
    id: Uuid,
) -> ApiResult<Json<Annotation>> {
    let (kind, id) = canonical_target(&state.pool, kind, id).await?;
    if !target_exists(&state.pool, kind, id).await? {
        return Err(ApiError::missing());
    }
    Ok(Json(load_annotation(&state.pool, kind, id).await?))
}

#[utoipa::path(get, path="/api/matches/{id}/annotation", params(("id"=Uuid, Path)), responses((status=200,body=Annotation),(status=404,body=ErrorBody)), tag="Annotations")]
async fn get_match_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Annotation>> {
    get_target_annotation(state, AnnotationKind::Match, id).await
}

#[utoipa::path(get, path="/api/positions/{id}/annotation", params(("id"=Uuid, Path)), responses((status=200,body=Annotation),(status=404,body=ErrorBody)), tag="Annotations")]
async fn get_position_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Annotation>> {
    get_target_annotation(state, AnnotationKind::Position, id).await
}

#[utoipa::path(get, path="/api/games/{id}/annotation", params(("id"=Uuid, Path)), responses((status=200,body=Annotation),(status=404,body=ErrorBody)), tag="Annotations")]
async fn get_game_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Annotation>> {
    get_target_annotation(state, AnnotationKind::Game, id).await
}

#[utoipa::path(get, path="/api/moves/{id}/annotation", params(("id"=Uuid, Path)), responses((status=200,body=Annotation),(status=404,body=ErrorBody)), tag="Annotations")]
async fn get_move_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Annotation>> {
    get_target_annotation(state, AnnotationKind::Move, id).await
}

fn normalize_tag(tag: &str) -> ApiResult<String> {
    if tag.chars().any(char::is_control) {
        return Err(ApiError::invalid("Tags cannot contain control characters"));
    }
    let tag = tag
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if tag.chars().count() > 64 {
        return Err(ApiError::invalid("Tags must be at most 64 characters"));
    }
    Ok(tag)
}

fn validate_update(mut update: AnnotationUpdate) -> ApiResult<AnnotationUpdate> {
    if update.expected_version < 0 {
        return Err(ApiError::invalid("expected_version must be nonnegative"));
    }
    if update.comment.chars().count() > 20_000
        || update
            .comment
            .chars()
            .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
    {
        return Err(ApiError::invalid(
            "Comments must be at most 20000 characters and cannot contain control characters other than line breaks and tabs",
        ));
    }
    let mut tags = BTreeSet::new();
    for tag in update.tags {
        let normalized = normalize_tag(&tag)?;
        if !normalized.is_empty() {
            tags.insert(normalized);
        }
    }
    if tags.len() > 30 {
        return Err(ApiError::invalid("At most 30 tags are allowed"));
    }
    update.tags = tags.into_iter().collect();
    Ok(update)
}

async fn put_target_annotation(
    state: AppState,
    kind: AnnotationKind,
    id: Uuid,
    payload: Result<Json<AnnotationUpdate>, JsonRejection>,
) -> ApiResult<Json<Annotation>> {
    let (kind, id) = canonical_target(&state.pool, kind, id).await?;
    let Json(update) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    let update = validate_update(update)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Hold the parent alive while saving, including the first annotation insert.
    let lock_sql = match kind {
        AnnotationKind::Match => "SELECT id FROM matches WHERE id=$1 FOR KEY SHARE",
        AnnotationKind::Position => "SELECT id FROM positions WHERE id=$1 FOR KEY SHARE",
        AnnotationKind::Game => "SELECT id FROM match_games WHERE id=$1 FOR KEY SHARE",
        AnnotationKind::Move => "SELECT id FROM match_moves WHERE id=$1 FOR KEY SHARE",
    };
    let target: Option<Uuid> = sqlx::query_scalar(lock_sql)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if target.is_none() {
        return Err(ApiError::missing());
    }
    let saved = if update.expected_version == 0 {
        sqlx::query_as::<_, Annotation>(&format!("INSERT INTO annotations(id,match_uuid,position_uuid,game_uuid,move_uuid,tags,comment,version) VALUES($1,$2,$3,$4,$5,$6,$7,1) ON CONFLICT(target_kind,target_id) DO NOTHING RETURNING {ANNOTATION_COLUMNS}"))
            .bind(Uuid::new_v4())
            .bind(if kind == AnnotationKind::Match { Some(id) } else { None })
            .bind(if kind == AnnotationKind::Position { Some(id) } else { None })
            .bind(if kind == AnnotationKind::Game { Some(id) } else { None })
            .bind(if kind == AnnotationKind::Move { Some(id) } else { None })
            .bind(&update.tags).bind(&update.comment).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?
    } else {
        sqlx::query_as::<_, Annotation>(&format!("UPDATE annotations SET tags=$1,comment=$2,version=version+1,updated_at=clock_timestamp() WHERE target_kind=$3 AND target_id=$4 AND version=$5 RETURNING {ANNOTATION_COLUMNS}"))
            .bind(&update.tags).bind(&update.comment).bind(kind.as_str()).bind(id).bind(update.expected_version)
            .fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?
    };
    let saved = saved.ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "Annotation changed; reload before saving".into(),
        )
    })?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(saved))
}

#[utoipa::path(put, path="/api/matches/{id}/annotation", params(("id"=Uuid, Path)), request_body=AnnotationUpdate, responses((status=200,body=Annotation),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Annotations")]
async fn put_match_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<AnnotationUpdate>, JsonRejection>,
) -> ApiResult<Json<Annotation>> {
    put_target_annotation(state, AnnotationKind::Match, id, payload).await
}

#[utoipa::path(put, path="/api/positions/{id}/annotation", params(("id"=Uuid, Path)), request_body=AnnotationUpdate, responses((status=200,body=Annotation),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Annotations")]
async fn put_position_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<AnnotationUpdate>, JsonRejection>,
) -> ApiResult<Json<Annotation>> {
    put_target_annotation(state, AnnotationKind::Position, id, payload).await
}

#[utoipa::path(put, path="/api/games/{id}/annotation", params(("id"=Uuid, Path)), request_body=AnnotationUpdate, responses((status=200,body=Annotation),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Annotations")]
async fn put_game_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<AnnotationUpdate>, JsonRejection>,
) -> ApiResult<Json<Annotation>> {
    put_target_annotation(state, AnnotationKind::Game, id, payload).await
}

#[utoipa::path(put, path="/api/moves/{id}/annotation", params(("id"=Uuid, Path)), request_body=AnnotationUpdate, responses((status=200,body=Annotation),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Annotations")]
async fn put_move_annotation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<AnnotationUpdate>, JsonRejection>,
) -> ApiResult<Json<Annotation>> {
    put_target_annotation(state, AnnotationKind::Move, id, payload).await
}

fn limit(value: Option<i64>) -> ApiResult<i64> {
    let limit = value.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    Ok(limit)
}

pub(crate) fn literal_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

#[utoipa::path(get, path="/api/annotations/search", params(SearchQuery), responses((status=200,body=AnnotationPage),(status=422,body=ErrorBody)), tag="Annotations")]
async fn search_annotations(
    State(state): State<AppState>,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> ApiResult<Json<AnnotationPage>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    let raw_q = query.q.as_deref().unwrap_or("");
    let q = raw_q.trim();
    if raw_q.chars().any(char::is_control)
        || (!q.is_empty() && !(3..=200).contains(&q.chars().count()))
    {
        return Err(ApiError::invalid(
            "Search needs 3–200 characters without control characters",
        ));
    }
    let tag = query
        .tag
        .as_deref()
        .map(normalize_tag)
        .transpose()?
        .filter(|s| !s.is_empty());
    if q.is_empty() && tag.is_none() {
        return Err(ApiError::invalid("Supply search text or an exact tag"));
    }
    let limit = limit(query.limit)?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "SELECT a.id,a.target_kind,a.target_id,m.id AS match_uuid,m.filename,m.white,m.black,coalesce(p.game_no,g.game_no,mv.game_no) AS game_no,coalesce(p.ply,mv.move_no) AS ply,a.tags,left(a.comment,300) AS comment_excerpt,a.updated_at FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid LEFT JOIN match_games g ON g.id=a.game_uuid LEFT JOIN match_moves mv ON mv.id=a.move_uuid JOIN matches m ON m.id=coalesce(a.match_uuid,p.match_uuid,g.match_uuid,mv.match_uuid) WHERE true",
    );
    if !q.is_empty() {
        sql.push(" AND a.search_text LIKE ")
            .push_bind(literal_pattern(q));
    }
    if let Some(database_id) = query.database_id {
        sql.push(" AND m.database_id=").push_bind(database_id);
    }
    if let Some(tag) = tag {
        sql.push(" AND a.tags @> ").push_bind(vec![tag]);
    }
    if let Some(kind) = query.kind {
        sql.push(" AND a.target_kind=").push_bind(kind.as_str());
    }
    if let Some(after) = query.after {
        sql.push(" AND a.id>").push_bind(after);
    }
    sql.push(" ORDER BY a.id LIMIT ").push_bind(limit + 1);
    let mut items = sql
        .build_query_as::<AnnotationHit>()
        .fetch_all(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|a| a.id)
    } else {
        None
    };
    Ok(Json(AnnotationPage { items, next_cursor }))
}

#[utoipa::path(get, path="/api/tags", params(TagQuery), responses((status=200,body=Vec<String>),(status=422,body=ErrorBody)), tag="Annotations")]
async fn tags(
    State(state): State<AppState>,
    query: Result<Query<TagQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<String>>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    let q = normalize_tag(query.q.as_deref().unwrap_or(""))?;
    let limit = limit(query.limit)?;
    let tags = sqlx::query_scalar("SELECT DISTINCT tag FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid LEFT JOIN match_games g ON g.id=a.game_uuid LEFT JOIN match_moves mv ON mv.id=a.move_uuid JOIN matches m ON m.id=coalesce(a.match_uuid,p.match_uuid,g.match_uuid,mv.match_uuid) CROSS JOIN LATERAL unnest(a.tags) AS tag WHERE tag LIKE $1 AND ($3::uuid IS NULL OR m.database_id=$3) ORDER BY tag LIMIT $2")
        .bind(literal_pattern(&q)).bind(limit).bind(query.database_id).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(tags))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_match_annotation,
        get_position_annotation,
        get_game_annotation,
        get_move_annotation,
        put_match_annotation,
        put_position_annotation,
        put_game_annotation,
        put_move_annotation,
        search_annotations,
        tags
    ),
    components(schemas(
        AnnotationKind,
        Annotation,
        AnnotationUpdate,
        AnnotationHit,
        AnnotationPage
    ))
)]
pub struct AnnotationDoc;
