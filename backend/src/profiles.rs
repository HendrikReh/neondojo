//! Explicit named snapshots of the shared current configuration.
use crate::configuration::validation::{changed_resources, normalize, resources};
use crate::json_errors::{Json, Path};
use crate::{
    api::{ApiError, AppState, ErrorBody},
    configuration::{store, *},
};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateConfigurationProfile {
    pub name: String,
    pub expected_state_revision: i64,
    pub configuration: ProfileConfiguration,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameConfigurationProfile {
    pub name: String,
    pub expected_state_revision: i64,
    pub expected_profile_revision: i64,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveConfigurationProfile {
    pub expected_state_revision: i64,
    pub expected_profile_revision: i64,
    pub configuration: ProfileConfiguration,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteConfigurationProfile {
    pub expected_state_revision: i64,
    pub expected_profile_revision: i64,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateConfigurationProfile {
    #[serde(alias = "expected_target_revision")]
    pub expected_profile_revision: i64,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigurationValidation {
    pub valid: bool,
    pub issues: Vec<ConfigurationIssue>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSourceAction {
    Keep,
    SaveCurrent,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActivateConfigurationProfile {
    pub expected_state_revision: i64,
    pub expected_target_revision: i64,
    pub expected_source_revision: i64,
    pub source_action: ProfileSourceAction,
    pub current_configuration: ProfileConfiguration,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/profiles", get(list_profiles).post(create_profile))
        .route(
            "/api/profiles/{id}",
            get(get_profile)
                .patch(rename_profile)
                .delete(delete_profile),
        )
        .route(
            "/api/profiles/{id}/snapshot",
            axum::routing::put(save_profile),
        )
        .route("/api/profiles/{id}/validate", post(validate_profile))
        .route("/api/profiles/{id}/activate", post(activate_profile))
}

#[utoipa::path(get,path="/api/profiles",responses((status=200,body=Vec<ConfigurationProfile>)),tag="Profiles")]
async fn list_profiles(State(app): State<AppState>) -> Result<Json<Vec<ConfigurationProfile>>> {
    let mut tx = app.pool.begin().await?;
    store::lock(&mut tx).await?;
    let result = store::profiles(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(get,path="/api/profiles/{id}",params(("id"=Uuid,Path)),responses((status=200,body=ConfigurationProfile),(status=404,body=ErrorBody)),tag="Profiles")]
async fn get_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ConfigurationProfile>> {
    let mut tx = app.pool.begin().await?;
    let result = store::profile(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(post,path="/api/profiles",request_body=CreateConfigurationProfile,responses((status=200,body=ConfigurationState),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Profiles")]
async fn create_profile(
    State(app): State<AppState>,
    input: std::result::Result<Json<CreateConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationState>> {
    let input = payload(input)?;
    let config = normalize(input.configuration)?;
    let mut tx = store::begin(&app).await?;
    let before = store::state(&mut tx).await?;
    store::revision(input.expected_state_revision, before.revision)?;
    let (name, key) = store::name(&mut tx, &input.name).await?;
    let issues = resources(&app, &mut tx, &config, true, true).await?;
    if !issues.is_empty() {
        return Err(ConfigurationError::resources(issues));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO configuration_profiles(id,name,name_key,configuration) VALUES($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(name)
    .bind(key)
    .bind(store::value(&config))
    .execute(&mut *tx)
    .await?;
    store::write_state(&mut tx, &config, id).await?;
    let mut result = store::state(&mut tx).await?;
    result.profile = Some(result.active_profile.clone());
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(patch,path="/api/profiles/{id}",params(("id"=Uuid,Path)),request_body=RenameConfigurationProfile,responses((status=200,body=ConfigurationState),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Profiles")]
async fn rename_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    input: std::result::Result<Json<RenameConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationState>> {
    let input = payload(input)?;
    let mut tx = store::begin(&app).await?;
    let before = store::state(&mut tx).await?;
    store::revision(input.expected_state_revision, before.revision)?;
    let target = store::profile(&mut tx, id).await?;
    store::revision(input.expected_profile_revision, target.revision)?;
    let (name, key) = store::name(&mut tx, &input.name).await?;
    if target.name != name {
        sqlx::query("UPDATE configuration_profiles SET name=$2,name_key=$3,revision=revision+1,updated_at=now() WHERE id=$1").bind(id).bind(name).bind(key).execute(&mut *tx).await?;
        store::bump(&mut tx).await?;
    }
    let mut result = store::state(&mut tx).await?;
    result.profile = Some(store::profile(&mut tx, id).await?);
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(put,path="/api/profiles/{id}/snapshot",params(("id"=Uuid,Path)),request_body=SaveConfigurationProfile,responses((status=200,body=ConfigurationState),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Profiles")]
async fn save_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    input: std::result::Result<Json<SaveConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationState>> {
    let input = payload(input)?;
    let config = normalize(input.configuration)?;
    let mut tx = store::begin(&app).await?;
    let before = store::state(&mut tx).await?;
    store::revision(input.expected_state_revision, before.revision)?;
    let target = store::profile(&mut tx, id).await?;
    store::revision(input.expected_profile_revision, target.revision)?;
    let snapshot_changed = store::value(&target.configuration) != store::value(&config);
    let current_changed = store::value(&before.current_configuration) != store::value(&config);
    if current_changed {
        let issues =
            changed_resources(&app, &mut tx, &before.current_configuration, &config).await?;
        if !issues.is_empty() {
            return Err(ConfigurationError::resources(issues));
        }
    }
    if snapshot_changed {
        sqlx::query("UPDATE configuration_profiles SET configuration=$2,revision=revision+1,updated_at=now() WHERE id=$1").bind(id).bind(store::value(&config)).execute(&mut *tx).await?;
    }
    if current_changed {
        store::write_state(&mut tx, &config, before.active_profile.id).await?;
    } else if snapshot_changed {
        store::bump(&mut tx).await?;
    }
    let mut result = store::state(&mut tx).await?;
    result.profile = Some(store::profile(&mut tx, id).await?);
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(delete,path="/api/profiles/{id}",params(("id"=Uuid,Path)),request_body=DeleteConfigurationProfile,responses((status=200,body=ConfigurationState),(status=409,body=ErrorBody)),tag="Profiles")]
async fn delete_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    input: std::result::Result<Json<DeleteConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationState>> {
    let input = payload(input)?;
    let mut tx = store::begin(&app).await?;
    let before = store::state(&mut tx).await?;
    store::revision(input.expected_state_revision, before.revision)?;
    let target = store::profile(&mut tx, id).await?;
    store::revision(input.expected_profile_revision, target.revision)?;
    if before.active_profile.id == id {
        return Err(ConfigurationError::new(
            StatusCode::CONFLICT,
            "active_profile",
            "Activate another profile before deleting this profile",
        ));
    }
    sqlx::query("DELETE FROM configuration_profiles WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    store::bump(&mut tx).await?;
    let result = store::state(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
#[utoipa::path(post,path="/api/profiles/{id}/validate",params(("id"=Uuid,Path)),request_body=ValidateConfigurationProfile,responses((status=200,body=ConfigurationValidation),(status=409,body=ErrorBody)),tag="Profiles")]
async fn validate_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    input: std::result::Result<Json<ValidateConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationValidation>> {
    let input = payload(input)?;
    let mut tx = store::begin(&app).await?;
    let target = store::profile(&mut tx, id).await?;
    store::revision(input.expected_profile_revision, target.revision)?;
    let config = normalize(target.configuration)?;
    let issues = resources(&app, &mut tx, &config, true, true).await?;
    tx.commit().await?;
    Ok(Json(ConfigurationValidation {
        valid: issues.is_empty(),
        issues,
    }))
}
#[utoipa::path(post,path="/api/profiles/{id}/activate",params(("id"=Uuid,Path)),request_body=ActivateConfigurationProfile,responses((status=200,body=ConfigurationState),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Profiles")]
async fn activate_profile(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    input: std::result::Result<Json<ActivateConfigurationProfile>, ApiError>,
) -> Result<Json<ConfigurationState>> {
    let input = payload(input)?;
    let current = normalize(input.current_configuration)?;
    let mut tx = store::begin(&app).await?;
    let before = store::state(&mut tx).await?;
    store::revision(input.expected_state_revision, before.revision)?;
    store::revision(
        input.expected_source_revision,
        before.active_profile.revision,
    )?;
    let target = store::profile(&mut tx, id).await?;
    store::revision(input.expected_target_revision, target.revision)?;
    if id == before.active_profile.id {
        tx.commit().await?;
        return Ok(Json(before));
    }
    let config = normalize(target.configuration)?;
    let issues = resources(&app, &mut tx, &config, true, true).await?;
    if !issues.is_empty() {
        return Err(ConfigurationError::resources(issues));
    }
    if matches!(input.source_action, ProfileSourceAction::SaveCurrent)
        && store::value(&before.active_profile.configuration) != store::value(&current)
    {
        sqlx::query("UPDATE configuration_profiles SET configuration=$2,revision=revision+1,updated_at=now() WHERE id=$1").bind(before.active_profile.id).bind(store::value(&current)).execute(&mut *tx).await?;
    }
    store::write_state(&mut tx, &config, id).await?;
    let result = store::state(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_profiles,
        get_profile,
        create_profile,
        rename_profile,
        save_profile,
        delete_profile,
        validate_profile,
        activate_profile
    ),
    components(schemas(
        CreateConfigurationProfile,
        RenameConfigurationProfile,
        SaveConfigurationProfile,
        DeleteConfigurationProfile,
        ValidateConfigurationProfile,
        ConfigurationValidation,
        ActivateConfigurationProfile,
        ProfileSourceAction
    ))
)]
pub struct ProfilesDoc;
