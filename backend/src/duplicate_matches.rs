//! Duplicate match finder: library matches with the same played record.
//!
//! Two library matches are duplicates when they have the same match length and
//! the same sequence of checker decisions in every game: the board (Position
//! ID) with the roll and the match context seen from the player on roll (cube
//! value, cube ownership, Crawford, own and opposing score). The comparison is
//! independent of which side a source calls White and of the order in which it
//! records the dice, so it also recognizes one match imported from different
//! source formats. A group whose members all share the original-file SHA-256
//! consists of identical files.
//!
//! The record itself is defined once, by the database function
//! `played_record_fingerprint`, and stored in `matches.played_record_fingerprint`.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::databases::{DatabaseMatch, database_matches, query};
use crate::play::PAUSED_MATCHES_DATABASE_ID;
use axum::{
    Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const DEFAULT_GROUPS: i64 = 50;
const MAX_GROUPS: i64 = 100;
/// Listed members per group; `match_count` reports the full size.
pub const MAX_GROUP_ITEMS: i32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateMatchKind {
    /// Every member was imported from a byte-identical original file.
    IdenticalFiles,
    /// The members have the same moves but different original files, for
    /// example the same match in MAT and XG format.
    SameMoves,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DuplicateMatchGroup {
    pub kind: DuplicateMatchKind,
    /// Number of matches in the group, including any beyond `items`.
    pub match_count: i64,
    /// Up to 50 members, oldest library entry (lowest ID) first.
    pub items: Vec<DatabaseMatch>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DuplicateMatchPage {
    pub groups: Vec<DuplicateMatchGroup>,
    pub next_cursor: Option<String>,
    /// Duplicate groups in the scope, across all pages.
    pub total_groups: i64,
    /// Matches that belong to a duplicate group, across all pages.
    pub total_matches: i64,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DuplicateMatchQuery {
    /// Compare only matches in this database; omitted compares all library
    /// databases. Paused Matches and saved-position context are never included.
    pub database_id: Option<Uuid>,
    /// `next_cursor` of the previous page.
    pub after: Option<String>,
    /// Groups per page, 1–100 (default 50).
    pub limit: Option<i64>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/library/duplicates", get(duplicate_matches))
}

#[utoipa::path(get, path="/api/library/duplicates", params(DuplicateMatchQuery), responses((status=200, body=DuplicateMatchPage), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Databases")]
async fn duplicate_matches(
    State(state): State<AppState>,
    q: Result<Query<DuplicateMatchQuery>, QueryRejection>,
) -> ApiResult<Json<DuplicateMatchPage>> {
    Ok(Json(find(&state.pool, query(q)?).await?))
}

/// One page of duplicate groups with the totals over all pages. Binds the
/// Paused Matches database, the optional scope, the listed members per group,
/// the cursor and the page size plus one. `matches_played_record_idx`
/// (migration 0094) delivers the library matches in group order.
const GROUPS: &str = "WITH grouped AS (
        SELECT min(m.match_no) AS first_match_no,
               (array_agg(m.id ORDER BY m.match_no))[1:$3::int] AS ids,
               count(*) AS match_count,
               count(DISTINCT m.sha256)=1 AS identical_files
        FROM matches m JOIN databases d ON d.id=m.database_id
        WHERE NOT m.collection_only AND NOT d.collection_only AND m.database_id<>$1
          AND ($2::uuid IS NULL OR m.database_id=$2)
          AND m.played_record_fingerprint IS NOT NULL
        GROUP BY m.match_length,m.played_record_fingerprint
        HAVING count(*)>1
    ), counted AS (
        SELECT *,count(*) OVER () AS total_groups,
               (sum(match_count) OVER ())::bigint AS total_matches
        FROM grouped
    )
    SELECT first_match_no,ids,match_count,identical_files,total_groups,total_matches
    FROM counted
    WHERE $4::bigint IS NULL OR first_match_no>$4
    ORDER BY first_match_no
    LIMIT $5";

#[derive(sqlx::FromRow)]
struct GroupRow {
    first_match_no: i64,
    ids: Vec<Uuid>,
    match_count: i64,
    identical_files: bool,
    total_groups: i64,
    total_matches: i64,
}

pub async fn find(pool: &PgPool, q: DuplicateMatchQuery) -> ApiResult<DuplicateMatchPage> {
    let limit = q.limit.unwrap_or(DEFAULT_GROUPS);
    if !(1..=MAX_GROUPS).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    let after = q
        .after
        .as_deref()
        .map(|cursor| {
            cursor
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| ApiError::invalid("Invalid cursor"))
        })
        .transpose()?;
    if let Some(id) = q.database_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::missing());
        }
    }
    // The played record is stored per match by the database function
    // `played_record_fingerprint` (migration 0086), refreshed on every write to
    // positions; grouping reads only the matches table.
    let rows: Vec<GroupRow> = sqlx::query_as(GROUPS)
        .bind(PAUSED_MATCHES_DATABASE_ID)
        .bind(q.database_id)
        .bind(MAX_GROUP_ITEMS)
        .bind(after)
        .bind(limit + 1)
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    let (total_groups, total_matches) = rows
        .first()
        .map_or((0, 0), |row| (row.total_groups, row.total_matches));
    let more = rows.len() as i64 > limit;
    let rows = &rows[..rows.len().min(limit as usize)];
    let ids: Vec<Uuid> = rows
        .iter()
        .flat_map(|row| row.ids.iter().copied())
        .collect();
    let mut members = database_matches(pool, &ids).await?.into_iter().peekable();
    let groups = rows
        .iter()
        .map(|row| DuplicateMatchGroup {
            kind: if row.identical_files {
                DuplicateMatchKind::IdenticalFiles
            } else {
                DuplicateMatchKind::SameMoves
            },
            match_count: row.match_count,
            items: std::iter::from_fn(|| members.next_if(|m| row.ids.contains(&m.id))).collect(),
        })
        .collect();
    Ok(DuplicateMatchPage {
        groups,
        next_cursor: more
            .then(|| rows.last().map(|row| row.first_match_no.to_string()))
            .flatten(),
        total_groups,
        total_matches,
    })
}

/// Support for the ignored large-library performance test: the plan of the
/// first page's group query with the parameters [`find`] binds.
#[doc(hidden)]
pub async fn explain_groups(
    pool: &PgPool,
    database_id: Option<Uuid>,
) -> anyhow::Result<serde_json::Value> {
    Ok(
        sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {GROUPS}"))
            .bind(PAUSED_MATCHES_DATABASE_ID)
            .bind(database_id)
            .bind(MAX_GROUP_ITEMS)
            .bind(None::<i64>)
            .bind(DEFAULT_GROUPS + 1)
            .fetch_one(pool)
            .await?,
    )
}

#[derive(OpenApi)]
#[openapi(
    paths(duplicate_matches),
    components(schemas(DuplicateMatchKind, DuplicateMatchGroup, DuplicateMatchPage))
)]
pub struct DuplicateMatchesDoc;
