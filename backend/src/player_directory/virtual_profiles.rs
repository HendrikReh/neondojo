//! Virtual player profiles group real profiles without changing them. Each
//! real profile belongs to at most one virtual profile; removing a member or
//! deleting the virtual profile leaves every real profile as it was.
use super::{DISPLAY_NAME, PlayerAlias, PlayerRating, PlayerRatingPoint, clean_name, ratings};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    databases::{conflict, json, query},
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

/// Bounds the overview; one person rarely needs more than a few.
const MAX_VIRTUAL_PROFILES: i64 = 500;

/// The virtual profile a real profile belongs to.
#[derive(Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub(super) struct VirtualPlayerRef {
    pub(super) id: Uuid,
    pub(super) name: String,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct VirtualPlayerProfile {
    id: Uuid,
    name: String,
    /// Chosen as My player profile in Settings.
    is_me: bool,
    version: i64,
    member_count: i64,
    /// Distinct matches of all members.
    match_count: i64,
    /// The matches that can enter the PR once analyzed: all but bot matches
    /// played with learning aids or still running in Competition mode.
    rateable_match_count: i64,
    game_count: i64,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct VirtualPlayerMember {
    id: Uuid,
    display_name: String,
    #[sqlx(json)]
    aliases: Vec<PlayerAlias>,
    match_count: i64,
}

#[derive(Serialize, ToSchema)]
struct VirtualPlayerDetails {
    #[serde(flatten)]
    profile: VirtualPlayerProfile,
    /// Members in name order.
    members: Vec<VirtualPlayerMember>,
    ratings: Vec<PlayerRating>,
    /// Per-match ratings, oldest first. Bot matches count only when played
    /// in Competition mode without learning aids.
    rating_history: Vec<PlayerRatingPoint>,
}

#[derive(Serialize, ToSchema)]
struct VirtualPlayerList {
    items: Vec<VirtualPlayerProfile>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct VirtualPlayerCreate {
    name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct VirtualPlayerRename {
    name: String,
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct VirtualPlayerMemberWrite {
    profile_id: Uuid,
    version: i64,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct VirtualPlayerVersion {
    /// Expected revision of the virtual profile.
    version: i64,
}

#[derive(Serialize, ToSchema)]
struct MyPlayerProfile {
    /// The virtual profile that counts as "me", if any.
    profile: Option<VirtualPlayerRef>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MyPlayerProfileWrite {
    /// Null makes no virtual profile "me".
    id: Option<Uuid>,
}

const MEMBER_COUNT: &str =
    "(SELECT count(*) FROM virtual_player_members v WHERE v.virtual_id=vp.id)";
const MATCH_COUNT: &str =
    "(SELECT count(DISTINCT s.match_id) FROM virtual_player_sides s WHERE s.virtual_id=vp.id)";
const RATEABLE_MATCH_COUNT: &str = "(SELECT count(DISTINCT s.match_id) FROM virtual_player_sides s
     WHERE s.virtual_id=vp.id AND s.rated)";
const GAME_COUNT: &str = "(SELECT count(*) FROM match_games g WHERE g.match_uuid IN (
        SELECT s.match_id FROM virtual_player_sides s WHERE s.virtual_id=vp.id))";

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/virtual-players", get(list).post(create))
        .route("/api/virtual-players/me", get(me).put(set_me))
        .route(
            "/api/virtual-players/{id}",
            get(detail).put(rename).delete(remove),
        )
        .route("/api/virtual-players/{id}/members", post(add_member))
        .route(
            "/api/virtual-players/{id}/members/{profile_id}",
            delete(remove_member),
        )
}

#[utoipa::path(get,path="/api/virtual-players",responses((status=200,body=VirtualPlayerList)),tag="Players")]
async fn list(State(state): State<AppState>) -> ApiResult<Json<VirtualPlayerList>> {
    let items = sqlx::query_as(&format!(
        "SELECT vp.id,vp.name,vp.is_me,vp.version,{MEMBER_COUNT} AS member_count,
           {MATCH_COUNT} AS match_count,{RATEABLE_MATCH_COUNT} AS rateable_match_count,
           {GAME_COUNT} AS game_count
         FROM virtual_player_profiles vp ORDER BY vp.is_me DESC,lower(vp.name),vp.id LIMIT $1"
    ))
    .bind(MAX_VIRTUAL_PROFILES)
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(VirtualPlayerList { items }))
}

#[utoipa::path(post,path="/api/virtual-players",request_body=VirtualPlayerCreate,responses((status=201,body=VirtualPlayerDetails),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn create(
    State(state): State<AppState>,
    body: Result<Json<VirtualPlayerCreate>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<VirtualPlayerDetails>)> {
    let body = json(body)?;
    let name = virtual_name(&body.name)?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO virtual_player_profiles(name) VALUES($1)
         ON CONFLICT ((lower(name))) DO NOTHING RETURNING id",
    )
    .bind(name)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let id = id.ok_or_else(taken)?;
    Ok((StatusCode::CREATED, Json(details(&state.pool, id).await?)))
}

#[utoipa::path(get,path="/api/virtual-players/{id}",params(("id"=Uuid,Path)),responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody)),tag="Players")]
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    Ok(Json(details(&state.pool, id).await?))
}

async fn details(pool: &PgPool, id: Uuid) -> ApiResult<VirtualPlayerDetails> {
    let profile = sqlx::query_as(&format!(
        "SELECT vp.id,vp.name,vp.is_me,vp.version,{MEMBER_COUNT} AS member_count,
           {MATCH_COUNT} AS match_count,{RATEABLE_MATCH_COUNT} AS rateable_match_count,
           {GAME_COUNT} AS game_count
         FROM virtual_player_profiles vp WHERE vp.id=$1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    let members = sqlx::query_as(&format!(
        "SELECT p.id,{DISPLAY_NAME} AS display_name,
           coalesce((SELECT jsonb_agg(jsonb_build_object('platform',a.platform,'name',a.name)
                     ORDER BY a.platform,a.name)
                     FROM player_aliases a WHERE a.profile_id=p.id),'[]'::jsonb) AS aliases,
           (SELECT count(DISTINCT mp.match_id) FROM player_aliases a
            JOIN match_players mp ON mp.alias_id=a.id WHERE a.profile_id=p.id) AS match_count
         FROM virtual_player_members v JOIN player_profiles p ON p.id=v.profile_id
         WHERE v.virtual_id=$1 ORDER BY lower({DISPLAY_NAME}),p.id"
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let (ratings, rating_history) = ratings(pool, id, true).await.map_err(anyhow::Error::from)?;
    Ok(VirtualPlayerDetails {
        profile,
        members,
        ratings,
        rating_history,
    })
}

#[utoipa::path(put,path="/api/virtual-players/{id}",params(("id"=Uuid,Path)),request_body=VirtualPlayerRename,responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn rename(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<VirtualPlayerRename>, JsonRejection>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    let body = json(body)?;
    let name = virtual_name(&body.name)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, body.version).await?;
    let renamed =
        sqlx::query("UPDATE virtual_player_profiles SET name=$2,version=version+1 WHERE id=$1")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await;
    match renamed {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => return Err(taken()),
        result => result.map_err(anyhow::Error::from)?,
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(details(&state.pool, id).await?))
}

#[utoipa::path(delete,path="/api/virtual-players/{id}",params(("id"=Uuid,Path),VirtualPlayerVersion),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Players")]
async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    version: Result<Query<VirtualPlayerVersion>, QueryRejection>,
) -> ApiResult<StatusCode> {
    let version = query(version)?.version;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, version).await?;
    // Members cascade; the real profiles stay untouched.
    sqlx::query("DELETE FROM virtual_player_profiles WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post,path="/api/virtual-players/{id}/members",params(("id"=Uuid,Path)),request_body=VirtualPlayerMemberWrite,responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn add_member(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<VirtualPlayerMemberWrite>, JsonRejection>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    let body = json(body)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, body.version).await?;
    // Lock the real profile so a concurrent merge cannot delete it meanwhile.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM player_profiles WHERE id=$1 FOR SHARE")
            .bind(body.profile_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if exists.is_none() {
        return Err(ApiError::invalid("Choose an existing player profile"));
    }
    let current: Option<VirtualPlayerRef> = sqlx::query_as(
        "SELECT vp.id,vp.name FROM virtual_player_members v
         JOIN virtual_player_profiles vp ON vp.id=v.virtual_id WHERE v.profile_id=$1",
    )
    .bind(body.profile_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    match current {
        Some(current) if current.id == id => {}
        Some(_) => {
            return Err(conflict(
                "Player profile already belongs to another virtual profile; remove it there first",
            ));
        }
        None => {
            let added = sqlx::query(
                "INSERT INTO virtual_player_members(profile_id,virtual_id) VALUES($1,$2)
                 ON CONFLICT(profile_id) DO NOTHING",
            )
            .bind(body.profile_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .rows_affected();
            if added == 0 {
                return Err(conflict(
                    "Player profile already belongs to another virtual profile; remove it there first",
                ));
            }
            bump(&mut tx, id).await?;
        }
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(details(&state.pool, id).await?))
}

#[utoipa::path(delete,path="/api/virtual-players/{id}/members/{profile_id}",params(("id"=Uuid,Path),("profile_id"=Uuid,Path),VirtualPlayerVersion),responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Players")]
async fn remove_member(
    State(state): State<AppState>,
    Path((id, profile_id)): Path<(Uuid, Uuid)>,
    version: Result<Query<VirtualPlayerVersion>, QueryRejection>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    let version = query(version)?.version;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, version).await?;
    let removed =
        sqlx::query("DELETE FROM virtual_player_members WHERE profile_id=$1 AND virtual_id=$2")
            .bind(profile_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .rows_affected();
    if removed > 0 {
        bump(&mut tx, id).await?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(details(&state.pool, id).await?))
}

#[utoipa::path(get,path="/api/virtual-players/me",responses((status=200,body=MyPlayerProfile)),tag="Players")]
async fn me(State(state): State<AppState>) -> ApiResult<Json<MyPlayerProfile>> {
    let profile = crate::player_scope::my_virtual_profile(&state.pool)
        .await?
        .map(|(id, name)| VirtualPlayerRef { id, name });
    Ok(Json(MyPlayerProfile { profile }))
}

#[utoipa::path(put,path="/api/virtual-players/me",request_body=MyPlayerProfileWrite,responses((status=200,body=MyPlayerProfile),(status=422,body=ErrorBody)),tag="Players")]
async fn set_me(
    State(state): State<AppState>,
    body: Result<Json<MyPlayerProfileWrite>, JsonRejection>,
) -> ApiResult<Json<MyPlayerProfile>> {
    let body = json(body)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Serializes concurrent choices; the partial unique index enforces one "me".
    sqlx::query("LOCK TABLE virtual_player_profiles IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(id) = body.id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM virtual_player_profiles WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::invalid(
                "Choose an existing virtual player profile",
            ));
        }
    }
    // Clear first: the unique index is checked row by row.
    sqlx::query(
        "UPDATE virtual_player_profiles SET is_me=false,version=version+1
         WHERE is_me AND id IS DISTINCT FROM $1",
    )
    .bind(body.id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    sqlx::query(
        "UPDATE virtual_player_profiles SET is_me=true,version=version+1 WHERE id=$1 AND NOT is_me",
    )
    .bind(body.id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let profile = sqlx::query_as("SELECT id,name FROM virtual_player_profiles WHERE is_me")
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(MyPlayerProfile { profile }))
}

/// Locks the virtual profile at the expected revision.
async fn lock(tx: &mut Transaction<'_, Postgres>, id: Uuid, version: i64) -> ApiResult<()> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT version FROM virtual_player_profiles WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    match current {
        None => Err(ApiError::missing()),
        Some(current) if current != version => Err(conflict(
            "Virtual player profile changed; reload before saving",
        )),
        Some(_) => Ok(()),
    }
}

async fn bump(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<()> {
    sqlx::query("UPDATE virtual_player_profiles SET version=version+1 WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

fn virtual_name(value: &str) -> ApiResult<&str> {
    let name = clean_name(value, 120)?;
    if name.is_empty() {
        return Err(ApiError::invalid(
            "Enter a name for the virtual player profile",
        ));
    }
    Ok(name)
}

fn taken() -> ApiError {
    conflict("A virtual player profile with this name already exists")
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list,
        create,
        detail,
        rename,
        remove,
        add_member,
        remove_member,
        me,
        set_me
    ),
    components(schemas(
        VirtualPlayerRef,
        VirtualPlayerProfile,
        VirtualPlayerMember,
        VirtualPlayerDetails,
        VirtualPlayerList,
        VirtualPlayerCreate,
        VirtualPlayerRename,
        VirtualPlayerMemberWrite,
        MyPlayerProfile,
        MyPlayerProfileWrite
    ))
)]
pub(super) struct VirtualPlayerDoc;
