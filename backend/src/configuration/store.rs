use super::validation::{enum_string, normalize, resources, upgrade_columns};
use super::*;
use sqlx::{PgPool, Postgres, Row, Transaction};

pub(crate) async fn error_thresholds<'e>(
    executor: impl sqlx::PgExecutor<'e>,
) -> anyhow::Result<ErrorThresholds> {
    let saved: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(
        "SELECT current_configuration #> '{analysis,error_thresholds}' FROM configuration_state WHERE singleton",
    )
    .fetch_optional(executor)
    .await?
    .flatten();
    if let Some(sqlx::types::Json(value)) = saved {
        match serde_json::from_value::<ErrorThresholds>(value) {
            Ok(thresholds) if thresholds.valid() => return Ok(thresholds),
            _ => tracing::warn!("Invalid saved error thresholds; using defaults"),
        }
    }
    Ok(ErrorThresholds::default())
}

pub(crate) async fn lock(
    tx: &mut Transaction<'_, Postgres>,
) -> std::result::Result<(), sqlx::Error> {
    crate::advisory_locks::xact_lock(tx, crate::advisory_locks::CONFIGURATION_PROFILES).await
}
pub(crate) async fn begin(state: &AppState) -> Result<Transaction<'_, Postgres>> {
    crate::settings::load(state).await?;
    crate::opening_book::load(&state.pool).await?;
    let mut tx = state.pool.begin().await?;
    lock(&mut tx).await?;
    Ok(tx)
}
fn configuration(value: Value) -> Result<ProfileConfiguration> {
    let mut config = serde_json::from_value(value)
        .map_err(|e| ConfigurationError::invalid("configuration", e.to_string()))?;
    upgrade_columns(&mut config);
    Ok(config)
}
pub(crate) fn value(config: &ProfileConfiguration) -> Value {
    serde_json::to_value(config).expect("serializable configuration")
}
const PROFILE_SELECT: &str =
    "SELECT id,name,revision,configuration,created_at,updated_at FROM configuration_profiles";
fn profile_row(row: sqlx::postgres::PgRow) -> Result<ConfigurationProfile> {
    Ok(ConfigurationProfile {
        id: row.get("id"),
        name: row.get("name"),
        revision: row.get("revision"),
        configuration: configuration(row.get("configuration"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
pub(crate) async fn profile(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<ConfigurationProfile> {
    let row = sqlx::query(&format!("{PROFILE_SELECT} WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            ConfigurationError::new(
                StatusCode::NOT_FOUND,
                "profile_not_found",
                "Profile not found",
            )
        })?;
    profile_row(row)
}
/// All profiles in display order, read with one query.
pub(crate) async fn profiles(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<ConfigurationProfile>> {
    sqlx::query(&format!("{PROFILE_SELECT} ORDER BY name_key,id"))
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(profile_row)
        .collect()
}
pub(crate) async fn state(tx: &mut Transaction<'_, Postgres>) -> Result<ConfigurationState> {
    let row=sqlx::query("SELECT revision,active_profile_id,current_configuration FROM configuration_state WHERE singleton=true FOR UPDATE").fetch_optional(&mut **tx).await?.ok_or_else(||ConfigurationError::new(StatusCode::CONFLICT,"configuration_not_initialized","Configuration has not been initialized"))?;
    let active_profile = profile(tx, row.get("active_profile_id")).await?;
    let current_configuration = configuration(row.get("current_configuration"))?;
    let differences = super::differences(&active_profile.configuration, &current_configuration);
    Ok(ConfigurationState {
        revision: row.get("revision"),
        active_profile,
        current_configuration,
        dirty: !differences.is_empty(),
        differences,
        profile: None,
        initialization_warnings: vec![],
    })
}
pub(crate) async fn read(pool: &PgPool) -> Result<ConfigurationState> {
    let mut tx = pool.begin().await?;
    lock(&mut tx).await?;
    let result = state(&mut tx).await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) fn revision(expected: i64, actual: i64) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(ConfigurationError::conflict())
    }
}
pub(crate) async fn name(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
) -> Result<(String, String)> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(ConfigurationError::invalid(
            "name",
            "Profile name must contain 1–80 characters and no control characters",
        ));
    }
    let (display, key): (String, String) =
        sqlx::query_as("SELECT normalize($1, NFC),lower(normalize($1, NFC))")
            .bind(trimmed)
            .fetch_one(&mut **tx)
            .await?;
    if display.chars().count() > 80 {
        return Err(ConfigurationError::invalid(
            "name",
            "Profile name must contain at most 80 characters",
        ));
    }
    Ok((display, key))
}
pub(crate) async fn initialize(
    state_ref: &AppState,
    input: InitializeConfiguration,
) -> Result<ConfigurationState> {
    let mut tx = begin(state_ref).await?;
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM configuration_state)")
        .fetch_one(&mut *tx)
        .await?
    {
        let result = state(&mut tx).await?;
        tx.commit().await?;
        return Ok(result);
    }
    let app: crate::settings::AppSettings =
        sqlx::query_as("SELECT * FROM app_settings WHERE singleton=true FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
    let book: crate::opening_book::BookConfig =
        sqlx::query_as("SELECT * FROM opening_book_settings WHERE singleton=true FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
    revision(input.expected_settings_version, app.version)?;
    revision(input.expected_book_version, book.version)?;
    let mut config = input.configuration;
    apply_app(&mut config, &app);
    apply_book(&mut config, &book);
    let mut warnings = vec![];
    if !config.library.database_id.is_empty()
        && !exists(&mut tx, "databases", &config.library.database_id).await?
    {
        config.library.database_id.clear();
        warnings.push(ConfigurationIssue {
            path: "library.database_id".into(),
            code: "legacy_reference_missing".into(),
            engine: None,
            message: "The previously selected database no longer exists; all databases are shown"
                .into(),
        });
    }
    if !config.learning.is_collection_scope()
        && !exists(&mut tx, "decks", &config.learning.deck_id).await?
    {
        config.learning.deck_id = "__all__".into();
        warnings.push(ConfigurationIssue{path:"learning.deck_id".into(),code:"legacy_reference_missing".into(),engine:None,message:"The previously selected deck no longer exists; all cards were selected during migration".into()});
    }
    let config = normalize(config)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO configuration_profiles(id,name,name_key,configuration) VALUES($1,'Standard','standard',$2)").bind(id).bind(value(&config)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO configuration_state(singleton,active_profile_id,current_configuration) VALUES(true,$1,$2)").bind(id).bind(value(&config)).execute(&mut *tx).await?;
    let mut result = state(&mut tx).await?;
    result.initialization_warnings = warnings;
    tx.commit().await?;
    Ok(result)
}
async fn exists(tx: &mut Transaction<'_, Postgres>, table: &str, id: &str) -> Result<bool> {
    let Ok(id) = Uuid::parse_str(id) else {
        return Ok(false);
    };
    Ok(
        sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=$1)"))
            .bind(id)
            .fetch_one(&mut **tx)
            .await?,
    )
}
pub(crate) fn apply_app(config: &mut ProfileConfiguration, app: &crate::settings::AppSettings) {
    config.general.player_name = app.player_name.clone();
    // Saved settings hold verified engine IDs; an unknown one reads as GNUbg.
    let engine = |id: &str| {
        ConfigurationEngine::from(
            crate::engines::EngineId::parse(id).unwrap_or(crate::engines::EngineId::Gnubg),
        )
    };
    config.analysis.engine = engine(&app.analysis_engine);
    config.play.engine = engine(&app.play_engine);
    config.analysis.plies = app.analysis_plies;
    config.play.plies = app.play_plies;
    config.play.top_two = app.play_top_two;
    config.play.default_mode = app.default_play_mode;
    config.play.tutor_threshold = app.tutor_threshold;
    config.play.training_auto_analysis = app.training_auto_analysis;
    config.appearance.bearoff_direction = if app.bearoff_direction == "left" {
        ConfigurationDirection::Left
    } else {
        ConfigurationDirection::Right
    };
}
pub(crate) fn apply_book(
    config: &mut ProfileConfiguration,
    book: &crate::opening_book::BookConfig,
) {
    config.opening_book = ConfigurationBook {
        provider: match book.provider.as_str() {
            "off" => ConfigurationBookProvider::Off,
            "xg" => ConfigurationBookProvider::Xg,
            "backgammondb" => ConfigurationBookProvider::Backgammondb,
            _ => ConfigurationBookProvider::Automatic,
        },
        path: book.path.clone(),
        xg_path: book.xg_path.clone(),
    };
}
pub(crate) async fn projections(
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
) -> Result<()> {
    let a = &config.analysis;
    let p = &config.play;
    sqlx::query("UPDATE app_settings SET version=version+1,player_name=$1,analysis_engine=$2,analysis_plies=$3,play_engine=$4,play_plies=$5,bearoff_direction=$6,default_play_mode=$7,tutor_threshold=$8,training_auto_analysis=$9,play_top_two=$10 WHERE singleton=true AND (player_name,analysis_engine,analysis_plies,play_engine,play_plies,bearoff_direction,default_play_mode,tutor_threshold,training_auto_analysis,play_top_two) IS DISTINCT FROM ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(&config.general.player_name).bind(enum_string(&a.engine)).bind(a.plies).bind(enum_string(&p.engine)).bind(p.plies).bind(enum_string(&config.appearance.bearoff_direction)).bind(p.default_mode).bind(p.tutor_threshold).bind(p.training_auto_analysis).bind(p.top_two).execute(&mut **tx).await?;
    let b = &config.opening_book;
    sqlx::query("UPDATE opening_book_settings SET version=version+1,provider=$1,path=$2,xg_path=$3 WHERE singleton=true AND (provider,path,xg_path) IS DISTINCT FROM ($1,$2,$3)").bind(enum_string(&b.provider)).bind(&b.path).bind(&b.xg_path).execute(&mut **tx).await?;
    Ok(())
}
pub(crate) async fn write_state(
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
    active: Uuid,
) -> Result<()> {
    projections(tx, config).await?;
    sqlx::query("UPDATE configuration_state SET revision=revision+1,current_configuration=$1,active_profile_id=$2,updated_at=now() WHERE singleton=true").bind(value(config)).bind(active).execute(&mut **tx).await?;
    Ok(())
}
pub(crate) async fn bump(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query(
        "UPDATE configuration_state SET revision=revision+1,updated_at=now() WHERE singleton=true",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
pub(crate) async fn update(
    state_ref: &AppState,
    input: UpdateConfiguration,
) -> Result<ConfigurationState> {
    let config = normalize(input.configuration)?;
    let mut tx = begin(state_ref).await?;
    let before = state(&mut tx).await?;
    revision(input.expected_revision, before.revision)?;
    if value(&config) != value(&before.current_configuration) {
        let old = &before.current_configuration;
        let engine = config.analysis.engine != old.analysis.engine
            || config.analysis.plies != old.analysis.plies
            || config.play.engine != old.play.engine
            || config.play.plies != old.play.plies
            || config.play.top_two != old.play.top_two;
        let book = value(&config)["opening_book"] != value(old)["opening_book"];
        let issues = resources(state_ref, &mut tx, &config, engine, book).await?;
        if !issues.is_empty() {
            return Err(ConfigurationError::resources(issues));
        }
        write_state(&mut tx, &config, before.active_profile.id).await?;
    }
    let result = state(&mut tx).await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) async fn restore(
    state_ref: &AppState,
    input: RestoreConfiguration,
) -> Result<ConfigurationState> {
    let mut tx = begin(state_ref).await?;
    let before = state(&mut tx).await?;
    revision(input.expected_state_revision, before.revision)?;
    revision(
        input.expected_profile_revision,
        before.active_profile.revision,
    )?;
    let config = normalize(before.active_profile.configuration.clone())?;
    let issues = resources(state_ref, &mut tx, &config, true, true).await?;
    if !issues.is_empty() {
        return Err(ConfigurationError::resources(issues));
    }
    if before.dirty {
        write_state(&mut tx, &config, before.active_profile.id).await?;
    }
    let result = state(&mut tx).await?;
    tx.commit().await?;
    Ok(result)
}

/// Called by legacy setters in their existing transaction after projection update.
pub(crate) async fn legacy_app(
    tx: &mut Transaction<'_, Postgres>,
    app: &crate::settings::AppSettings,
) -> Result<()> {
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM configuration_state)")
        .fetch_one(&mut **tx)
        .await?
    {
        return Ok(());
    }
    let mut state = state(tx).await?;
    apply_app(&mut state.current_configuration, app);
    sqlx::query("UPDATE configuration_state SET revision=revision+1,current_configuration=$1,updated_at=now() WHERE singleton=true").bind(value(&state.current_configuration)).execute(&mut **tx).await?;
    Ok(())
}
pub(crate) async fn legacy_book(
    tx: &mut Transaction<'_, Postgres>,
    book: &crate::opening_book::BookConfig,
) -> Result<()> {
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM configuration_state)")
        .fetch_one(&mut **tx)
        .await?
    {
        return Ok(());
    }
    let mut state = state(tx).await?;
    apply_book(&mut state.current_configuration, book);
    sqlx::query("UPDATE configuration_state SET revision=revision+1,current_configuration=$1,updated_at=now() WHERE singleton=true").bind(value(&state.current_configuration)).execute(&mut **tx).await?;
    Ok(())
}
