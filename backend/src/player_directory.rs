use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    databases::{conflict, json, query},
    match_analysis::{match_performance_rating, performance_rating_value},
    search::literal_pattern,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::get,
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct PlayerProfile {
    id: Uuid,
    display_name: String,
    first_name: String,
    last_name: String,
    version: i64,
    match_count: i64,
    game_count: i64,
}

#[derive(Serialize, Deserialize, ToSchema, sqlx::FromRow)]
struct PlayerAlias {
    platform: String,
    name: String,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct PlayerSearchHit {
    #[serde(flatten)]
    #[sqlx(flatten)]
    profile: PlayerProfile,
    /// Alias responsible for the hit when the visible name does not match.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(json(nullable))]
    matching_alias: Option<PlayerAlias>,
}

#[derive(Serialize, ToSchema)]
struct PlayerRating {
    profile: String,
    value: f64,
    match_count: i64,
}

/// One fully analyzed match of the player, for a rating trend.
#[derive(Serialize, ToSchema)]
struct PlayerRatingPoint {
    match_id: Uuid,
    /// Date played, or the import date when the match records none.
    date: chrono::NaiveDate,
    profile: String,
    value: f64,
    /// Unforced checker plays and close cube decisions behind the value.
    decisions: i64,
    /// The same rating for checker play alone: saved checker losses over
    /// unforced checker plays. Absent without such plays.
    checker_value: Option<f64>,
    /// The remaining loss over close cube decisions. Absent without them.
    cube_value: Option<f64>,
}

#[derive(Serialize, ToSchema)]
struct PlayerDetails {
    #[serde(flatten)]
    profile: PlayerProfile,
    aliases: Vec<PlayerAlias>,
    ratings: Vec<PlayerRating>,
    /// Per-match ratings, oldest first.
    rating_history: Vec<PlayerRatingPoint>,
}

#[derive(Deserialize, ToSchema)]
struct PlayerNameWrite {
    first_name: String,
    last_name: String,
    version: i64,
}

#[derive(Deserialize, ToSchema)]
struct PlayerMerge {
    source_id: Uuid,
    source_version: i64,
    version: i64,
}

#[derive(Deserialize, ToSchema)]
struct PlayerAliasWrite {
    platform: String,
    name: String,
    version: i64,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum PlayerSort {
    #[default]
    Name,
    Matches,
    Games,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum PlayerSortDirection {
    Asc,
    Desc,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PlayerQuery {
    q: Option<String>,
    /// Continues the default order (name ascending) after this profile.
    after: Option<Uuid>,
    /// Omitted: name. Players with equal counts follow in name order.
    #[serde(default)]
    sort: PlayerSort,
    /// Omitted: name ascending; match and game counts descending.
    direction: Option<PlayerSortDirection>,
    /// Exclude the destination profile when choosing a profile to merge.
    exclude_id: Option<Uuid>,
    /// One-based result page. Mutually exclusive with `after`.
    page: Option<i64>,
}

#[derive(Serialize, ToSchema)]
struct PlayerPage {
    items: Vec<PlayerSearchHit>,
    next_cursor: Option<Uuid>,
    /// Total matching players when `page` is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<i64>,
}

pub(crate) const DISPLAY_NAME: &str = "coalesce(nullif(concat_ws(' ',nullif(p.first_name,''),nullif(p.last_name,'')),''),p.source_name)";
const MATCHING_ALIAS: &str =
    "FROM player_aliases a WHERE a.profile_id=p.id AND lower(a.name) LIKE $2";
const MATCH_COUNT: &str = "(SELECT count(DISTINCT mp.match_id) FROM player_aliases a
     JOIN match_players mp ON mp.alias_id=a.id WHERE a.profile_id=p.id)";
const GAME_COUNT: &str = "(SELECT count(*) FROM match_games g WHERE EXISTS (
        SELECT 1 FROM player_aliases a JOIN match_players mp ON mp.alias_id=a.id
        WHERE a.profile_id=p.id AND mp.match_id=g.match_uuid
    ))";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/players", get(list))
        .route("/api/players/{id}", get(detail).put(update))
        .route("/api/players/{id}/merge", axum::routing::post(merge))
        .route("/api/players/{id}/aliases", axum::routing::post(add_alias))
}

#[utoipa::path(get,path="/api/players",params(PlayerQuery),responses((status=200,body=PlayerPage),(status=422,body=ErrorBody)),tag="Players")]
async fn list(
    State(state): State<AppState>,
    params: Result<Query<PlayerQuery>, QueryRejection>,
) -> ApiResult<Json<PlayerPage>> {
    let params = query(params)?;
    let search = params.q.as_deref().unwrap_or("").trim();
    if search.chars().count() > 240 {
        return Err(ApiError::invalid("Player search is too long"));
    }
    let offset = crate::search::page_offset(params.page, params.after.is_some(), 50)?;
    let descending = match params.direction {
        Some(direction) => direction == PlayerSortDirection::Desc,
        None => params.sort != PlayerSort::Name,
    };
    if params.after.is_some() && (params.sort != PlayerSort::Name || descending) {
        return Err(ApiError::invalid(
            "A cursor continues the name order only; use page with another sort",
        ));
    }
    let direction = if descending { "DESC" } else { "ASC" };
    // The page is chosen from the profiles, the final order from its rows.
    let (page_order, row_order) = match params.sort {
        PlayerSort::Name => (
            format!("lower({DISPLAY_NAME}) {direction}, p.id"),
            format!("lower(p.display_name) {direction}, p.id"),
        ),
        PlayerSort::Matches => (
            format!("{MATCH_COUNT} {direction}, lower({DISPLAY_NAME}), p.id"),
            format!("match_count {direction}, lower(p.display_name), p.id"),
        ),
        PlayerSort::Games => (
            format!("{GAME_COUNT} {direction}, lower({DISPLAY_NAME}), p.id"),
            format!("game_count {direction}, lower(p.display_name), p.id"),
        ),
    };
    let filters = format!(
        "(EXISTS (SELECT 1 FROM player_aliases a JOIN match_players mp ON mp.alias_id=a.id
                 WHERE a.profile_id=p.id) OR p.first_name<>'' OR p.last_name<>'')
         AND ($3::uuid IS NULL OR p.id<>$3)
         AND ($1='' OR lower({DISPLAY_NAME}) LIKE $2 OR EXISTS (SELECT 1 {MATCHING_ALIAS}))"
    );
    let pattern = literal_pattern(search);
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let total = if offset.is_some() {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        Some(
            sqlx::query_scalar(&format!(
                "SELECT count(*) FROM player_profiles p WHERE {filters}"
            ))
            .bind(search)
            .bind(&pattern)
            .bind(params.exclude_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?,
        )
    } else {
        None
    };
    let mut items = sqlx::query_as::<_, PlayerSearchHit>(&format!(
        "WITH page AS MATERIALIZED (
           SELECT p.id,p.first_name,p.last_name,p.version,{DISPLAY_NAME} AS display_name
           FROM player_profiles p
           WHERE {filters}
             AND ($4::uuid IS NULL OR (lower({DISPLAY_NAME}), p.id) >
                  (SELECT lower({DISPLAY_NAME}), p.id FROM player_profiles p WHERE p.id=$4))
           ORDER BY {page_order} LIMIT 51 OFFSET $5
         )
         SELECT p.id,p.display_name,p.first_name,p.last_name,p.version,
           {MATCH_COUNT} AS match_count,{GAME_COUNT} AS game_count,
           CASE WHEN $1<>'' AND lower(p.display_name) NOT LIKE $2 THEN (
             SELECT jsonb_build_object('name',a.name,'platform',a.platform)
             {MATCHING_ALIAS} ORDER BY a.name,a.platform LIMIT 1
           ) END AS matching_alias
         FROM page p ORDER BY {row_order}"
    ))
    .bind(search)
    .bind(pattern)
    .bind(params.exclude_id)
    .bind(params.after)
    .bind(offset.unwrap_or(0))
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let next_cursor = if items.len() > 50 {
        items.pop();
        items.last().map(|item| item.profile.id)
    } else {
        None
    };
    Ok(Json(PlayerPage {
        items,
        next_cursor,
        total,
    }))
}

#[utoipa::path(get,path="/api/players/{id}",params(("id"=Uuid,Path)),responses((status=200,body=PlayerDetails),(status=404,body=ErrorBody)),tag="Players")]
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PlayerDetails>> {
    Ok(Json(details(&state.pool, id).await?))
}

async fn details(pool: &PgPool, id: Uuid) -> ApiResult<PlayerDetails> {
    let profile = sqlx::query_as(&format!(
        "SELECT p.id,{DISPLAY_NAME} AS display_name,p.first_name,p.last_name,p.version,
           {MATCH_COUNT} AS match_count,{GAME_COUNT} AS game_count
         FROM player_profiles p WHERE p.id=$1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    let aliases = sqlx::query_as(
        "SELECT platform,name FROM player_aliases WHERE profile_id=$1 ORDER BY platform,name",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let (ratings, rating_history) = ratings(pool, id).await.map_err(anyhow::Error::from)?;
    Ok(PlayerDetails {
        profile,
        aliases,
        ratings,
        rating_history,
    })
}

async fn ratings(
    pool: &PgPool,
    id: Uuid,
) -> Result<(Vec<PlayerRating>, Vec<PlayerRatingPoint>), sqlx::Error> {
    let mut rows = sqlx::query_as::<_, (Uuid, i16, serde_json::Value, chrono::NaiveDate, Option<f64>)>(
        "WITH participants AS (
           SELECT mp.match_id,mp.side FROM player_aliases a JOIN match_players mp ON mp.alias_id=a.id
           WHERE a.profile_id=$1
         ), candidates AS (
           SELECT j.match_uuid,j.result,coalesce(m.played_on,m.created_at::date) AS played,j.id AS job_id
           FROM (SELECT DISTINCT match_id FROM participants) p
           JOIN matches m ON m.id=p.match_id
           CROSS JOIN LATERAL (
             SELECT DISTINCT ON (result #>> '{performance_rating,profile}') j.match_uuid,j.result,j.id
             FROM jobs j WHERE j.match_uuid=m.id AND j.source_revision=m.source_revision
               AND j.kind='match' AND j.game_no IS NULL AND j.status='completed'
               AND j.total_games>0 AND j.completed_games=j.total_games
               AND j.result #>> '{performance_rating,profile}' IS NOT NULL
             ORDER BY result #>> '{performance_rating,profile}', j.created_at DESC,j.id DESC
           ) j
         )
         SELECT c.match_uuid,p.side,c.result,c.played,
                -- Null when the job saved no checker decisions for this player;
                -- older jobs keep only totals and cannot be split.
                (SELECT sum(a.equity_loss)::float8 FROM move_analyses a
                 WHERE a.job_id=c.job_id AND a.kind='checker' AND a.player=p.side) AS checker_loss
         FROM candidates c
         JOIN participants p ON p.match_id=c.match_uuid ORDER BY c.match_uuid",
    )
    .bind(id)
    .fetch(pool);
    let mut totals = BTreeMap::<String, (f64, i64, i64, Option<Uuid>)>::new();
    let mut history = Vec::new();
    while let Some((match_id, side, result, date, checker_loss)) = rows.try_next().await? {
        if let Some(rating) = match_performance_rating(&result) {
            let player = &rating.players[side as usize];
            if let Some(value) = player.value {
                history.push(PlayerRatingPoint {
                    match_id,
                    date,
                    profile: rating.profile.clone(),
                    value,
                    decisions: i64::from(player.unforced_checker_moves)
                        + i64::from(player.close_cube_decisions),
                    checker_value: checker_loss.and_then(|loss| {
                        performance_rating_value(
                            loss.min(player.normalized_equity_loss),
                            i64::from(player.unforced_checker_moves),
                        )
                    }),
                    cube_value: checker_loss.and_then(|loss| {
                        performance_rating_value(
                            (player.normalized_equity_loss - loss).max(0.0),
                            i64::from(player.close_cube_decisions),
                        )
                    }),
                });
                let (loss, decisions, count, last_match) =
                    totals.entry(rating.profile).or_default();
                *loss += player.normalized_equity_loss;
                *decisions += i64::from(player.unforced_checker_moves)
                    + i64::from(player.close_cube_decisions);
                if *last_match != Some(match_id) {
                    *count += 1;
                    *last_match = Some(match_id);
                }
            }
        }
    }
    history.sort_by(|a, b| a.date.cmp(&b.date).then(a.match_id.cmp(&b.match_id)));
    let ratings = totals
        .into_iter()
        .filter_map(|(profile, (loss, decisions, match_count, _))| {
            performance_rating_value(loss, decisions).map(|value| PlayerRating {
                profile,
                value,
                match_count,
            })
        })
        .collect();
    Ok((ratings, history))
}

#[utoipa::path(put,path="/api/players/{id}",params(("id"=Uuid,Path)),request_body=PlayerNameWrite,responses((status=200,body=PlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<PlayerNameWrite>, JsonRejection>,
) -> ApiResult<Json<PlayerDetails>> {
    let body = json(body)?;
    let first_name = clean_name(&body.first_name, 120)?;
    let last_name = clean_name(&body.last_name, 120)?;
    let changed = sqlx::query(
        "UPDATE player_profiles SET first_name=$2,last_name=$3,version=version+1 WHERE id=$1 AND version=$4",
    )
    .bind(id).bind(first_name).bind(last_name).bind(body.version)
    .execute(&state.pool).await.map_err(anyhow::Error::from)?.rows_affected();
    if changed == 0 {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM player_profiles WHERE id=$1)")
                .bind(id)
                .fetch_one(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
        return Err(if exists { stale() } else { ApiError::missing() });
    }
    Ok(Json(details(&state.pool, id).await?))
}

fn stale() -> ApiError {
    conflict("Player profile changed; reload before saving")
}

fn clean_name(value: &str, max: usize) -> ApiResult<&str> {
    let value = value.trim();
    if value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(ApiError::invalid("Invalid player name"));
    }
    Ok(value)
}

#[utoipa::path(post,path="/api/players/{id}/merge",params(("id"=Uuid,Path)),request_body=PlayerMerge,responses((status=200,body=PlayerDetails),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn merge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<PlayerMerge>, JsonRejection>,
) -> ApiResult<Json<PlayerDetails>> {
    let body = json(body)?;
    if id == body.source_id {
        return Err(ApiError::invalid("Choose a different player"));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let profiles: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id,version FROM player_profiles WHERE id=ANY($1) ORDER BY id FOR UPDATE",
    )
    .bind(&[id, body.source_id][..])
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if !profiles.contains(&(id, body.version))
        || !profiles.contains(&(body.source_id, body.source_version))
    {
        return Err(stale());
    }
    sqlx::query("UPDATE player_aliases SET profile_id=$1 WHERE profile_id=$2")
        .bind(id)
        .bind(body.source_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    sqlx::query("DELETE FROM player_profiles WHERE id=$1")
        .bind(body.source_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    sqlx::query("UPDATE player_profiles SET version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(details(&state.pool, id).await?))
}

#[utoipa::path(post,path="/api/players/{id}/aliases",params(("id"=Uuid,Path)),request_body=PlayerAliasWrite,responses((status=200,body=PlayerDetails),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn add_alias(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<PlayerAliasWrite>, JsonRejection>,
) -> ApiResult<Json<PlayerDetails>> {
    let body = json(body)?;
    let name = clean_name(&body.name, 1024)?;
    if name.is_empty()
        || !["", "backgammongalaxy.com", "opengammon.com"].contains(&body.platform.as_str())
    {
        return Err(ApiError::invalid("Invalid player alias"));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let changed =
        sqlx::query("UPDATE player_profiles SET version=version+1 WHERE id=$1 AND version=$2")
            .bind(id)
            .bind(body.version)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .rows_affected();
    if changed == 0 {
        return Err(stale());
    }
    let added = sqlx::query("INSERT INTO player_aliases(platform,name,profile_id) VALUES($1,$2,$3) ON CONFLICT(platform,name) DO NOTHING")
        .bind(body.platform).bind(name).bind(id).execute(&mut *tx).await.map_err(anyhow::Error::from)?.rows_affected();
    if added == 0 {
        return Err(conflict("Alias already belongs to a profile"));
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(details(&state.pool, id).await?))
}

#[derive(OpenApi)]
#[openapi(
    paths(list, detail, update, merge, add_alias),
    components(schemas(
        PlayerProfile,
        PlayerSearchHit,
        PlayerDetails,
        PlayerAlias,
        PlayerRating,
        PlayerRatingPoint,
        PlayerNameWrite,
        PlayerAliasWrite,
        PlayerMerge,
        PlayerPage,
        PlayerSort,
        PlayerSortDirection
    ))
)]
pub(crate) struct PlayerDoc;
