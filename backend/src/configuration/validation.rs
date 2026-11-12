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

pub(crate) fn normalize(mut config: ProfileConfiguration) -> Result<ProfileConfiguration> {
    upgrade_columns(&mut config);
    if config.schema_version != 1 {
        return Err(ConfigurationError::invalid(
            "schema_version",
            "Expected schema version 1",
        ));
    }
    text(
        "general.player_name",
        &mut config.general.player_name,
        120,
        false,
    )?;
    if !engine_depths(&enum_string(&config.analysis.engine), "analysis")
        .contains(&config.analysis.plies)
    {
        return Err(ConfigurationError::invalid(
            "analysis.plies",
            "Analysis depth is not verified for the selected engine",
        ));
    }
    if !engine_depths(&enum_string(&config.play.engine), "play").contains(&config.play.plies) {
        return Err(ConfigurationError::invalid(
            "play.plies",
            "Play depth is not verified for the selected engine",
        ));
    }
    if !(0..=1).contains(&config.play.human_player) || !(0..=25).contains(&config.play.match_length)
    {
        return Err(ConfigurationError::invalid(
            "play.match_length",
            "Human player must be 0 or 1 and match length 0 (money game) or 1..25",
        ));
    }
    if !config.play.tutor_threshold.is_finite()
        || config.play.tutor_threshold <= 0.0
        || config.play.tutor_threshold > 1.0
    {
        return Err(ConfigurationError::invalid(
            "play.tutor_threshold",
            "Threshold must be greater than zero and at most one",
        ));
    }
    let themes: Value =
        serde_json::from_str(include_str!("../../../shared/configuration-options.json"))
            .expect("valid embedded configuration options");
    if !themes["board_themes"].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|v| v.as_str() == Some(&config.appearance.board_theme))
    }) {
        return Err(ConfigurationError::invalid(
            "appearance.board_theme",
            "Unknown board theme",
        ));
    }
    for (name, value) in [
        ("opening_book.path", &config.opening_book.path),
        ("opening_book.xg_path", &config.opening_book.xg_path),
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
    if !config.library.database_id.is_empty()
        && Uuid::parse_str(&config.library.database_id).is_err()
    {
        return Err(ConfigurationError::invalid(
            "library.database_id",
            "Expected a database UUID",
        ));
    }
    choice(
        "library.sort",
        &config.library.sort,
        &[
            "occurrence_database",
            "occurrence_game",
            "occurrence_move",
            "id",
            "name",
            "white",
            "white_pr",
            "black",
            "black_pr",
            "match_length",
            "result",
            "score",
            "played_at",
            "created_at",
            "tournament",
            "format",
            "position_count",
            "analysis_status",
            "tags",
            "comment",
        ],
    )?;
    let all: std::collections::BTreeSet<_> = LIBRARY_COLUMNS
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();
    let order: std::collections::BTreeSet<_> =
        config.library.column_order.iter().cloned().collect();
    if order != all || config.library.column_order.len() != all.len() {
        return Err(ConfigurationError::invalid(
            "library.column_order",
            "Column order must contain every data column exactly once",
        ));
    }
    let shown: std::collections::BTreeSet<_> =
        config.library.visible_columns.iter().cloned().collect();
    if shown.is_empty()
        || !shown.is_subset(&all)
        || shown.len() != config.library.visible_columns.len()
    {
        return Err(ConfigurationError::invalid(
            "library.visible_columns",
            "At least one known data column is required, without duplicates",
        ));
    }
    config.library.visible_columns = config
        .library
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
        &config.library.column_widths,
        &library_widths,
    )?;
    widths(
        "positions.column_widths",
        &config.positions.column_widths,
        POSITION_COLUMNS,
    )?;
    let f = &mut config.library.filters;
    f.tag = f
        .tag
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    for (key, value, max) in [
        ("q", &mut f.q, 240),
        ("player", &mut f.player, 240),
        ("date_from", &mut f.date_from, 10),
        ("date_to", &mut f.date_to, 10),
        ("date_field", &mut f.date_field, 16),
        ("tag", &mut f.tag, 64),
        ("comment", &mut f.comment, 240),
        ("position_id", &mut f.position_id, 14),
        ("tournament", &mut f.tournament, 240),
        ("round", &mut f.round, 240),
        ("match_name", &mut f.match_name, 240),
        ("match_type", &mut f.match_type, 32),
        ("result", &mut f.result, 32),
        ("match_points_min", &mut f.match_points_min, 8),
        ("match_points_max", &mut f.match_points_max, 8),
    ] {
        text(&format!("library.filters.{key}"), value, max, true)?;
    }
    choice(
        "library.filters.date_field",
        &f.date_field,
        &["played", "imported"],
    )?;
    choice(
        "library.filters.match_type",
        &f.match_type,
        &["", "match", "money"],
    )?;
    choice(
        "library.filters.result",
        &f.result,
        &[
            "",
            "completed",
            "ongoing",
            "unknown",
            "white_won",
            "black_won",
            "draw",
        ],
    )?;
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
    let bounds: std::result::Result<Vec<_>, _> = [&f.match_points_min, &f.match_points_max]
        .iter()
        .map(|value| {
            if value.is_empty() {
                Ok(None)
            } else {
                value.parse::<i32>().map(Some)
            }
        })
        .collect();
    let bounds = bounds.map_err(|_| {
        ConfigurationError::invalid(
            "library.filters.match_points_min",
            "Expected integer bounds",
        )
    })?;
    if bounds.iter().flatten().any(|n| !(0..=32767).contains(n))
        || bounds[0].zip(bounds[1]).is_some_and(|(a, b)| a > b)
    {
        return Err(ConfigurationError::invalid(
            "library.filters.match_points_min",
            "Invalid match points range",
        ));
    }
    config.positions.definition = config
        .positions
        .definition
        .validate()
        .map_err(|e| ConfigurationError::invalid("positions.definition", e.1))?;
    let f = &mut config.learning.filters;
    for (key, value, max) in [
        ("q", &mut f.q, 200),
        ("player", &mut f.player, 240),
        ("tag", &mut f.tag, 64),
        ("comment", &mut f.comment, 200),
        ("position_id", &mut f.position_id, 14),
    ] {
        text(&format!("learning.filters.{key}"), value, max, true)?;
    }
    choice(
        "learning.filters.kind",
        &f.kind,
        &["", "checker", "cube_offer", "cube_response"],
    )?;
    f.tag = f.tag.to_lowercase();
    choice(
        "learning.filters.status",
        &f.status,
        &["", "new", "due", "reviewed"],
    )?;
    if config.learning.deck_id != "__all__" && Uuid::parse_str(&config.learning.deck_id).is_err() {
        return Err(ConfigurationError::invalid(
            "learning.deck_id",
            "Expected a deck UUID or all cards",
        ));
    }
    if !config.learning.generation.min_loss.is_finite() || config.learning.generation.min_loss < 0.0
    {
        return Err(ConfigurationError::invalid(
            "learning.generation.min_loss",
            "Loss must be finite and nonnegative",
        ));
    }
    text(
        "learning.generation.player",
        &mut config.learning.generation.player,
        240,
        true,
    )?;
    choice(
        "learning.generation.player",
        &config.learning.generation.player,
        &["", "0", "1"],
    )?;
    if let Some(f) = &mut config.annotations.filters {
        f.tag = f
            .tag
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        text("annotations.filters.q", &mut f.q, 200, true)?;
        text("annotations.filters.tag", &mut f.tag, 64, true)?;
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
    choice("review.player", &config.review.player, &["", "0", "1"])?;
    choice(
        "review.kind",
        &config.review.kind,
        &["", "checker", "cube", "cube_offer", "cube_response"],
    )?;
    choice(
        "review.severity",
        &config.review.severity,
        &["", "none", "doubtful", "bad", "very_bad"],
    )?;
    for (path, value) in [
        (
            "library.filters.position_id",
            &config.library.filters.position_id,
        ),
        (
            "learning.filters.position_id",
            &config.learning.filters.position_id,
        ),
    ] {
        if !value.is_empty()
            && (value.len() != 14
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"+/".contains(&c)))
        {
            return Err(ConfigurationError::invalid(
                path,
                "Expected 14-character GNUbg position ID",
            ));
        }
    }
    Ok(config)
}

pub(crate) async fn resources(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    config: &ProfileConfiguration,
    engine: bool,
    book: bool,
    previous: Option<&ProfileConfiguration>,
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
            });
        }
        for (path, selected, plies, default_threads) in [
            (
                "analysis.engine",
                &config.analysis.engine,
                config.analysis.plies,
                crate::settings::SAGE_ANALYSIS_THREADS,
            ),
            (
                "play.engine",
                &config.play.engine,
                config.play.plies,
                crate::settings::SAGE_PLAY_THREADS,
            ),
        ] {
            let threads =
                crate::settings::sage_threads(settings.open_sage_threads, default_threads);
            if *selected == ConfigurationEngine::OpenSage
                && crate::settings::open_sage::verified_profile(&state.pool, plies, threads)
                    .await
                    .is_err()
            {
                issues.push(ConfigurationIssue { path: path.into(), code: "engine_unavailable".into(),
                    message: "The configured Open Sage installation or requested depth is unavailable or changed".into() });
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
            crate::xg_book::status(&cfg.xg_path)
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
            });
        }
    }
    let mut references = vec![(
        "library.database_id",
        config.library.database_id.clone(),
        "databases",
    )];
    if let Some(id) = config.positions.definition.database_id {
        references.push((
            "positions.definition.database_id",
            id.to_string(),
            "databases",
        ));
    }
    if let Some(id) = config
        .annotations
        .filters
        .as_ref()
        .and_then(|f| f.database_id.clone())
    {
        references.push(("annotations.filters.database_id", id, "databases"));
    }
    if config.learning.deck_id != "__all__" {
        references.push(("learning.deck_id", config.learning.deck_id.clone(), "decks"));
    }
    for (path, id, table) in references {
        if id.is_empty() {
            continue;
        }
        // Existing unavailable selections stay readable/editable. Ordinary
        // edits validate only references that the user actually changed;
        // activation and availability checks pass no previous configuration.
        if previous.is_some_and(|old| match path {
            "library.database_id" => old.library.database_id == id,
            "learning.deck_id" => old.learning.deck_id == id,
            "positions.definition.database_id" => {
                old.positions
                    .definition
                    .database_id
                    .map(|value| value.to_string())
                    .as_deref()
                    == Some(id.as_str())
            }
            "annotations.filters.database_id" => {
                old.annotations
                    .filters
                    .as_ref()
                    .and_then(|filter| filter.database_id.as_deref())
                    == Some(id.as_str())
            }
            _ => false,
        }) {
            continue;
        }
        let id =
            Uuid::parse_str(&id).map_err(|_| ConfigurationError::invalid(path, "Expected UUID"))?;
        let exists: bool =
            sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=$1)"))
                .bind(id)
                .fetch_one(&mut **tx)
                .await?;
        if !exists {
            issues.push(ConfigurationIssue {
                path: path.into(),
                code: "reference_missing".into(),
                message: format!(
                    "Referenced {} no longer exists",
                    if table == "decks" { "deck" } else { "database" }
                ),
            });
        }
    }
    let _ = state;
    Ok(issues)
}
pub(crate) fn enum_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializable enum")
        .as_str()
        .expect("string enum")
        .to_owned()
}
