use super::validation::{changed_resources, enum_string, normalize, resources, upgrade_columns};
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
fn configuration(mut value: Value) -> Result<ProfileConfiguration> {
    if let Some(locale) = value.pointer_mut("/general/locale")
        && matches!(locale.as_str(), Some("es" | "fr" | "it" | "pl"))
    {
        *locale = Value::String("en".into());
    }
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
    if initialized(&mut tx).await? {
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
    ProfileSettings::from(&app).apply(&mut config);
    config.opening_book = ConfigurationBook::from(&book);
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
/// The profile settings that `app_settings` repeats for engine and job code.
/// The other columns of that table belong to the installation.
struct ProfileSettings {
    player_name: String,
    analysis_engine: ConfigurationEngine,
    analysis_plies: i32,
    play_engine: ConfigurationEngine,
    play_plies: i32,
    play_top_two: bool,
    bearoff_direction: ConfigurationDirection,
    default_play_mode: crate::play::PlayMode,
    tutor_threshold: f64,
    training_auto_analysis: bool,
}
impl From<&crate::settings::AppSettings> for ProfileSettings {
    fn from(app: &crate::settings::AppSettings) -> Self {
        // Saved settings hold verified engine IDs; an unknown one reads as GNUbg.
        let engine = |id: &str| {
            ConfigurationEngine::from(
                crate::engines::EngineId::parse(id).unwrap_or(crate::engines::EngineId::Gnubg),
            )
        };
        Self {
            player_name: app.player_name.clone(),
            analysis_engine: engine(&app.analysis_engine),
            analysis_plies: app.analysis_plies,
            play_engine: engine(&app.play_engine),
            play_plies: app.play_plies,
            play_top_two: app.play_top_two,
            bearoff_direction: if app.bearoff_direction == "left" {
                ConfigurationDirection::Left
            } else {
                ConfigurationDirection::Right
            },
            default_play_mode: app.default_play_mode,
            tutor_threshold: app.tutor_threshold,
            training_auto_analysis: app.training_auto_analysis,
        }
    }
}
impl From<&ProfileConfiguration> for ProfileSettings {
    fn from(config: &ProfileConfiguration) -> Self {
        Self {
            player_name: config.general.player_name.clone(),
            analysis_engine: config.analysis.engine.clone(),
            analysis_plies: config.analysis.plies,
            play_engine: config.play.engine.clone(),
            play_plies: config.play.plies,
            play_top_two: config.play.top_two,
            bearoff_direction: config.appearance.bearoff_direction.clone(),
            default_play_mode: config.play.default_mode,
            tutor_threshold: config.play.tutor_threshold,
            training_auto_analysis: config.play.training_auto_analysis,
        }
    }
}
impl ProfileSettings {
    fn apply(self, config: &mut ProfileConfiguration) {
        config.general.player_name = self.player_name;
        config.analysis.engine = self.analysis_engine;
        config.analysis.plies = self.analysis_plies;
        config.play.engine = self.play_engine;
        config.play.plies = self.play_plies;
        config.play.top_two = self.play_top_two;
        config.play.default_mode = self.default_play_mode;
        config.play.tutor_threshold = self.tutor_threshold;
        config.play.training_auto_analysis = self.training_auto_analysis;
        config.appearance.bearoff_direction = self.bearoff_direction;
    }

    /// The only statement that changes profile settings in `app_settings`.
    async fn project(&self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        sqlx::query("UPDATE app_settings SET version=version+1,player_name=$1,analysis_engine=$2,analysis_plies=$3,play_engine=$4,play_plies=$5,bearoff_direction=$6,default_play_mode=$7,tutor_threshold=$8,training_auto_analysis=$9,play_top_two=$10 WHERE singleton=true AND (player_name,analysis_engine,analysis_plies,play_engine,play_plies,bearoff_direction,default_play_mode,tutor_threshold,training_auto_analysis,play_top_two) IS DISTINCT FROM ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&self.player_name)
            .bind(enum_string(&self.analysis_engine))
            .bind(self.analysis_plies)
            .bind(enum_string(&self.play_engine))
            .bind(self.play_plies)
            .bind(enum_string(&self.bearoff_direction))
            .bind(self.default_play_mode)
            .bind(self.tutor_threshold)
            .bind(self.training_auto_analysis)
            .bind(self.play_top_two)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}
impl From<&crate::opening_book::BookConfig> for ConfigurationBook {
    fn from(book: &crate::opening_book::BookConfig) -> Self {
        Self {
            provider: match book.provider.as_str() {
                "off" => ConfigurationBookProvider::Off,
                "xg" => ConfigurationBookProvider::Xg,
                "backgammondb" => ConfigurationBookProvider::Backgammondb,
                _ => ConfigurationBookProvider::Automatic,
            },
            path: book.path.clone(),
            xg_path: book.xg_path.clone(),
        }
    }
}
impl ConfigurationBook {
    /// The only statement that changes a saved book choice. Only the defaults of
    /// a new or older installation are filled in elsewhere, by `opening_book::load`.
    async fn project(&self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        sqlx::query("UPDATE opening_book_settings SET version=version+1,provider=$1,path=$2,xg_path=$3 WHERE singleton=true AND (provider,path,xg_path) IS DISTINCT FROM ($1,$2,$3)")
            .bind(enum_string(&self.provider))
            .bind(&self.path)
            .bind(&self.xg_path)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}
async fn project_settings(
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
) -> Result<()> {
    ProfileSettings::from(config).project(tx).await?;
    config.opening_book.project(tx).await
}

/// The one way to change the current configuration. The profile settings that
/// engine, job and book code read from their own tables follow it.
pub(crate) async fn write_state(
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
    active: Uuid,
) -> Result<()> {
    project_settings(tx, config).await?;
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
        let issues =
            changed_resources(state_ref, &mut tx, &before.current_configuration, &config).await?;
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

async fn initialized(tx: &mut Transaction<'_, Postgres>) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM configuration_state)")
            .fetch_one(&mut **tx)
            .await?,
    )
}
/// Compare the effective configuration so a legacy row with defaulted fields
/// also keeps its revision. Refresh the derived settings tables in either case.
async fn edit_current_settings(
    tx: &mut Transaction<'_, Postgres>,
    edit: impl FnOnce(&mut ProfileConfiguration),
) -> Result<()> {
    let state = state(tx).await?;
    let mut config = state.current_configuration;
    let before = value(&config);
    edit(&mut config);
    if value(&config) == before {
        return project_settings(tx, &config).await;
    }
    write_state(tx, &config, state.active_profile.id).await
}

/// Saves the profile settings of `PUT /api/settings`. Until the configuration
/// is initialized, which imports them, the settings table is their only place.
pub(crate) async fn save_app(
    tx: &mut Transaction<'_, Postgres>,
    app: &crate::settings::AppSettings,
) -> Result<()> {
    let settings = ProfileSettings::from(app);
    if !initialized(tx).await? {
        return settings.project(tx).await;
    }
    edit_current_settings(tx, |config| settings.apply(config)).await
}
/// Saves the book choice of `PUT /api/opening-book`, like [`save_app`].
pub(crate) async fn save_book(
    tx: &mut Transaction<'_, Postgres>,
    book: &crate::opening_book::BookConfig,
) -> Result<()> {
    let book = ConfigurationBook::from(book);
    if !initialized(tx).await? {
        return book.project(tx).await;
    }
    edit_current_settings(tx, |config| config.opening_book = book).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, opening_book::BookConfig, settings::AppSettings};

    fn base() -> ProfileConfiguration {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/configuration-normalization.json"
        ))
        .unwrap();
        normalize(serde_json::from_value(fixture["base"].clone()).unwrap()).unwrap()
    }

    #[test]
    fn retired_locales_preserve_the_rest_of_a_saved_configuration() {
        let expected = value(&changed());
        for locale in ["es", "fr", "it", "pl"] {
            let mut legacy = expected.clone();
            legacy["general"]["locale"] = Value::String(locale.into());
            assert_eq!(value(&configuration(legacy).unwrap()), expected);
        }
    }

    /// Every profile setting differs from `base` and from the table defaults.
    fn changed() -> ProfileConfiguration {
        let mut config = base();
        config.general.player_name = "Changed".into();
        config.analysis.engine = ConfigurationEngine::OpenSage;
        config.analysis.plies = 2;
        config.play.engine = ConfigurationEngine::OpenSage;
        config.play.plies = 2;
        config.play.top_two = true;
        config.play.default_mode = crate::play::PlayMode::Tutor;
        config.play.tutor_threshold = 0.125;
        config.play.training_auto_analysis = false;
        config.appearance.bearoff_direction = ConfigurationDirection::Left;
        config.opening_book = ConfigurationBook {
            provider: ConfigurationBookProvider::Xg,
            path: "/changed/book.sqlite".into(),
            xg_path: "/changed/XG.ob".into(),
        };
        config
    }

    async fn app(pool: &PgPool) -> AppSettings {
        sqlx::query_as("SELECT * FROM app_settings")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn book(pool: &PgPool) -> BookConfig {
        sqlx::query_as("SELECT version,provider,path,xg_path FROM opening_book_settings")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// The tables must repeat the profile settings of the configuration.
    async fn assert_projected(pool: &PgPool, expected: &ProfileConfiguration) {
        let mut projected = base();
        ProfileSettings::from(&app(pool).await).apply(&mut projected);
        projected.opening_book = ConfigurationBook::from(&book(pool).await);
        assert_eq!(value(&projected), value(expected));
        let mut tx = pool.begin().await.unwrap();
        if initialized(&mut tx).await.unwrap() {
            let state = state(&mut tx).await.unwrap();
            assert_eq!(value(&state.current_configuration), value(expected));
        }
    }

    async fn rows(pool: &PgPool) -> AppState {
        let state = AppState::new(pool.clone(), Engine::new("/missing/gnubg".into()));
        begin(&state).await.unwrap().commit().await.unwrap();
        state
    }

    async fn initialize_base(pool: &PgPool) {
        let state = rows(pool).await;
        let mut tx = pool.begin().await.unwrap();
        let config = base();
        ProfileSettings::from(&config)
            .project(&mut tx)
            .await
            .unwrap();
        config.opening_book.project(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        initialize(
            &state,
            InitializeConfiguration {
                expected_settings_version: app(pool).await.version,
                expected_book_version: book(pool).await.version,
                configuration: config,
            },
        )
        .await
        .unwrap();
        assert_projected(pool, &base()).await;
    }

    fn settings(current: AppSettings, config: &ProfileConfiguration) -> AppSettings {
        AppSettings {
            player_name: config.general.player_name.clone(),
            analysis_engine: enum_string(&config.analysis.engine),
            analysis_plies: config.analysis.plies,
            play_engine: enum_string(&config.play.engine),
            play_plies: config.play.plies,
            play_top_two: config.play.top_two,
            bearoff_direction: enum_string(&config.appearance.bearoff_direction),
            default_play_mode: config.play.default_mode,
            tutor_threshold: config.play.tutor_threshold,
            training_auto_analysis: config.play.training_auto_analysis,
            ..current
        }
    }

    fn book_settings(config: &ProfileConfiguration) -> BookConfig {
        BookConfig {
            version: 0,
            provider: enum_string(&config.opening_book.provider),
            path: config.opening_book.path.clone(),
            xg_path: config.opening_book.xg_path.clone(),
        }
    }

    async fn save_all(pool: &PgPool, config: &ProfileConfiguration) {
        let app = settings(app(pool).await, config);
        let mut tx = pool.begin().await.unwrap();
        save_app(&mut tx, &app).await.unwrap();
        save_book(&mut tx, &book_settings(config)).await.unwrap();
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn every_write_changes_the_configuration_and_its_projection(pool: PgPool) {
        initialize_base(&pool).await;
        for (config, through_settings) in [
            (changed(), false),
            (base(), true),
            (changed(), true),
            (base(), false),
        ] {
            let revision = read(&pool).await.unwrap().revision;
            if through_settings {
                save_all(&pool, &config).await;
            } else {
                let mut tx = pool.begin().await.unwrap();
                let active = state(&mut tx).await.unwrap().active_profile.id;
                write_state(&mut tx, &config, active).await.unwrap();
                tx.commit().await.unwrap();
            }
            assert_projected(&pool, &config).await;
            assert!(read(&pool).await.unwrap().revision > revision);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn settings_saved_before_initialization_are_imported(pool: PgPool) {
        let state = rows(&pool).await;
        save_all(&pool, &changed()).await;
        assert_projected(&pool, &changed()).await;
        let imported = initialize(
            &state,
            InitializeConfiguration {
                expected_settings_version: app(&pool).await.version,
                expected_book_version: book(&pool).await.version,
                configuration: base(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            value(&imported.current_configuration),
            value(&changed()),
            "the tables are the source until the configuration exists"
        );
        assert!(!imported.dirty);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unchanged_legacy_book_repairs_projections_without_revising_the_profile(pool: PgPool) {
        initialize_base(&pool).await;
        // Older configurations acquire the default for top_two when read.
        sqlx::query("UPDATE configuration_state SET current_configuration = current_configuration #- '{play,top_two}'")
            .execute(&pool).await.unwrap();
        sqlx::query("UPDATE app_settings SET player_name='Drifted'")
            .execute(&pool)
            .await
            .unwrap();
        let before = read(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        save_book(&mut tx, &book_settings(&before.current_configuration))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_projected(&pool, &before.current_configuration).await;
        assert_eq!(read(&pool).await.unwrap().revision, before.revision);
    }

    /// A changed table value cannot outlive the next write.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_write_repairs_a_projection_that_drifted(pool: PgPool) {
        initialize_base(&pool).await;
        sqlx::query("UPDATE app_settings SET player_name='Drifted',play_plies=3")
            .execute(&pool)
            .await
            .unwrap();
        let mut config = base();
        config.opening_book.path = "/another/book.sqlite".into();
        let mut tx = pool.begin().await.unwrap();
        save_book(&mut tx, &book_settings(&config)).await.unwrap();
        tx.commit().await.unwrap();
        assert_projected(&pool, &config).await;
    }
}
