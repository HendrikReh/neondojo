//! Virtual player profiles group real profiles without changing them. Each
//! real profile belongs to at most one virtual profile; removing a member or
//! deleting the virtual profile leaves every real profile as it was.
use super::{DISPLAY_NAME, PlayerAlias, PlayerRating, PlayerRatingPoint, clean_name, ratings};
use crate::domain_error::DomainError;
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    databases::conflict,
};
use axum::{
    Router,
    extract::State,
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct VirtualPlayerRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct VirtualPlayerProfile {
    pub id: Uuid,
    pub name: String,
    /// Chosen as My player profile in Settings.
    pub is_me: bool,
    pub version: i64,
    pub member_count: i64,
    /// Distinct matches of all members.
    pub match_count: i64,
    /// The matches that can enter the PR once analyzed: all but bot matches
    /// played with learning aids or still running in Competition mode.
    pub rateable_match_count: i64,
    pub game_count: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct VirtualPlayerMember {
    pub id: Uuid,
    pub display_name: String,
    #[sqlx(json)]
    pub aliases: Vec<PlayerAlias>,
    pub match_count: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VirtualPlayerDetails {
    #[serde(flatten)]
    pub profile: VirtualPlayerProfile,
    /// Members in name order.
    pub members: Vec<VirtualPlayerMember>,
    pub ratings: Vec<PlayerRating>,
    /// Per-match ratings, oldest first. Bot matches count only when played
    /// in Competition mode without learning aids.
    pub rating_history: Vec<PlayerRatingPoint>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VirtualPlayerList {
    pub items: Vec<VirtualPlayerProfile>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VirtualPlayerCreate {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VirtualPlayerRename {
    pub name: String,
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VirtualPlayerMemberWrite {
    pub profile_id: Uuid,
    pub version: i64,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct VirtualPlayerVersion {
    /// Expected revision of the virtual profile.
    version: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MyPlayerProfile {
    /// The virtual profile that counts as "me", if any.
    pub profile: Option<VirtualPlayerRef>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MyPlayerProfileWrite {
    /// Null makes no virtual profile "me".
    pub id: Option<Uuid>,
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

pub(crate) async fn list_virtual_service(pool: &PgPool) -> Result<VirtualPlayerList, DomainError> {
    let items = sqlx::query_as(&format!(
        "SELECT vp.id,vp.name,vp.is_me,vp.version,{MEMBER_COUNT} AS member_count,
           {MATCH_COUNT} AS match_count,{RATEABLE_MATCH_COUNT} AS rateable_match_count,
           {GAME_COUNT} AS game_count
         FROM virtual_player_profiles vp ORDER BY vp.is_me DESC,lower(vp.name),vp.id LIMIT $1"
    ))
    .bind(MAX_VIRTUAL_PROFILES)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(VirtualPlayerList { items })
}

pub(crate) async fn create_virtual_service(
    pool: &PgPool,
    name: &str,
) -> Result<VirtualPlayerDetails, DomainError> {
    let name = virtual_name(name)?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO virtual_player_profiles(name) VALUES($1)
         ON CONFLICT ((lower(name))) DO NOTHING RETURNING id",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let id = id.ok_or_else(taken)?;
    Ok(details(pool, id).await?)
}

pub(crate) async fn get_virtual_service(
    pool: &PgPool,
    id: Uuid,
) -> Result<VirtualPlayerDetails, DomainError> {
    Ok(details(pool, id).await?)
}

pub(crate) async fn rename_virtual_service(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    version: i64,
) -> Result<VirtualPlayerDetails, DomainError> {
    let name = virtual_name(name)?;
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, version).await?;
    let renamed =
        sqlx::query("UPDATE virtual_player_profiles SET name=$2,version=version+1 WHERE id=$1")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await;
    match renamed {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(taken().into());
        }
        result => result.map_err(anyhow::Error::from)?,
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(details(pool, id).await?)
}

pub(crate) async fn delete_virtual_service(
    pool: &PgPool,
    id: Uuid,
    version: i64,
) -> Result<(), DomainError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, version).await?;
    // Members cascade; the real profiles stay untouched.
    sqlx::query("DELETE FROM virtual_player_profiles WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(())
}

pub(crate) async fn add_virtual_member_service(
    pool: &PgPool,
    id: Uuid,
    profile_id: Uuid,
    version: i64,
) -> Result<VirtualPlayerDetails, DomainError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock(&mut tx, id, version).await?;
    // Lock the real profile so a concurrent merge cannot delete it meanwhile.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM player_profiles WHERE id=$1 FOR SHARE")
            .bind(profile_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if exists.is_none() {
        return Err(DomainError::invalid("Choose an existing player profile"));
    }
    let current: Option<VirtualPlayerRef> = sqlx::query_as(
        "SELECT vp.id,vp.name FROM virtual_player_members v
         JOIN virtual_player_profiles vp ON vp.id=v.virtual_id WHERE v.profile_id=$1",
    )
    .bind(profile_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    match current {
        Some(current) if current.id == id => {}
        Some(_) => {
            return Err(conflict(
                "Player profile already belongs to another virtual profile; remove it there first",
            )
            .into());
        }
        None => {
            let added = sqlx::query(
                "INSERT INTO virtual_player_members(profile_id,virtual_id) VALUES($1,$2)
                 ON CONFLICT(profile_id) DO NOTHING",
            )
            .bind(profile_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .rows_affected();
            if added == 0 {
                return Err(conflict(
                    "Player profile already belongs to another virtual profile; remove it there first",
                )
                .into());
            }
            bump(&mut tx, id).await?;
        }
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(details(pool, id).await?)
}

pub(crate) async fn remove_virtual_member_service(
    pool: &PgPool,
    id: Uuid,
    profile_id: Uuid,
    version: i64,
) -> Result<VirtualPlayerDetails, DomainError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
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
    Ok(details(pool, id).await?)
}

pub(crate) async fn get_my_profile_service(pool: &PgPool) -> Result<MyPlayerProfile, DomainError> {
    let profile = crate::player_scope::my_virtual_profile(pool)
        .await?
        .map(|(id, name)| VirtualPlayerRef { id, name });
    Ok(MyPlayerProfile { profile })
}

pub(crate) async fn set_my_profile_service(
    pool: &PgPool,
    id: Option<Uuid>,
) -> Result<MyPlayerProfile, DomainError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Serializes concurrent choices, the only writers of `is_me`; the partial
    // unique index enforces one "me". Other edits lock only their own row.
    crate::advisory_locks::xact_lock(&mut tx, crate::advisory_locks::MY_VIRTUAL_PLAYER)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(id) = id {
        // Keeps a concurrent delete from removing the choice before it is saved.
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM virtual_player_profiles WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
        if exists.is_none() {
            return Err(DomainError::invalid(
                "Choose an existing virtual player profile",
            ));
        }
    }
    // Clear first: the unique index is checked row by row.
    sqlx::query(
        "UPDATE virtual_player_profiles SET is_me=false,version=version+1
         WHERE is_me AND id IS DISTINCT FROM $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    sqlx::query(
        "UPDATE virtual_player_profiles SET is_me=true,version=version+1 WHERE id=$1 AND NOT is_me",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let profile = sqlx::query_as("SELECT id,name FROM virtual_player_profiles WHERE is_me")
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(MyPlayerProfile { profile })
}

#[utoipa::path(get,path="/api/virtual-players",responses((status=200,body=VirtualPlayerList)),tag="Players")]
async fn list(State(state): State<AppState>) -> ApiResult<Json<VirtualPlayerList>> {
    Ok(Json(list_virtual_service(&state.pool).await?))
}

#[utoipa::path(post,path="/api/virtual-players",request_body=VirtualPlayerCreate,responses((status=201,body=VirtualPlayerDetails),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn create(
    State(state): State<AppState>,
    Json(body): Json<VirtualPlayerCreate>,
) -> ApiResult<(StatusCode, Json<VirtualPlayerDetails>)> {
    Ok((
        StatusCode::CREATED,
        Json(create_virtual_service(&state.pool, &body.name).await?),
    ))
}

#[utoipa::path(get,path="/api/virtual-players/{id}",params(("id"=Uuid,Path)),responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody)),tag="Players")]
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    Ok(Json(get_virtual_service(&state.pool, id).await?))
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
    Json(body): Json<VirtualPlayerRename>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    Ok(Json(
        rename_virtual_service(&state.pool, id, &body.name, body.version).await?,
    ))
}

#[utoipa::path(delete,path="/api/virtual-players/{id}",params(("id"=Uuid,Path),VirtualPlayerVersion),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Players")]
async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(VirtualPlayerVersion { version }): Query<VirtualPlayerVersion>,
) -> ApiResult<StatusCode> {
    delete_virtual_service(&state.pool, id, version).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post,path="/api/virtual-players/{id}/members",params(("id"=Uuid,Path)),request_body=VirtualPlayerMemberWrite,responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Players")]
async fn add_member(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<VirtualPlayerMemberWrite>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    Ok(Json(
        add_virtual_member_service(&state.pool, id, body.profile_id, body.version).await?,
    ))
}

#[utoipa::path(delete,path="/api/virtual-players/{id}/members/{profile_id}",params(("id"=Uuid,Path),("profile_id"=Uuid,Path),VirtualPlayerVersion),responses((status=200,body=VirtualPlayerDetails),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Players")]
async fn remove_member(
    State(state): State<AppState>,
    Path((id, profile_id)): Path<(Uuid, Uuid)>,
    Query(VirtualPlayerVersion { version }): Query<VirtualPlayerVersion>,
) -> ApiResult<Json<VirtualPlayerDetails>> {
    Ok(Json(
        remove_virtual_member_service(&state.pool, id, profile_id, version).await?,
    ))
}

#[utoipa::path(get,path="/api/virtual-players/me",responses((status=200,body=MyPlayerProfile)),tag="Players")]
async fn me(State(state): State<AppState>) -> ApiResult<Json<MyPlayerProfile>> {
    Ok(Json(get_my_profile_service(&state.pool).await?))
}

#[utoipa::path(put,path="/api/virtual-players/me",request_body=MyPlayerProfileWrite,responses((status=200,body=MyPlayerProfile),(status=422,body=ErrorBody)),tag="Players")]
async fn set_me(
    State(state): State<AppState>,
    Json(body): Json<MyPlayerProfileWrite>,
) -> ApiResult<Json<MyPlayerProfile>> {
    Ok(Json(set_my_profile_service(&state.pool, body.id).await?))
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
