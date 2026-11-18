//! Complete, versioned personal configuration. Runtime work keeps its own snapshots.
pub(crate) mod store;
pub(crate) mod validation;

use crate::api::AppState;
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
        pub enum $name { $(#[serde(rename=$value)] $variant),+ }
    };
}
string_enum!(ConfigurationLocale { De=>"de", En=>"en", Es=>"es", Fr=>"fr", It=>"it", Pl=>"pl" });
string_enum!(ConfigurationEngine { Gnubg=>"gnubg", OpenSage=>"bgsage" });

static OPTIONS: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!("../../shared/configuration-options.json"))
        .expect("valid embedded configuration options")
});

/// Inclusive lower bounds of the four grades, measured in normalized equity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorThresholds {
    pub questionable: f64,
    pub error: f64,
    pub blunder: f64,
    pub large_blunder: f64,
}

impl Default for ErrorThresholds {
    fn default() -> Self {
        static DEFAULTS: std::sync::LazyLock<ErrorThresholds> = std::sync::LazyLock::new(|| {
            serde_json::from_value(OPTIONS["analysis_error_thresholds"].clone())
                .expect("valid embedded analysis error thresholds")
        });
        *DEFAULTS
    }
}

impl ErrorThresholds {
    pub(crate) fn valid(self) -> bool {
        0.0 < self.questionable
            && self.questionable < self.error
            && self.error < self.blunder
            && self.blunder < self.large_blunder
            && self.large_blunder <= 1.0
    }

    pub(crate) fn bands(self) -> [(crate::model::MoveSeverity, f64); 4] {
        use crate::model::MoveSeverity::*;
        [
            (VeryBad, self.large_blunder),
            (Blunder, self.blunder),
            (Bad, self.error),
            (Doubtful, self.questionable),
        ]
    }

    /// Classify a stored move row with the same bounds as live feedback.
    /// The caller supplies the loss and availability expressions from its query.
    pub(crate) fn push_classification<'a>(
        self,
        sql: &mut sqlx::QueryBuilder<'a, sqlx::Postgres>,
        loss: &str,
        status: &str,
    ) {
        sql.push("CASE WHEN COALESCE(")
            .push(status)
            .push(", '') NOT IN ('forced','unavailable') THEN CASE");
        for (grade, minimum) in self.bands() {
            sql.push(" WHEN ")
                .push(loss)
                .push(" >= ")
                .push_bind(minimum)
                .push(" THEN ")
                .push_bind(grade.as_str());
        }
        sql.push(" ELSE 'none' END ELSE 'none' END");
    }

    pub(crate) fn classify(self, loss: f64) -> crate::model::MoveSeverity {
        self.bands()
            .into_iter()
            .find(|&(_, minimum)| loss >= minimum)
            .map_or(crate::model::MoveSeverity::None, |(grade, _)| grade)
    }
}

pub(crate) fn available_cpu_threads() -> Option<i32> {
    std::thread::available_parallelism()
        .ok()
        .and_then(|count| i32::try_from(count.get()).ok())
}

/// Inclusive range and default for simultaneously running analysis jobs.
pub(crate) fn parallel_analysis_jobs() -> (i32, i32, i32) {
    let option = |name: &str| {
        OPTIONS["parallel_analysis_jobs"][name]
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .expect("parallel analysis job option is a valid integer")
    };
    (option("min"), option("max"), option("default"))
}

pub(crate) fn engine_min_threads(engine: &str) -> i32 {
    OPTIONS["engine_threads"][engine]["min"]
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .expect("valid embedded minimum thread count")
}

/// Verified provider profiles, shared with client normalization and catalogs.
pub(crate) fn engine_depths(engine: &str, purpose: &str) -> Vec<i32> {
    OPTIONS["engine_profiles"][engine][purpose]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()))
                .collect()
        })
        .unwrap_or_default()
}
string_enum!(ConfigurationScope { Game=>"game", Match=>"match" });
string_enum!(ConfigurationSearchFilter { EngineDefault=>"engine_default" });
string_enum!(ConfigurationMode { Light=>"light", Dark=>"dark", System=>"system" });
string_enum!(ConfigurationUiTheme { Graphite=>"graphite", Club=>"club", Paper=>"paper" });
string_enum!(ConfigurationDirection { Left=>"left", Right=>"right" });
string_enum!(ConfigurationBoardView { Before=>"before", Arrows=>"arrows", After=>"after" });
string_enum!(ConfigurationBookProvider { Off=>"off", Automatic=>"automatic", Backgammondb=>"backgammondb", Xg=>"xg" });
string_enum!(ConfigurationSortDirection { Asc=>"asc", Desc=>"desc" });

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfiguration {
    pub schema_version: i32,
    pub general: ConfigurationGeneral,
    pub analysis: ConfigurationAnalysis,
    pub play: ConfigurationPlay,
    pub appearance: ConfigurationAppearance,
    pub opening_book: ConfigurationBook,
    pub library: ConfigurationLibrary,
    pub positions: ConfigurationPositions,
    pub learning: ConfigurationLearning,
    pub annotations: ConfigurationAnnotations,
    pub review: ConfigurationReview,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationGeneral {
    pub player_name: String,
    pub locale: ConfigurationLocale,
    pub play_sounds: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationAnalysis {
    pub engine: ConfigurationEngine,
    pub plies: i32,
    pub scope: ConfigurationScope,
    pub search_filter: ConfigurationSearchFilter,
    #[serde(default)]
    pub error_thresholds: ErrorThresholds,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationPlay {
    pub engine: ConfigurationEngine,
    pub plies: i32,
    pub default_mode: crate::play::PlayMode,
    pub tutor_threshold: f64,
    pub training_auto_analysis: bool,
    pub human_player: i32,
    pub match_length: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationAppearance {
    pub ui_theme: ConfigurationUiTheme,
    pub mode: ConfigurationMode,
    pub board_theme: String,
    pub bearoff_direction: ConfigurationDirection,
    pub board_view: ConfigurationBoardView,
    /// Preferred board pane width in Study cards; null uses the responsive default.
    #[serde(default)]
    pub analysis_board_width: Option<i32>,
    /// Preferred board pane width in the analysis overview; null uses the responsive default.
    #[serde(default)]
    pub analysis_overview_board_width: Option<i32>,
    /// Preferred board pane width in the Errors tab; null uses the responsive default.
    #[serde(default)]
    pub analysis_errors_board_width: Option<i32>,
    /// Preferred board pane width in the Statistics tab; null uses the responsive default.
    #[serde(default)]
    pub analysis_statistics_board_width: Option<i32>,
    /// Preferred board pane width in the Notes tab; null uses the responsive default.
    #[serde(default)]
    pub analysis_notes_board_width: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationBook {
    pub provider: ConfigurationBookProvider,
    pub path: String,
    pub xg_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationLibraryFilters {
    pub q: String,
    pub player: String,
    pub date_from: String,
    pub date_to: String,
    pub date_field: String,
    pub tag: String,
    pub comment: String,
    pub position_id: String,
    pub tournament: String,
    pub round: String,
    pub match_name: String,
    pub match_type: String,
    pub result: String,
    pub match_points_min: String,
    pub match_points_max: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationLibrary {
    pub database_id: String,
    pub filters: ConfigurationLibraryFilters,
    pub sort: String,
    pub direction: ConfigurationSortDirection,
    pub visible_columns: Vec<String>,
    pub column_order: Vec<String>,
    pub column_widths: Option<BTreeMap<String, i32>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationPositions {
    #[serde(serialize_with = "serialize_position_definition")]
    pub definition: crate::position_browser::PositionFilterDefinition,
    pub show_played: bool,
    pub column_widths: Option<BTreeMap<String, i32>>,
    #[serde(default = "default_position_columns")]
    pub visible_columns: Vec<String>,
}
fn default_position_columns() -> Vec<String> {
    serde_json::from_value(OPTIONS["default_position_columns"].clone())
        .expect("valid embedded position column defaults")
}
fn serialize_position_definition<S: serde::Serializer>(
    definition: &crate::position_browser::PositionFilterDefinition,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    let mut value = serde_json::to_value(definition).map_err(serde::ser::Error::custom)?;
    if let Some(fields) = value.as_object_mut() {
        fields.retain(|_, value| !value.is_null());
    }
    value.serialize(serializer)
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationCardFilters {
    pub q: String,
    pub player: String,
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub topics: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub game_plan: String,
    pub comment: String,
    pub position_id: String,
    pub kind: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationGeneration {
    pub min_loss: f64,
    pub player: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationLearning {
    pub deck_id: String,
    pub filters: ConfigurationCardFilters,
    pub generation: ConfigurationGeneration,
}
impl ConfigurationLearning {
    fn is_collection_scope(&self) -> bool {
        matches!(self.deck_id.as_str(), "__all__" | "__without_deck__")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationAnnotationFilters {
    pub q: String,
    pub tag: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationAnnotations {
    pub filters: Option<ConfigurationAnnotationFilters>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationReview {
    pub player: String,
    pub kind: String,
    pub severity: String,
    pub errors_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationProfile {
    pub id: Uuid,
    pub name: String,
    pub revision: i64,
    pub configuration: ProfileConfiguration,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationDifference {
    pub path: String,
    pub saved: Value,
    pub current: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationState {
    pub revision: i64,
    pub active_profile: ConfigurationProfile,
    pub current_configuration: ProfileConfiguration,
    pub dirty: bool,
    pub differences: Vec<ConfigurationDifference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ConfigurationProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initialization_warnings: Vec<ConfigurationIssue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigurationErrorBody {
    pub error: String,
    pub code: String,
    pub issues: Vec<ConfigurationIssue>,
}
pub(crate) struct ConfigurationError(pub StatusCode, pub ConfigurationErrorBody);
pub(crate) type Result<T> = std::result::Result<T, ConfigurationError>;
impl ConfigurationError {
    pub(crate) fn new(status: StatusCode, code: &str, error: impl Into<String>) -> Self {
        Self(
            status,
            ConfigurationErrorBody {
                error: error.into(),
                code: code.into(),
                issues: vec![],
            },
        )
    }
    pub(crate) fn conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "Configuration changed; reload before saving",
        )
    }
    pub(crate) fn invalid(path: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            ConfigurationErrorBody {
                error: message.clone(),
                code: "invalid_configuration".into(),
                issues: vec![ConfigurationIssue {
                    path: path.into(),
                    code: "invalid_value".into(),
                    message,
                }],
            },
        )
    }
    pub(crate) fn resources(issues: Vec<ConfigurationIssue>) -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            ConfigurationErrorBody {
                error: "Configuration resources are unavailable".into(),
                code: "unavailable_resources".into(),
                issues,
            },
        )
    }
}
impl From<sqlx::Error> for ConfigurationError {
    fn from(error: sqlx::Error) -> Self {
        if error
            .as_database_error()
            .is_some_and(|e| e.is_unique_violation())
        {
            Self::new(
                StatusCode::CONFLICT,
                "duplicate_name",
                "A profile with this name already exists",
            )
        } else {
            tracing::error!(?error, "Configuration database operation failed");
            Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Configuration could not be saved",
            )
        }
    }
}
impl From<anyhow::Error> for ConfigurationError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(?error, "Configuration operation failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Configuration operation failed",
        )
    }
}
impl IntoResponse for ConfigurationError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}
pub(crate) fn payload<T>(value: std::result::Result<Json<T>, JsonRejection>) -> Result<T> {
    value
        .map(|Json(v)| v)
        .map_err(|e| ConfigurationError::invalid("configuration", e.body_text()))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InitializeConfiguration {
    pub expected_settings_version: i64,
    pub expected_book_version: i64,
    pub configuration: ProfileConfiguration,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfiguration {
    pub expected_revision: i64,
    pub configuration: ProfileConfiguration,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreConfiguration {
    pub expected_state_revision: i64,
    pub expected_profile_revision: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigurationAvailability {
    pub revision: i64,
    pub valid: bool,
    pub issues: Vec<ConfigurationIssue>,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/configuration",
            get(get_configuration).put(put_configuration),
        )
        .route(
            "/api/configuration/initialize",
            post(initialize_configuration),
        )
        .route(
            "/api/configuration/restore-profile",
            post(restore_configuration),
        )
        .route(
            "/api/configuration/availability",
            get(configuration_availability),
        )
}
#[utoipa::path(get,path="/api/configuration/availability",responses((status=200,body=ConfigurationAvailability),(status=409,body=ConfigurationErrorBody)),tag="Configuration")]
async fn configuration_availability(
    State(app): State<AppState>,
) -> Result<Json<ConfigurationAvailability>> {
    let mut tx = store::begin(&app).await?;
    let current = store::state(&mut tx).await?;
    let issues =
        validation::resources(&app, &mut tx, &current.current_configuration, true, true).await?;
    tx.commit().await?;
    Ok(Json(ConfigurationAvailability {
        revision: current.revision,
        valid: issues.is_empty(),
        issues,
    }))
}
#[utoipa::path(get,path="/api/configuration",responses((status=200,body=ConfigurationState),(status=409,body=ConfigurationErrorBody)),tag="Configuration")]
async fn get_configuration(State(state): State<AppState>) -> Result<Json<ConfigurationState>> {
    Ok(Json(store::read(&state.pool).await?))
}
#[utoipa::path(post,path="/api/configuration/initialize",request_body=InitializeConfiguration,responses((status=200,body=ConfigurationState),(status=409,body=ConfigurationErrorBody),(status=422,body=ConfigurationErrorBody)),tag="Configuration")]
async fn initialize_configuration(
    State(state): State<AppState>,
    input: std::result::Result<Json<InitializeConfiguration>, JsonRejection>,
) -> Result<Json<ConfigurationState>> {
    Ok(Json(store::initialize(&state, payload(input)?).await?))
}
#[utoipa::path(put,path="/api/configuration",request_body=UpdateConfiguration,responses((status=200,body=ConfigurationState),(status=409,body=ConfigurationErrorBody),(status=422,body=ConfigurationErrorBody)),tag="Configuration")]
async fn put_configuration(
    State(state): State<AppState>,
    input: std::result::Result<Json<UpdateConfiguration>, JsonRejection>,
) -> Result<Json<ConfigurationState>> {
    Ok(Json(store::update(&state, payload(input)?).await?))
}
#[utoipa::path(post,path="/api/configuration/restore-profile",request_body=RestoreConfiguration,responses((status=200,body=ConfigurationState),(status=409,body=ConfigurationErrorBody),(status=422,body=ConfigurationErrorBody)),tag="Configuration")]
async fn restore_configuration(
    State(state): State<AppState>,
    input: std::result::Result<Json<RestoreConfiguration>, JsonRejection>,
) -> Result<Json<ConfigurationState>> {
    Ok(Json(store::restore(&state, payload(input)?).await?))
}

pub fn differences(
    saved: &ProfileConfiguration,
    current: &ProfileConfiguration,
) -> Vec<ConfigurationDifference> {
    fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<ConfigurationDifference>) {
        if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
            let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                walk(
                    &if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    },
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    out,
                );
            }
        } else if a != b {
            out.push(ConfigurationDifference {
                path: path.into(),
                saved: a.clone(),
                current: b.clone(),
            });
        }
    }
    let mut result = vec![];
    walk(
        "",
        &serde_json::to_value(saved).expect("serializable configuration"),
        &serde_json::to_value(current).expect("serializable configuration"),
        &mut result,
    );
    result
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_configuration,
        initialize_configuration,
        put_configuration,
        restore_configuration,
        configuration_availability
    ),
    components(schemas(
        ProfileConfiguration,
        ErrorThresholds,
        ConfigurationState,
        ConfigurationProfile,
        ConfigurationDifference,
        ConfigurationErrorBody,
        ConfigurationIssue,
        InitializeConfiguration,
        UpdateConfiguration,
        RestoreConfiguration,
        ConfigurationAvailability
    ))
)]
pub struct ConfigurationDoc;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analysis_board_widths_are_optional_and_bounded() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/configuration-normalization.json"
        ))
        .unwrap();
        let legacy: ProfileConfiguration = serde_json::from_value(fixture["base"].clone()).unwrap();
        let legacy = serde_json::to_value(
            validation::normalize(legacy).unwrap_or_else(|error| panic!("{}", error.1.error)),
        )
        .unwrap();
        for (field, max) in [
            ("analysis_board_width", 1680),
            ("analysis_overview_board_width", 1680),
            ("analysis_errors_board_width", 1680),
            ("analysis_statistics_board_width", 1680),
            ("analysis_notes_board_width", 1680),
        ] {
            assert_eq!(legacy["appearance"].get(field), Some(&Value::Null));
            for width in [320, 540, max] {
                let mut value = fixture["base"].clone();
                value["appearance"][field] = serde_json::json!(width);
                let decoded = serde_json::from_value(value).unwrap();
                assert!(validation::normalize(decoded).is_ok());
            }
            for width in [0, 319, max + 1] {
                let mut value = fixture["base"].clone();
                value["appearance"][field] = serde_json::json!(width);
                let decoded = serde_json::from_value(value).unwrap();
                assert!(validation::normalize(decoded).is_err());
            }
        }
    }
    #[test]
    fn independent_engines_accept_only_verified_sage_profiles() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/configuration-normalization.json"
        ))
        .unwrap();
        for area in ["analysis", "play"] {
            let mut value = fixture["base"].clone();
            value[area]["engine"] = serde_json::json!("bgsage");
            value[area]["plies"] = serde_json::json!(2);
            let decoded = serde_json::from_value::<ProfileConfiguration>(value.clone());
            assert!(
                decoded.is_ok(),
                "verified Open Sage must be selectable in {area}"
            );
            assert!(validation::normalize(decoded.unwrap()).is_ok());
            value[area]["plies"] = serde_json::json!(1);
            assert!(validation::normalize(serde_json::from_value(value.clone()).unwrap()).is_ok());
            if area == "analysis" {
                for depth in [3, 4] {
                    value[area]["plies"] = serde_json::json!(depth);
                    assert!(
                        validation::normalize(serde_json::from_value(value.clone()).unwrap())
                            .is_ok()
                    );
                }
            }
            let unverified_depths: &[i32] = if area == "analysis" {
                &[0, 5]
            } else {
                &[0, 3, 4]
            };
            for &unverified in unverified_depths {
                value[area]["plies"] = serde_json::json!(unverified);
                let decoded =
                    serde_json::from_value::<ProfileConfiguration>(value.clone()).unwrap();
                assert!(
                    validation::normalize(decoded).is_err(),
                    "unverified Sage {unverified}-ply accepted in {area}"
                );
            }
        }
    }
    #[test]
    fn normalization_matches_shared_semantic_vectors() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/configuration-normalization.json"
        ))
        .unwrap();
        let saved = validation::normalize(serde_json::from_value(fixture["base"].clone()).unwrap())
            .unwrap_or_else(|e| panic!("{}", e.1.error));
        for case in fixture["cases"].as_array().unwrap() {
            let mut changed = fixture["base"].clone();
            for change in case["changes"].as_array().unwrap() {
                let segments: Vec<_> = change["path"].as_str().unwrap().split('.').collect();
                let mut cursor = &mut changed;
                for segment in &segments[..segments.len() - 1] {
                    cursor = &mut cursor[*segment];
                }
                cursor[*segments.last().unwrap()] = change["value"].clone();
            }
            let current = validation::normalize(serde_json::from_value(changed).unwrap())
                .unwrap_or_else(|e| panic!("{}: {}", case["name"], e.1.error));
            let actual: Vec<_> = differences(&saved, &current)
                .into_iter()
                .map(|d| d.path)
                .collect();
            let expected: Vec<_> = case["expected_paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(actual, expected, "{}", case["name"]);
        }
    }
}
