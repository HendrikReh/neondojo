//! The single integration test target. Every file in this directory is one
//! module here, so all integration tests link into one binary. Cargo does
//! not discover test targets on its own (`autotests = false`); add a module
//! line for each new test file. `every_test_file_is_a_module` checks that.

mod profile_support;
mod support;

mod analysis_catalog;
mod analysis_remaining_time;
mod analysis_statistics;
mod analysis_time_estimate;
mod annotations;
mod benchmarks;
mod card_search;
mod checker_turns;
mod configuration;
mod database_backup;
mod databases;
mod deck_management;
mod duplicate_matches;
mod game_move_annotations;
mod game_plan_rules;
mod game_plans;
mod game_results;
mod game_selection;
mod import_api;
mod import_corpus;
mod imported_analysis;
mod learning;
mod library_occurrences;
mod library_sorting;
mod mat_corpus;
mod match_analysis;
mod match_metadata;
mod move_assessments;
mod ogxm_import;
mod open_sage;
mod open_sage_play;
mod open_sage_settings;
mod play;
mod play_history;
mod player_directory;
mod position_bookmarks;
mod position_browser;
mod position_decks;
mod position_learning;
mod profiles;
mod recurring_mistakes;
mod recurring_mistakes_imported;
mod recurring_mistakes_native;
mod review_decisions;
mod saved_position_retention;
mod schema_migrations;
mod search_performance;
mod settings;
mod study_reports;
mod virtual_players;
mod workflow;

#[test]
fn every_test_file_is_a_module() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let root = include_str!("integration.rs");
    let mut missing = vec![];
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let is_module = path.extension().is_some_and(|extension| extension == "rs")
            || path.join("mod.rs").is_file();
        if is_module
            && stem != "integration"
            && !root.lines().any(|line| line == format!("mod {stem};"))
        {
            missing.push(stem.to_owned());
        }
    }
    assert!(
        missing.is_empty(),
        "add these modules to tests/integration.rs: {missing:?}"
    );
}
