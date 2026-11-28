use super::state::*;
use super::*;
use crate::configure_app;
use chrono::{NaiveDate, Utc};
use gpui_kit::{WindowOptions, test::TestWindowExt};
use neondojo::databases::{ActionItem, ActionResponse, Database, DatabaseMatch, LibraryOccurrence};
use neondojo::duplicate_matches::{DuplicateMatchGroup, DuplicateMatchKind};
use neondojo::model::{ImportWarning, ImportWarningKind, Job};
use neondojo::play::{BOT_DATABASE_ID, PAUSED_MATCHES_DATABASE_ID, PlayMode};
use neondojo_i18n::Locale;
use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use uuid::Uuid;

fn sample_database(
    id: Uuid,
    name: &str,
    is_system: bool,
    is_default: bool,
    count: i64,
) -> Database {
    let now = Utc::now();
    Database {
        id,
        name: name.to_string(),
        description: format!("{name} description"),
        version: 1,
        is_default,
        is_system,
        created_at: now,
        updated_at: now,
        match_count: count,
    }
}

fn sample_match(id: Uuid, match_no: i64, db_id: Uuid) -> DatabaseMatch {
    DatabaseMatch {
        occurrence: None,
        player_sides: None,
        opponent_player_id: None,
        id,
        source_available: true,
        play_session_id: None,
        play_paused: false,
        study_report_id: None,
        play_phase: None,
        play_mode: None,
        play_assisted: false,
        match_no,
        filename: format!("match-{match_no}.sgf"),
        format: "sgf".to_string(),
        white: format!("Alice {match_no}"),
        black: format!("Bob {match_no}"),
        match_length: 7,
        position_count: 142,
        created_at: Utc::now(),
        database_id: db_id,
        display_name: format!("Open {match_no}"),
        played_on: NaiveDate::from_ymd_opt(2026, 4, 15),
        tournament: Some("Berlin Open".to_string()),
        round: Some("Final".to_string()),
        place: Some("Berlin".to_string()),
        result_state: "white_won".to_string(),
        white_score: Some(7),
        black_score: Some(5),
        white_pr: Some(4.12),
        black_pr: Some(6.85),
        pr_method: Some("gnubg".to_string()),
        pr_profile: Some("3-ply".to_string()),
        analysis_status: Some("completed".to_string()),
        game_count: 8,
        import_warning_count: 0,
        tags: vec!["holding game".to_string()],
        comment: "Strong cube decision in game 3.".to_string(),
        version: 1,
    }
}

fn sample_job(id: Uuid, match_id: Uuid, status: &str, progress: i32) -> Job {
    let now = Utc::now();
    Job {
        engine_snapshot: None,
        id,
        position_uuid: None,
        match_uuid: Some(match_id),
        game_no: None,
        total_games: 5,
        completed_games: if status == "completed" { 5 } else { 1 },
        kind: "match".to_string(),
        plies: 3,
        status: status.to_string(),
        progress,
        result: None,
        error: None,
        attempts: 1,
        created_at: now,
        updated_at: now,
    }
}

// ── Pure Unit Tests ──────────────────────────────────────────────────────────

#[test]
fn validates_14_char_gnubg_position_ids_strictly() {
    assert!(is_valid_position_id("4HPwATDgc/ABMA"));
    assert!(is_valid_position_id("  4HPwATDgc/ABMA  "));
    assert!(is_valid_position_id("A+B/0123456789"));

    // Invalid lengths & characters
    assert!(!is_valid_position_id(""));
    assert!(!is_valid_position_id("4HPwATDgc/ABM")); // 13 chars
    assert!(!is_valid_position_id("4HPwATDgc/ABMAA")); // 15 chars
    assert!(!is_valid_position_id("4HPwATDgc-ABMA")); // '-' invalid
    assert!(!is_valid_position_id("4HPwATDgc_ABMA")); // '_' invalid
    assert!(!is_valid_position_id("4HPwATDgc=ABMA")); // '=' invalid
    assert!(!is_valid_position_id("4HPwATDgc ABMA")); // space inside
    assert!(!is_valid_position_id("4HPwATDgcäABMA")); // non-ASCII
}

#[test]
fn cross_page_selection_dedupes_enforces_100_and_50_export_caps_and_retains_failures() {
    let db_id = Uuid::from_u128(1);
    let mut sel = LibrarySelectionState::default();
    assert!(sel.is_empty());
    assert!(!sel.can_export());

    // Build 110 unique matches spanning multiple pages.
    let all_matches: Vec<DatabaseMatch> = (1..=110)
        .map(|i| sample_match(Uuid::from_u128(1000 + i), i as i64, db_id))
        .collect();

    // Selecting first 50 allows export.
    sel.set_many(&all_matches[0..50], true);
    assert_eq!(sel.len(), 50);
    assert!(sel.can_export());
    assert!(sel.can_mutate_membership());

    // Selecting 51st disables export (50-match export cap).
    sel.set_many(&all_matches[50..51], true);
    assert_eq!(sel.len(), 51);
    assert!(!sel.can_export());

    // Re-selecting an already-selected ID deduplicates cleanly.
    sel.set_many(&all_matches[0..10], true);
    assert_eq!(sel.len(), 51);

    // Selecting remaining matches enforces hard 100 cap.
    sel.set_many(&all_matches[51..110], true);
    assert_eq!(sel.len(), MAX_SELECTION);
    assert!(sel.is_at_cap());

    // Partial batch failure retention keeps ONLY failed IDs selected.
    let ok_id = all_matches[0].id;
    let dup_id = all_matches[1].id;
    let busy_id = all_matches[2].id;
    let err_id = all_matches[3].id;
    let response = ActionResponse {
        results: vec![
            ActionItem {
                match_id: ok_id,
                status: "moved".to_string(),
                created_id: None,
                existing_id: None,
                error: None,
            },
            ActionItem {
                match_id: dup_id,
                status: "duplicate".to_string(),
                created_id: None,
                existing_id: Some(dup_id),
                error: None,
            },
            ActionItem {
                match_id: busy_id,
                status: "busy".to_string(),
                created_id: None,
                existing_id: None,
                error: Some("Analysis running".to_string()),
            },
            ActionItem {
                match_id: err_id,
                status: "error".to_string(),
                created_id: None,
                existing_id: None,
                error: Some("Database conflict".to_string()),
            },
        ],
    };
    let summary = sel.retain_failed(&response);
    assert_eq!(summary.succeeded, 2);
    assert_eq!(summary.failed, 2);
    assert!(summary.has_failures());
    assert_eq!(sel.len(), 2);
    assert!(sel.contains(busy_id));
    assert!(sel.contains(err_id));
    assert!(!sel.contains(ok_id));
    assert!(!sel.contains(dup_id));
}

#[test]
fn paused_selection_blocks_membership_mutations_and_duplicate_select_copies_skips_oldest() {
    let db_id = Uuid::from_u128(1);
    let mut paused = sample_match(Uuid::new_v4(), 1, PAUSED_MATCHES_DATABASE_ID);
    paused.play_paused = true;
    paused.play_session_id = Some(Uuid::new_v4());

    let mut sel = LibrarySelectionState::default();
    sel.set_many(&[paused], true);
    assert!(sel.has_paused());
    assert!(!sel.can_mutate_membership());
    assert_eq!(sel.partition_analyzable().1, 1);

    // Duplicate finder "Select copies" keeps index 0 (oldest) unselected and selects copies.
    sel.clear();
    let g1_oldest = sample_match(Uuid::new_v4(), 10, db_id);
    let g1_copy1 = sample_match(Uuid::new_v4(), 11, db_id);
    let g1_copy2 = sample_match(Uuid::new_v4(), 12, db_id);
    let g2_oldest = sample_match(Uuid::new_v4(), 20, db_id);
    let g2_copy1 = sample_match(Uuid::new_v4(), 21, db_id);

    let g1_oldest_id = g1_oldest.id;
    let g1_copy1_id = g1_copy1.id;
    let g1_copy2_id = g1_copy2.id;
    let g2_oldest_id = g2_oldest.id;
    let g2_copy1_id = g2_copy1.id;

    let groups = vec![
        DuplicateMatchGroup {
            kind: DuplicateMatchKind::IdenticalFiles,
            match_count: 3,
            items: vec![g1_oldest, g1_copy1, g1_copy2],
        },
        DuplicateMatchGroup {
            kind: DuplicateMatchKind::SameMoves,
            match_count: 2,
            items: vec![g2_oldest, g2_copy1],
        },
    ];
    let added = sel.select_duplicate_copies(&groups);
    assert_eq!(added, 3);
    assert!(!sel.contains(g1_oldest_id));
    assert!(sel.contains(g1_copy1_id));
    assert!(sel.contains(g1_copy2_id));
    assert!(!sel.contains(g2_oldest_id));
    assert!(sel.contains(g2_copy1_id));
}

#[test]
fn mutually_exclusive_search_panels_switch_and_collapse() {
    let mut active: Option<SearchPanelKind> = None;

    toggle_search_panel(&mut active, SearchPanelKind::Filters);
    assert_eq!(active, Some(SearchPanelKind::Filters));

    toggle_search_panel(&mut active, SearchPanelKind::Notes);
    assert_eq!(active, Some(SearchPanelKind::Notes));

    toggle_search_panel(&mut active, SearchPanelKind::Position);
    assert_eq!(active, Some(SearchPanelKind::Position));

    toggle_search_panel(&mut active, SearchPanelKind::Duplicates);
    assert_eq!(active, Some(SearchPanelKind::Duplicates));

    // Clicking active panel collapses it cleanly.
    toggle_search_panel(&mut active, SearchPanelKind::Duplicates);
    assert_eq!(active, None);
}

#[test]
fn row_click_routing_sends_paused_and_unfinished_competition_to_play_never_analysis() {
    let db_id = Uuid::from_u128(1);

    // 1. Paused match -> OpenPlaySession { resume: true }
    let paused_session = Uuid::new_v4();
    let mut paused = sample_match(Uuid::new_v4(), 1, PAUSED_MATCHES_DATABASE_ID);
    paused.play_paused = true;
    paused.play_session_id = Some(paused_session);
    paused.play_mode = Some(PlayMode::Coach);
    paused.play_phase = Some("move".to_string());
    assert!(matches!(
        row_open_event(&paused),
        WorkspaceEvent::OpenPlaySession { session_id, resume: true } if session_id == paused_session
    ));
    assert!(!can_analyze_match(&paused));

    // 2. Unfinished Competition match (even not paused!) -> OpenPlaySession { resume: true }
    let comp_session = Uuid::new_v4();
    let mut comp_active = sample_match(Uuid::new_v4(), 2, BOT_DATABASE_ID);
    comp_active.play_mode = Some(PlayMode::Competition);
    comp_active.play_session_id = Some(comp_session);
    comp_active.play_phase = Some("roll".to_string());
    comp_active.match_length = 7;
    assert!(is_unfinished_competition(&comp_active));
    assert!(!can_analyze_match(&comp_active));
    assert!(!can_show_match_pr(&comp_active));
    assert!(matches!(
        row_open_event(&comp_active),
        WorkspaceEvent::OpenPlaySession { session_id, resume: true } if session_id == comp_session
    ));

    // 3. Finished Competition match -> OpenMatchInAnalysis
    let mut comp_done = sample_match(Uuid::new_v4(), 3, BOT_DATABASE_ID);
    comp_done.play_mode = Some(PlayMode::Competition);
    comp_done.play_session_id = Some(Uuid::new_v4());
    comp_done.play_phase = Some("match_over".to_string());
    assert!(!is_unfinished_competition(&comp_done));
    assert!(can_analyze_match(&comp_done));
    assert!(can_show_match_pr(&comp_done));
    assert!(matches!(
        row_open_event(&comp_done),
        WorkspaceEvent::OpenMatchInAnalysis { match_id, game_no: None, position_id: None, .. }
            if match_id == comp_done.id
    ));

    // 4. Finished Competition money game (match_length == 0, play_phase == "game_over") -> Analysis
    let mut comp_money_done = sample_match(Uuid::new_v4(), 4, BOT_DATABASE_ID);
    comp_money_done.match_length = 0;
    comp_money_done.play_mode = Some(PlayMode::Competition);
    comp_money_done.play_session_id = Some(Uuid::new_v4());
    comp_money_done.play_phase = Some("game_over".to_string());
    assert!(!is_unfinished_competition(&comp_money_done));
    assert!(matches!(
        row_open_event(&comp_money_done),
        WorkspaceEvent::OpenMatchInAnalysis { match_id, .. } if match_id == comp_money_done.id
    ));

    // 5. Position-ID occurrence row -> OpenMatchInAnalysis preserving game_no & occurrence UUID
    let occ_id = Uuid::new_v4();
    let mut occ_match = sample_match(Uuid::new_v4(), 5, db_id);
    occ_match.occurrence = Some(LibraryOccurrence {
        id: occ_id,
        game_no: 3,
        move_no: 14,
        database_name: "Unsorted".to_string(),
    });
    assert!(matches!(
        row_open_event(&occ_match),
        WorkspaceEvent::OpenMatchInAnalysis {
            match_id,
            game_no: Some(3),
            position_id: Some(pid),
            decision_no: None,
        } if match_id == occ_match.id && pid == occ_id
    ));
}

#[test]
fn column_layout_visibility_min_one_reorder_resize_hidden_sort_and_prefs_round_trip() {
    let mut layout = ColumnLayoutState::default();
    assert_eq!(layout.order().len(), 15);
    assert_eq!(layout.ordered_visible().len(), 13);
    assert!(!layout.is_visible(LibraryDataColumn::Tournament));
    assert!(!layout.is_visible(LibraryDataColumn::Comment));

    // Sorting by a hidden column triggers the hidden-sort badge.
    assert!(layout.is_sort_column_hidden("tournament", false));
    assert!(!layout.is_sort_column_hidden("created_at", false));
    assert!(!layout.is_sort_column_hidden("occurrence_game", true));

    // Hide all columns except one -> min-1 guard refuses hiding the final visible column.
    for &col in LibraryDataColumn::ALL {
        if col != LibraryDataColumn::White {
            layout.toggle_visible(col, false);
        }
    }
    assert_eq!(layout.ordered_visible(), vec![LibraryDataColumn::White]);
    assert!(!layout.toggle_visible(LibraryDataColumn::White, false));
    assert_eq!(layout.ordered_visible(), vec![LibraryDataColumn::White]);

    // Show all, reorder, and resize with clamping.
    layout.show_all();
    assert_eq!(layout.ordered_visible().len(), 15);
    assert!(layout.move_column(LibraryDataColumn::White, -1));
    assert_eq!(layout.order()[0], LibraryDataColumn::White);
    assert_eq!(layout.order()[1], LibraryDataColumn::Id);

    layout.set_width("white", LibraryDataColumn::White.min_width(), 9999);
    assert_eq!(layout.width_of(LibraryDataColumn::White), MAX_COLUMN_WIDTH);
    layout.set_width("white", LibraryDataColumn::White.min_width(), 10);
    assert_eq!(
        layout.width_of(LibraryDataColumn::White),
        LibraryDataColumn::White.min_width()
    );

    // Round-trip through LocalUiState::library preferences.
    let mut prefs = crate::local_state::LibraryUiPrefs::default();
    layout.write_to_prefs(&mut prefs);
    let restored = ColumnLayoutState::from_prefs(&prefs);
    assert_eq!(restored, layout);
}

#[test]
fn restored_bulk_jobs_count_completed_failed_and_missing_separately() {
    let db = Uuid::new_v4();
    let completed = sample_job(Uuid::new_v4(), Uuid::new_v4(), "completed", 100);
    let mut cancelled = sample_job(Uuid::new_v4(), Uuid::new_v4(), "failed", 45);
    cancelled.error = Some("Cancelled by user".into());
    let missing = sample_job(Uuid::new_v4(), Uuid::new_v4(), "queued", 0);

    let mut original = BulkAnalysisTracker::default();
    original.track_jobs(
        &[
            (completed.clone(), db),
            (cancelled.clone(), db),
            (missing.clone(), db),
        ],
        false,
    );
    let mut prefs = crate::local_state::LibraryUiPrefs::default();
    original.write_to_prefs(&mut prefs);
    let mut restored = BulkAnalysisTracker::from_prefs(&prefs);
    assert!(restored.scope_progress(Some(db)).unwrap().loading);
    let mut unavailable = neondojo::databases::LibraryBulkJobStatus::from(&missing);
    unavailable.status = "missing".into();

    assert_eq!(
        restored.apply_batch(vec![(&completed).into(), (&cancelled).into(), unavailable]),
        (true, true)
    );
    assert_eq!(
        restored.scope_progress(Some(db)),
        Some(BulkAnalysisProgress {
            total: 3,
            completed: 1,
            failed: 1,
            missing: 1,
            loading: false
        })
    );
    // A previously queued snapshot cannot undo the terminal reconciliation.
    assert_eq!(
        restored.apply_batch(vec![(&missing).into()]),
        (false, false)
    );
    assert!(restored.active_job_ids().is_empty());
    restored.dismiss_if_finished();
    assert!(restored.scope_progress(None).is_none());
}

#[test]
fn database_rules_filter_chips_notes_validation_and_bulk_tracker_work() {
    let unsorted = sample_database(Uuid::from_u128(1), "Unsorted", true, true, 12);
    let empty_bot = sample_database(BOT_DATABASE_ID, "Bot Matches", true, false, 0);
    let paused = sample_database(PAUSED_MATCHES_DATABASE_ID, "Paused Matches", true, false, 4);
    let duels = sample_database(ENGINE_DUELS_DATABASE_ID, "Engine Duels", true, false, 3);
    let custom = sample_database(Uuid::new_v4(), "League 2026", false, false, 5);

    assert!(accepts_library_matches(unsorted.id));
    assert!(accepts_library_matches(empty_bot.id));
    assert!(!accepts_library_matches(paused.id));
    assert!(accepts_library_matches(duels.id));

    assert!(can_empty_database(&unsorted));
    assert!(!can_empty_database(&empty_bot));
    assert!(!can_empty_database(&paused));
    assert!(can_empty_database(&duels));
    assert!(!can_delete_database(&unsorted));
    assert!(can_delete_database(&custom));

    // Filter state -> chips & LibraryQuery mapping + occurrence sort fallback.
    let mut filters = LibraryFilterState {
        q: "Alice".to_string(),
        tournament: "Berlin".to_string(),
        match_type: "match".to_string(),
        position_id: "4HPwATDgc/ABMA".to_string(),
        ..LibraryFilterState::default()
    };
    let chips = filters.applied_chips(Locale::En);
    assert_eq!(chips.len(), 4);
    let q_with_pos = filters.to_library_query(None, "occurrence_game", SortDirection::Asc, 2);
    assert_eq!(q_with_pos.sort.as_deref(), Some("occurrence_game"));
    assert_eq!(q_with_pos.position_id.as_deref(), Some("4HPwATDgc/ABMA"));
    assert_eq!(q_with_pos.page, Some(2));

    assert!(filters.remove_filter("position_id"));
    let q_without_pos = filters.to_library_query(None, "occurrence_game", SortDirection::Asc, 1);
    assert_eq!(q_without_pos.sort.as_deref(), Some("created_at"));

    // Notes validation
    let mut notes = NotesSearchState::new();
    assert!(notes.validate(Locale::En).is_err());
    notes.query = "ab".to_string();
    assert!(notes.validate(Locale::En).is_err());
    notes.query = "cube".to_string();
    assert!(notes.validate(Locale::En).is_ok());

    // Bulk analysis tracker
    let mut tracker = BulkAnalysisTracker::default();
    let j1 = sample_job(Uuid::new_v4(), Uuid::new_v4(), "queued", 0);
    let j2 = sample_job(Uuid::new_v4(), Uuid::new_v4(), "running", 40);
    tracker.track_jobs(
        &[(j1.clone(), unsorted.id), (j2.clone(), unsorted.id)],
        false,
    );
    assert_eq!(
        tracker.scope_progress(Some(unsorted.id)),
        Some(BulkAnalysisProgress {
            total: 2,
            ..Default::default()
        })
    );

    let mut j1_done = j1.clone();
    j1_done.status = "completed".to_string();
    j1_done.progress = 100;
    let (changed, finished) = tracker.apply_batch(vec![(&j1_done).into()]);
    assert!(changed && finished);
    assert_eq!(
        tracker.scope_progress(Some(unsorted.id)),
        Some(BulkAnalysisProgress {
            total: 2,
            completed: 1,
            ..Default::default()
        })
    );

    // Import warning formatter covers all 6 variants cleanly.
    for kind in [
        ImportWarningKind::IllegalPlayReplaced,
        ImportWarningKind::CustomStartPosition,
        ImportWarningKind::UnfinishedGame,
        ImportWarningKind::IllegalPlayTruncated,
        ImportWarningKind::UnreadablePlayTruncated,
        ImportWarningKind::ScoreMismatch,
    ] {
        let mut w = ImportWarning::at(kind, 2, Some(7));
        w.source_line = Some(42);
        w.source_text = Some("bad move".to_string());
        w.recorded_score = Some([3, 1]);
        w.expected_score = Some([2, 1]);
        let text = format_import_warning(&w, Locale::En);
        assert!(!text.is_empty());
    }
}

// ── Headless GPUI Integration Tests ──────────────────────────────────────────

#[gpui_kit::test]
async fn headless_offline_render_and_mutually_exclusive_accordion_panels(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        configure_app(LocalUiState::default(), false, cx);
    });

    let (window, workspace) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| LibraryWorkspace::new(window, cx))
            })
        })
        .expect("open library window");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("library-workspace").visible());
        assert!(window.find("library-database-sidebar").visible());
        assert!(window.find("library-empty-state").visible());

        // Toggle Filters panel on
        window.click("library-panel-toggle-filters", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).active_search_panel,
            Some(SearchPanelKind::Filters)
        );
        assert!(window.find("library-panel-filters").visible());

        // Switch to Notes panel (mutually exclusive: Filters closes)
        window.click("library-panel-toggle-notes", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).active_search_panel,
            Some(SearchPanelKind::Notes)
        );
        assert!(window.find("library-panel-notes").visible());

        // Switch to Position ID panel
        window.click("library-panel-toggle-position", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).active_search_panel,
            Some(SearchPanelKind::Position)
        );
        assert!(window.find("library-panel-position").visible());

        // Switch to Duplicates panel and collapse it by clicking again
        window.click("library-panel-toggle-duplicates", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).active_search_panel,
            Some(SearchPanelKind::Duplicates)
        );
        assert!(window.find("library-panel-duplicates").visible());

        window.click("library-panel-toggle-duplicates", cx);
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).active_search_panel, None);
    })
    .expect("update window");
}

#[gpui_kit::test]
async fn headless_row_clicks_route_paused_and_competition_to_play_and_normal_to_analysis(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        configure_app(LocalUiState::default(), false, cx);
    });

    let emitted: Arc<Mutex<Vec<WorkspaceEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let emitted_sink = Arc::clone(&emitted);

    let (window, workspace) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let entity = cx.new(|cx| LibraryWorkspace::new(window, cx));
                cx.subscribe(&entity, move |_, event: &WorkspaceEvent, _| {
                    emitted_sink.lock().unwrap().push(event.clone());
                })
                .detach();
                entity
            })
        })
        .expect("open library window");

    let unsorted_id = Uuid::from_u128(1);
    let paused_session = Uuid::new_v4();
    let comp_session = Uuid::new_v4();
    let normal_match = sample_match(Uuid::new_v4(), 101, unsorted_id);
    let normal_id = normal_match.id;

    let mut paused_match = sample_match(Uuid::new_v4(), 102, PAUSED_MATCHES_DATABASE_ID);
    paused_match.play_paused = true;
    paused_match.play_session_id = Some(paused_session);
    paused_match.play_phase = Some("move".to_string());
    let paused_id = paused_match.id;

    let mut comp_match = sample_match(Uuid::new_v4(), 103, BOT_DATABASE_ID);
    comp_match.play_mode = Some(PlayMode::Competition);
    comp_match.play_session_id = Some(comp_session);
    comp_match.play_phase = Some("roll".to_string());
    let comp_id = comp_match.id;

    let dbs = vec![
        sample_database(unsorted_id, "Unsorted", true, true, 1),
        sample_database(BOT_DATABASE_ID, "Bot Matches", true, false, 1),
        sample_database(PAUSED_MATCHES_DATABASE_ID, "Paused Matches", true, false, 1),
        sample_database(ENGINE_DUELS_DATABASE_ID, "Engine Duels", true, false, 0),
    ];

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |ws, cx| {
            ws.seed_test_data(dbs, vec![normal_match, paused_match, comp_match]);
            cx.notify();
        });
        window.render_frame(cx);

        // Clicking normal match -> OpenMatchInAnalysis
        window.click(
            SharedString::from(format!("library-row-open-{normal_id}")),
            cx,
        );
        // Clicking paused match -> OpenPlaySession { resume: true }
        window.click(
            SharedString::from(format!("library-row-open-{paused_id}")),
            cx,
        );
        // Clicking unfinished Competition match -> OpenPlaySession { resume: true }
        window.click(
            SharedString::from(format!("library-row-open-{comp_id}")),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("click rows");

    let events = emitted.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        WorkspaceEvent::OpenMatchInAnalysis { match_id, .. } if *match_id == normal_id
    ));
    assert!(matches!(
        &events[1],
        WorkspaceEvent::OpenPlaySession { session_id, resume: true } if *session_id == paused_session
    ));
    assert!(matches!(
        &events[2],
        WorkspaceEvent::OpenPlaySession { session_id, resume: true } if *session_id == comp_session
    ));
}

#[gpui_kit::test]
async fn new_match_picker_opens_the_paused_database_and_resumes_either_row(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::shell::{AppShell, AppView, WorkspaceId};
    use std::{cell::Cell, rc::Rc};

    let (window, shell) = cx.update(|cx| {
        gpui_kit::init(cx);
        configure_app(LocalUiState::default(), false, cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| AppShell::new(None, window, cx))
        })
        .unwrap()
    });
    let rows = [
        (Uuid::new_v4(), Uuid::new_v4()),
        (Uuid::new_v4(), Uuid::new_v4()),
    ];
    let resumed = Rc::new(Cell::new(None));

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-tab-library", cx);
        let library = shell.read(cx).library_workspace().unwrap();
        library.update(cx, |ws, cx| {
            let matches = rows
                .iter()
                .enumerate()
                .map(|(i, &(match_id, session_id))| {
                    let mut m = sample_match(match_id, i as i64 + 1, PAUSED_MATCHES_DATABASE_ID);
                    m.play_paused = true;
                    m.play_session_id = Some(session_id);
                    m
                })
                .collect();
            ws.seed_test_data(
                vec![sample_database(
                    PAUSED_MATCHES_DATABASE_ID,
                    "Paused Matches",
                    true,
                    false,
                    2,
                )],
                matches,
            );
            cx.notify();
        });
        let observed = resumed.clone();
        cx.subscribe(&library, move |_, event, _| {
            if let WorkspaceEvent::OpenPlaySession {
                session_id,
                resume: true,
            } = event
            {
                observed.set(Some(*session_id));
            }
        })
        .detach();

        window.render_frame(cx);
        window.click("workspace-tab-play", cx);
    })
    .unwrap();
    for (match_id, session_id) in rows {
        cx.update_window(window, |_, window, cx| {
            window.press("cmd-n", cx);
            window.render_frame(cx);
            window.click("play-setup-paused-matches", cx);
        })
        .unwrap();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                shell.read(cx).active_view(),
                AppView::Workspace(WorkspaceId::Library)
            );
            assert_eq!(
                shell
                    .read(cx)
                    .library_workspace()
                    .unwrap()
                    .read(cx)
                    .selected_database_id,
                Some(PAUSED_MATCHES_DATABASE_ID)
            );
            window.click(
                SharedString::from(format!("library-row-check-{match_id}")),
                cx,
            );
            window.click("library-batch-resume-btn", cx);
        })
        .unwrap();
        assert_eq!(resumed.get(), Some(session_id));
        assert_eq!(
            cx.update(|cx| shell.read(cx).active_view()),
            AppView::Workspace(WorkspaceId::Play)
        );
    }
}

#[gpui_kit::test]
async fn headless_position_search_occurrence_columns_column_picker_and_import_paths(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        configure_app(LocalUiState::default(), false, cx);
    });

    let (window, workspace) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| LibraryWorkspace::new(window, cx))
            })
        })
        .expect("open library window");

    let unsorted_id = Uuid::from_u128(1);
    let mut occ_match = sample_match(Uuid::new_v4(), 7, unsorted_id);
    occ_match.analysis_status = Some("partial".to_string());
    occ_match.occurrence = Some(LibraryOccurrence {
        id: Uuid::new_v4(),
        game_no: 2,
        move_no: 9,
        database_name: "Unsorted".to_string(),
    });
    let row_key = library_row_key(&occ_match);
    let dbs = vec![sample_database(unsorted_id, "Unsorted", true, true, 1)];

    // Non-hidden subfolder containing unsupported & hidden files to verify offline import_paths.
    let tmp = tempdir().expect("tempdir");
    let import_dir = tmp.path().join("Matches");
    fs::create_dir_all(&import_dir).unwrap();
    fs::write(import_dir.join("notes.pdf"), b"ignored").unwrap();
    fs::write(import_dir.join(".DS_Store"), b"hidden").unwrap();

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |ws, cx| {
            ws.seed_test_data(dbs, vec![occ_match]);
            // Invalid Position ID sets validation error without activating position filter.
            ws.apply_position_search("short".to_string(), cx);
            assert!(ws.position_error.is_some());
            assert!(!ws.filters.has_position_filter());

            // Valid 14-char Position ID activates dynamic occurrence columns.
            ws.apply_position_search("4HPwATDgc/ABMA".to_string(), cx);
            assert!(ws.position_error.is_none());
            assert!(ws.filters.has_position_filter());
        });
        window.render_frame(cx);

        // All 3 dynamic occurrence headers & applied position chip render.
        assert!(window.find("library-th-occurrence_database").visible());
        assert!(window.find("library-th-occurrence_game").visible());
        assert!(window.find("library-th-occurrence_move").visible());
        assert!(window.find("library-applied-filter-chips").visible());

        // Select row via checkbox and verify selection state updates.
        window.click(
            SharedString::from(format!("library-row-check-{row_key}")),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).selection.len(), 1);

        // Open column picker, hide `created_at` (currently sorted column), verify hidden-sort badge.
        window.click("library-columns-picker-btn", cx);
        window.render_frame(cx);
        assert!(window.find("library-columns-popover").visible());

        window.click("library-col-toggle-created_at", cx);
        window.render_frame(cx);
        assert!(window.find("library-hidden-sort-badge").visible());
        assert!(
            !cx.global::<LocalUiState>()
                .library
                .visible_columns
                .iter()
                .any(|c| c == "created_at")
        );

        // Reorder White column left and verify persisted order updates.
        window.click("library-col-left-white", cx);
        window.render_frame(cx);
        assert_eq!(
            cx.global::<LocalUiState>()
                .library
                .column_order
                .first()
                .map(String::as_str),
            Some("white")
        );

        // import_paths handles unsupported/hidden folder gracefully offline.
        workspace.update(cx, |ws, cx| {
            ws.import_paths(vec![import_dir.clone()], cx);
        });
        window.render_frame(cx);
        assert!(window.find("library-import-run-panel").visible());
        let run = workspace.read(cx).import_run.as_ref().expect("import run");
        assert!(!run.running);
        assert_eq!(run.skipped_unsupported, 1);
        assert_eq!(run.ignored_hidden, 1);
        assert!(run.error.is_some());
    })
    .expect("headless position, columns, and import checks");
}

#[test]
fn import_run_cooperative_cancel_and_warning_accounting() {
    let mut run = ImportRunState::start(3, 1, 2, "Unsorted");
    assert!(run.running);
    assert!(!run.cancel_flag.load(std::sync::atomic::Ordering::SeqCst));

    run.record_row(ImportFileRow {
        path: PathBuf::from("/tmp/a.sgf"),
        display_name: "a.sgf".to_string(),
        status: ImportFileStatus::Imported {
            match_id: Uuid::new_v4(),
            position_count: 88,
            warnings: vec![ImportWarning::at(
                ImportWarningKind::CustomStartPosition,
                1,
                None,
            )],
        },
    });
    assert_eq!(run.done, 1);
    assert_eq!(run.imported, 1);
    assert_eq!(run.total_warning_count(), 1);

    run.request_stop();
    assert!(run.stopping);
    assert!(run.cancel_flag.load(std::sync::atomic::Ordering::SeqCst));

    run.finish(true);
    assert!(!run.running);
    assert!(!run.stopping);
    assert!(run.stopped);
}

#[gpui_kit::test]
async fn headless_sidebar_switching_sort_toggle_and_visibility_lifecycle(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        configure_app(LocalUiState::default(), false, cx);
    });

    let (window, workspace) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| LibraryWorkspace::new(window, cx))
            })
        })
        .expect("open library window");

    let unsorted_id = Uuid::from_u128(1);
    let custom_id = Uuid::new_v4();
    let m1 = sample_match(Uuid::new_v4(), 1, custom_id);
    let dbs = vec![
        sample_database(unsorted_id, "Unsorted", true, true, 0),
        sample_database(BOT_DATABASE_ID, "Bot Matches", true, false, 0),
        sample_database(PAUSED_MATCHES_DATABASE_ID, "Paused Matches", true, false, 0),
        sample_database(ENGINE_DUELS_DATABASE_ID, "Engine Duels", true, false, 0),
        sample_database(custom_id, "League 2026", false, false, 1),
    ];

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |ws, cx| {
            ws.seed_test_data(dbs, vec![m1]);
            cx.notify();
        });
        window.render_frame(cx);

        // Select custom database in sidebar -> active DB actions appear & prefs persist.
        window.click(SharedString::from(format!("library-db-{custom_id}")), cx);
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).selected_database_id, Some(custom_id));
        assert!(window.find("library-active-db-actions").visible());
        assert_eq!(
            cx.global::<LocalUiState>().library.database_id,
            custom_id.to_string()
        );

        // Click White column header twice -> toggles sort to Desc then Asc.
        window.click("library-th-white", cx);
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).sort, "white");
        assert_eq!(workspace.read(cx).direction, SortDirection::Desc);

        window.click("library-th-white", cx);
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).direction, SortDirection::Asc);

        // Hide and re-show workspace cleanly via set_visible + refresh_on_library_mutated.
        workspace.update(cx, |ws, cx| {
            ws.set_visible(false, cx);
            assert!(!ws.visible);
            ws.set_visible(true, cx);
            assert!(ws.visible);
            ws.refresh_on_library_mutated(cx);
        });
        window.render_frame(cx);
        assert!(window.find("library-workspace").visible());
    })
    .expect("sidebar, sort, and visibility checks");
}
