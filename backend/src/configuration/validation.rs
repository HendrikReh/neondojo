use super::*;
use crate::api::AppState;
use sqlx::{Postgres, Transaction};

pub(crate) const LIBRARY_COLUMNS: &[(&str, i32)] = &[
    ("id", 44),
    ("white", 60),
    ("white_pr", 44),
    ("black", 60),
    ("black_pr", 44),
    ("play_mode", 90),
    ("match_length", 44),
    ("score", 54),
    ("played_at", 70),
    ("created_at", 70),
    ("tournament", 70),
    ("position_count", 70),
    ("analysis_status", 70),
    ("tags", 96),
    ("comment", 96),
];
const POSITION_COLUMNS: &[(&str, i32)] = &[
    ("select", 32),
    ("origin", 112),
    ("database", 70),
    ("game", 44),
    ("ply", 44),
    ("player", 70),
    ("opponent", 70),
    ("dice", 44),
    ("score", 60),
    ("cube", 44),
    ("played_move", 80),
    ("loss", 60),
    ("decks", 80),
    ("notes", 120),
    ("actions", 60),
];

fn text(path: &str, value: &mut String, max: usize, trim: bool) -> Result<()> {
    if trim {
        *value = value.trim().to_owned();
    }
    if value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(ConfigurationError::invalid(
            path,
            format!("Must contain at most {max} characters and no control characters"),
        ));
    }
    Ok(())
}
fn choice(path: &str, value: &str, values: &[&str]) -> Result<()> {
    if values.contains(&value) {
        Ok(())
    } else {
        Err(ConfigurationError::invalid(path, "Unsupported value"))
    }
}
fn widths(
    path: &str,
    value: &Option<BTreeMap<String, i32>>,
    columns: &[(&str, i32)],
) -> Result<()> {
    if let Some(widths) = value {
        for (id, width) in widths {
            let min = columns
                .iter()
                .find(|(key, _)| key == id)
                .map(|(_, min)| *min)
                .ok_or_else(|| ConfigurationError::invalid(path, "Unknown column"))?;
            if !(min..=2400).contains(width) {
                return Err(ConfigurationError::invalid(
                    path,
                    "Column width is out of range",
                ));
            }
        }
    }
    Ok(())
}
// Profiles remain schema version 1. Fill the new column and widen existing
// action columns without changing the saved visibility or order of old columns.
pub(super) fn upgrade_columns(config: &mut ProfileConfiguration) {
    let order = &mut config.library.column_order;
    if order.len() + 1 == LIBRARY_COLUMNS.len()
        && !order.iter().any(|id| id == "play_mode")
        && let Some(index) = order.iter().position(|id| id == "match_length")
    {
        order.insert(index, "play_mode".into());
    }
    for (widths, old, new) in [
        (&mut config.library.column_widths, 88, 160),
        (&mut config.positions.column_widths, 40, 60),
    ] {
        if let Some(width) = widths.as_mut().and_then(|w| w.get_mut("actions"))
            && (old..new).contains(width)
        {
            *width = new;
        }
    }
}

fn position_id(path: &str, value: &str) -> Result<()> {
    if !value.is_empty() && !crate::engine::valid_id(value, 14) {
        return Err(ConfigurationError::invalid(
            path,
            "Expected 14-character GNUbg position ID",
        ));
    }
    Ok(())
}

fn normalize_tag(path: &str, value: &mut String) -> Result<()> {
    *value = crate::annotations::normalize_tag(value)
        .map_err(|error| ConfigurationError::invalid(path, error.1.error))?;
    Ok(())
}

fn normalize_general(general: &ConfigurationGeneral) -> Result<()> {
    if !crate::settings::valid_player_name(&general.player_name) {
        return Err(ConfigurationError::invalid(
            "general.player_name",
            crate::settings::PLAYER_NAME_RULE,
        ));
    }
    Ok(())
}

fn normalize_analysis(analysis: &ConfigurationAnalysis) -> Result<()> {
    if !analysis.error_thresholds.valid() {
        return Err(ConfigurationError::invalid(
            "analysis.error_thresholds",
            "Error thresholds must increase strictly, be greater than zero, and be at most one",
        ));
    }
    let engine = crate::engines::EngineId::from(&analysis.engine);
    if !crate::settings::valid_engine_depth(engine.id(), "analysis", analysis.plies) {
        return Err(ConfigurationError::invalid(
            "analysis.plies",
            "Analysis depth is not verified for the selected engine",
        ));
    }
    Ok(())
}

fn normalize_play(play: &ConfigurationPlay) -> Result<()> {
    let engine = crate::engines::EngineId::from(&play.engine);
    if !crate::settings::valid_engine_depth(engine.id(), "play", play.plies) {
        return Err(ConfigurationError::invalid(
            "play.plies",
            "Play depth is not verified for the selected engine",
        ));
    }
    if play.top_two && !crate::settings::supports_top_two(engine.id(), play.plies) {
        return Err(ConfigurationError::invalid(
            "play.top_two",
            crate::settings::TOP_TWO_PROFILE_ERROR,
        ));
    }
    if !crate::settings::valid_human_player(play.human_player)
        || !crate::settings::valid_play_match_length(play.match_length)
    {
        return Err(ConfigurationError::invalid(
            "play.match_length",
            crate::settings::PLAY_MATCH_LENGTH_RULE,
        ));
    }
    if !crate::settings::valid_tutor_threshold(play.tutor_threshold) {
        return Err(ConfigurationError::invalid(
            "play.tutor_threshold",
            crate::settings::TUTOR_THRESHOLD_RULE,
        ));
    }
    Ok(())
}

fn normalize_appearance(appearance: &ConfigurationAppearance) -> Result<()> {
    let options = &*super::OPTIONS;
    let board_themes = options["board_themes"]
        .as_array()
        .expect("embedded board themes");
    for (path, theme) in [
        (
            "appearance.board_theme",
            Some(appearance.board_theme.as_str()),
        ),
        (
            "appearance.checker_theme",
            appearance.checker_theme.as_deref(),
        ),
        ("appearance.dice_theme", appearance.dice_theme.as_deref()),
        ("appearance.cube_theme", appearance.cube_theme.as_deref()),
    ] {
        if let Some(theme) = theme
            && !(path == "appearance.dice_theme"
                && Some(theme) == options["default_dice_theme"].as_str())
            && !board_themes.iter().any(|v| v.as_str() == Some(theme))
        {
            return Err(ConfigurationError::invalid(path, "Unknown board theme"));
        }
    }
    let bounds = &options["analysis_board_width"];
    let min = bounds["min"]
        .as_i64()
        .expect("embedded minimum board width");
    let max = bounds["max"]
        .as_i64()
        .expect("embedded maximum board width");
    for (path, width) in [
        (
            "appearance.analysis_board_width",
            appearance.analysis_board_width,
        ),
        (
            "appearance.analysis_overview_board_width",
            appearance.analysis_overview_board_width,
        ),
        (
            "appearance.analysis_errors_board_width",
            appearance.analysis_errors_board_width,
        ),
        (
            "appearance.analysis_statistics_board_width",
            appearance.analysis_statistics_board_width,
        ),
        (
            "appearance.analysis_notes_board_width",
            appearance.analysis_notes_board_width,
        ),
    ] {
        if width.is_some_and(|width| !(min..=max).contains(&i64::from(width))) {
            return Err(ConfigurationError::invalid(
                path,
                "Board width is out of range",
            ));
        }
    }
    Ok(())
}

fn normalize_opening_book(book: &ConfigurationBook) -> Result<()> {
    for (name, value) in [
        ("opening_book.path", &book.path),
        ("opening_book.xg_path", &book.xg_path),
    ] {
        if value.is_empty()
            || value.len() > 4096
            || value.chars().any(char::is_control)
            || !std::path::Path::new(value).is_absolute()
        {
            return Err(ConfigurationError::invalid(
                name,
                "Book reference must be an absolute path of at most 4096 bytes",
            ));
        }
    }
    Ok(())
}

fn normalize_library(library: &mut ConfigurationLibrary) -> Result<()> {
    if !library.database_id.is_empty() && Uuid::parse_str(&library.database_id).is_err() {
        return Err(ConfigurationError::invalid(
            "library.database_id",
            "Expected a database UUID",
        ));
    }
    if crate::databases::sort_spec(&library.sort).is_none() {
        return Err(ConfigurationError::invalid(
            "library.sort",
            "Unsupported value",
        ));
    }
    let all: std::collections::BTreeSet<_> = LIBRARY_COLUMNS
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();
    let order: std::collections::BTreeSet<_> = library.column_order.iter().cloned().collect();
    if order != all || library.column_order.len() != all.len() {
        return Err(ConfigurationError::invalid(
            "library.column_order",
            "Column order must contain every data column exactly once",
        ));
    }
    let shown: std::collections::BTreeSet<_> = library.visible_columns.iter().cloned().collect();
    if shown.is_empty() || !shown.is_subset(&all) || shown.len() != library.visible_columns.len() {
        return Err(ConfigurationError::invalid(
            "library.visible_columns",
            "At least one known data column is required, without duplicates",
        ));
    }
    library.visible_columns = library
        .column_order
        .iter()
        .filter(|id| shown.contains(*id))
        .cloned()
        .collect();
    let mut library_widths = LIBRARY_COLUMNS.to_vec();
    library_widths.extend([
        ("select", 32),
        ("actions", 160),
        ("occurrence_database", 70),
        ("occurrence_game", 44),
        ("occurrence_move", 44),
    ]);
    widths(
        "library.column_widths",
        &library.column_widths,
        &library_widths,
    )?;
    normalize_library_filters(&mut library.filters)
}

fn normalize_library_filters(f: &mut ConfigurationLibraryFilters) -> Result<()> {
    normalize_tag("library.filters.tag", &mut f.tag)?;
    for (key, value, max) in [
        ("q", &mut f.q, 240),
        ("player", &mut f.player, 240),
        ("date_from", &mut f.date_from, 10),
        ("date_to", &mut f.date_to, 10),
        ("date_field", &mut f.date_field, 16),
        ("comment", &mut f.comment, 240),
        ("position_id", &mut f.position_id, 14),
        ("tournament", &mut f.tournament, 240),
        ("round", &mut f.round, 240),
        ("match_name", &mut f.match_name, 240),
        ("match_type", &mut f.match_type, 32),
        ("result", &mut f.result, 32),
        ("match_points_min", &mut f.match_points_min, 10),
        ("match_points_max", &mut f.match_points_max, 10),
    ] {
        text(&format!("library.filters.{key}"), value, max, true)?;
    }
    choice(
        "library.filters.date_field",
        &f.date_field,
        crate::databases::DATE_FIELDS,
    )?;
    if !f.match_type.is_empty() && !crate::databases::MATCH_TYPES.contains(&f.match_type.as_str()) {
        return Err(ConfigurationError::invalid(
            "library.filters.match_type",
            "Unsupported value",
        ));
    }
    if !f.result.is_empty() && !crate::databases::RESULT_STATES.contains(&f.result.as_str()) {
        return Err(ConfigurationError::invalid(
            "library.filters.result",
            "Unsupported value",
        ));
    }
    for (key, value) in [("date_from", &f.date_from), ("date_to", &f.date_to)] {
        if !value.is_empty() && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
            return Err(ConfigurationError::invalid(
                &format!("library.filters.{key}"),
                "Expected YYYY-MM-DD",
            ));
        }
    }
    if !f.date_from.is_empty() && !f.date_to.is_empty() && f.date_from > f.date_to {
        return Err(ConfigurationError::invalid(
            "library.filters.date_to",
            "Date range is reversed",
        ));
    }
    let parse_points = |value: &str| -> Result<Option<i32>> {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<i32>().map(Some).map_err(|_| {
                ConfigurationError::invalid(
                    "library.filters.match_points_min",
                    "Expected integer bounds",
                )
            })
        }
    };
    let min_points = parse_points(&f.match_points_min)?;
    let max_points = parse_points(&f.match_points_max)?;
    if !crate::databases::valid_match_points_range(min_points, max_points) {
        return Err(ConfigurationError::invalid(
            "library.filters.match_points_min",
            "Invalid match points range",
        ));
    }
    position_id("library.filters.position_id", &f.position_id)
}

fn normalize_positions(positions: &mut ConfigurationPositions) -> Result<()> {
    widths(
        "positions.column_widths",
        &positions.column_widths,
        POSITION_COLUMNS,
    )?;
    let position_columns = POSITION_COLUMNS
        .iter()
        .map(|&(id, _)| id)
        .filter(|id| !matches!(*id, "select" | "actions" | "played_move"));
    let shown: std::collections::BTreeSet<_> = positions
        .visible_columns
        .iter()
        .map(String::as_str)
        .collect();
    let canonical: Vec<_> = position_columns
        .filter(|id| shown.contains(id))
        .map(str::to_owned)
        .collect();
    if canonical.is_empty() || canonical.len() != positions.visible_columns.len() {
        return Err(ConfigurationError::invalid(
            "positions.visible_columns",
            "At least one known data column is required, without duplicates",
        ));
    }
    positions.visible_columns = canonical;
    positions.definition = positions
        .definition
        .clone()
        .validate()
        .map_err(|e| ConfigurationError::invalid("positions.definition", e.1.error))?;
    Ok(())
}

fn normalize_learning(learning: &mut ConfigurationLearning) -> Result<()> {
    let f = &mut learning.filters;
    if !f.game_plan.is_empty() && crate::game_plan::GamePlan::parse(&f.game_plan).is_none() {
        return Err(ConfigurationError::invalid(
            "learning.filters.game_plan",
            "Unsupported value",
        ));
    }
    normalize_tag("learning.filters.tag", &mut f.tag)?;
    for (key, value, max) in [
        ("q", &mut f.q, 200),
        ("player", &mut f.player, 240),
        ("topics", &mut f.topics, 650),
        ("comment", &mut f.comment, 200),
        ("position_id", &mut f.position_id, 14),
    ] {
        text(&format!("learning.filters.{key}"), value, max, true)?;
    }
    if !f.kind.is_empty() && crate::learning::CardKind::parse(&f.kind).is_none() {
        return Err(ConfigurationError::invalid(
            "learning.filters.kind",
            "Unsupported value",
        ));
    }
    if !f.status.is_empty() && crate::learning::CardReviewState::parse(&f.status).is_none() {
        return Err(ConfigurationError::invalid(
            "learning.filters.status",
            "Unsupported value",
        ));
    }
    position_id("learning.filters.position_id", &f.position_id)?;
    if !learning.is_collection_scope() && Uuid::parse_str(&learning.deck_id).is_err() {
        return Err(ConfigurationError::invalid(
            "learning.deck_id",
            "Expected a deck UUID or a collection scope",
        ));
    }
    if !learning.generation.min_loss.is_finite() || learning.generation.min_loss < 0.0 {
        return Err(ConfigurationError::invalid(
            "learning.generation.min_loss",
            "Loss must be finite and nonnegative",
        ));
    }
    text(
        "learning.generation.player",
        &mut learning.generation.player,
        240,
        true,
    )?;
    choice(
        "learning.generation.player",
        &learning.generation.player,
        &["", "0", "1"],
    )
}

fn normalize_annotations(annotations: &mut ConfigurationAnnotations) -> Result<()> {
    if let Some(f) = &mut annotations.filters {
        normalize_tag("annotations.filters.tag", &mut f.tag)?;
        text("annotations.filters.q", &mut f.q, 200, true)?;
        choice(
            "annotations.filters.kind",
            &f.kind,
            &["", "match", "position", "game", "move"],
        )?;
        if f.q.is_empty() && f.tag.is_empty() || !f.q.is_empty() && f.q.chars().count() < 3 {
            return Err(ConfigurationError::invalid(
                "annotations.filters.q",
                "Search needs a tag or at least three characters",
            ));
        }
        if f.database_id
            .as_ref()
            .is_some_and(|id| !id.is_empty() && Uuid::parse_str(id).is_err())
        {
            return Err(ConfigurationError::invalid(
                "annotations.filters.database_id",
                "Expected database UUID",
            ));
        }
    }
    Ok(())
}

fn normalize_review(review: &ConfigurationReview) -> Result<()> {
    choice("review.player", &review.player, &["", "0", "1"])?;
    choice(
        "review.kind",
        &review.kind,
        &["", "checker", "cube", "cube_offer", "cube_response"],
    )?;
    choice(
        "review.severity",
        &review.severity,
        &["", "none", "doubtful", "bad", "blunder", "very_bad"],
    )
}

pub(crate) fn normalize(mut config: ProfileConfiguration) -> Result<ProfileConfiguration> {
    upgrade_columns(&mut config);
    if config.schema_version != 1 {
        return Err(ConfigurationError::invalid(
            "schema_version",
            "Expected schema version 1",
        ));
    }
    normalize_general(&config.general)?;
    normalize_analysis(&config.analysis)?;
    normalize_play(&config.play)?;
    normalize_appearance(&config.appearance)?;
    normalize_opening_book(&config.opening_book)?;
    normalize_library(&mut config.library)?;
    normalize_positions(&mut config.positions)?;
    normalize_learning(&mut config.learning)?;
    normalize_annotations(&mut config.annotations)?;
    normalize_review(&config.review)?;
    Ok(config)
}

/// Which resources that a changed configuration newly relies on are unavailable.
/// Changing other settings checks none, so they can be saved while an
/// installation is missing.
pub(crate) async fn changed_resources(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    old: &ProfileConfiguration,
    new: &ProfileConfiguration,
) -> Result<Vec<ConfigurationIssue>> {
    let engine = new.analysis.engine != old.analysis.engine
        || new.analysis.plies != old.analysis.plies
        || new.play.engine != old.play.engine
        || new.play.plies != old.play.plies
        || new.play.top_two != old.play.top_two;
    let book = super::store::value(new)["opening_book"] != super::store::value(old)["opening_book"];
    resources(state, tx, new, engine, book).await
}

pub(crate) async fn resources(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
    engine: bool,
    book: bool,
) -> Result<Vec<ConfigurationIssue>> {
    let mut issues = vec![];
    if engine {
        let settings: crate::settings::AppSettings =
            sqlx::query_as("SELECT * FROM app_settings WHERE singleton=true")
                .fetch_one(&mut **tx)
                .await?;
        if settings.gnubg().check_installation().await.is_err() {
            issues.push(ConfigurationIssue {
                path: "analysis.engine".into(),
                code: "engine_unavailable".into(),
                message: "The configured GNUbg installation is unavailable".into(),
                engine: Some(ConfigurationEngine::Gnubg),
            });
        }
        for (path, selected, plies, threads) in [
            (
                "analysis.engine",
                &config.analysis.engine,
                config.analysis.plies,
                settings.sage_analysis_threads(),
            ),
            (
                "play.engine",
                &config.play.engine,
                config.play.plies,
                settings.sage_play_threads(),
            ),
        ] {
            if crate::engines::EngineId::from(selected).requires_verified_profile()
                && crate::settings::open_sage::verified_profile(&state.pool, plies, threads)
                    .await
                    .is_err()
            {
                issues.push(ConfigurationIssue {
                    path: path.into(),
                    code: "engine_unavailable".into(),
                    message: "The configured NeonGammon installation or requested depth is unavailable or changed".into(),
                    engine: Some(selected.clone()),
                });
            }
        }
    }
    if book && config.opening_book.provider != ConfigurationBookProvider::Off {
        let cfg = crate::opening_book::BookConfig {
            version: 0,
            provider: enum_string(&config.opening_book.provider),
            path: config.opening_book.path.clone(),
            xg_path: config.opening_book.xg_path.clone(),
        };
        let (bg, xg) = tokio::join!(
            crate::opening_book::status(&cfg),
            state.opening_books.xg.status(&cfg.xg_path)
        );
        let available = match cfg.provider.as_str() {
            "backgammondb" => bg.available,
            "xg" => xg.available,
            _ => bg.available || xg.available,
        };
        if !available {
            issues.push(ConfigurationIssue {
                path: "opening_book.provider".into(),
                code: "book_unavailable".into(),
                message: "The selected opening book is unavailable".into(),
                engine: None,
            });
        }
    }
    Ok(issues)
}
pub(crate) fn enum_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializable enum")
        .as_str()
        .expect("string enum")
        .to_owned()
}
