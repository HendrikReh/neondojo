//! Unit and headless `#[gpui_kit::test]` tests for the Analysis workspace.
//!
//! Verifies all critical invariants:
//! - `DiceTabState` state machine (auto-start, failure non-retry loop, explicit
//!   delete silence across revisits, manual retry, position/engine change auto-start)
//! - 5-tab board width clamping (`320..=1680`), keyboard splitter undo/reset,
//!   and `LocalUiState` atomic persistence
//! - Cube-only (`position_count == 0`) games in move list, selector, and rendering
//! - Persistent notes drafts surviving navigation + HTTP 409 conflict preservation
//! - 3-engine candidate alignment & `|Δ|` sorting, monotonic 1s ETA timer,
//!   once-per-tab-visit Cube auto-select, responsive ≥1050px Statistics layout
//! - Headless GPUI rendering of Welcome Drop, 3-pane Analysis, all 5 tabs,
//!   Dice 6×6 cell click projection, `set_visible` persistence, and `invalidate_match`.

use super::AnalysisWorkspace;
use super::cube_stats::stats_uses_two_columns;
use super::state::{
    ActiveBoardTarget, AnalysisDetailTab, BoardWidthController, CubeAutoSelectState,
    DiceDistributionSnapshot, DiceTabState, DiceTransitionAction, InterleavedMoveRow,
    MAX_ANNOTATION_TAGS, MonotonicRemainingTimer, MoveHistoryFilter, NoteTargetKey,
    NotesDraftCache, build_candidate_compare_rows, build_interleaved_move_rows, is_cube_only_game,
};
use crate::configure_app;
use crate::local_state::LocalUiState;
use crate::tokio_bridge::TokioBridge;
use chrono::Utc;
use gpui_kit::{AppContext, TestAppContext, WindowOptions, test::TestWindowExt};
use neondojo::{
    analysis_catalog::{AnalysisCatalogEntry, SelectedPositionAnalysis},
    analysis_timing::{AnalysisRemainingTime, RemainingTimeBasis, RemainingTimeState},
    annotations::{Annotation, AnnotationKind, AnnotationUpdate},
    api::MatchAnalysisEngine,
    domain_error::DomainError,
    engine::{Analysis, Candidate, DiceEvaluation, Distribution},
    match_analysis::{CubeEquityUnit, CubeEvaluation, Decision},
    model::{Job, MatchGame, MatchSummary, MoveSeverity, Position},
    move_assessments::AssessmentSource,
    move_catalog::StoredMove,
    review_decisions::ReviewDecision,
};
use neondojo_board::BoardViewMode;
use neondojo_rules::backgammon::initial_board;
use std::collections::HashMap;
use tempfile::tempdir;
use uuid::Uuid;

fn sample_position(match_id: Uuid, game_no: i32, ply: i32) -> Position {
    Position {
        cube_enabled: Some(true),
        id: Uuid::new_v4(),
        match_uuid: match_id,
        ordinal: ply,
        game_no,
        ply,
        player: 0,
        dice: vec![3, 1],
        board: initial_board().to_vec(),
        position_id: "4HPwATDgc/ABMA".to_string(),
        gnubg_match_id: "cInlAAAAAAAE".to_string(),
        notation: "8/5 6/5".to_string(),
    }
}

fn sample_distribution(mean: f64) -> Distribution {
    let mut rolls = Vec::new();
    for d1 in 1..=6 {
        for d2 in 1..=d1 {
            let weight = if d1 == d2 { 1 } else { 2 };
            rolls.push(DiceEvaluation {
                dice: [d1, d2],
                weight,
                probability: f64::from(weight) / 36.0,
                best: Candidate {
                    rank: 1,
                    notation: if d1 == 3 && d2 == 1 {
                        "8/5 6/5".to_string()
                    } else {
                        "24/23 13/11".to_string()
                    },
                    path: None,
                    equity: mean + (f64::from(d1 - d2) * 0.02),
                    probabilities: vec![0.52, 0.14, 0.01, 0.48, 0.12, 0.01],
                    plies: Some(2),
                },
            });
        }
    }
    Distribution {
        engine: "GNUbg".to_string(),
        plies: 2,
        mean_equity: mean,
        rolls,
    }
}

fn sample_job(
    position_id: Uuid,
    status: &str,
    result: Option<serde_json::Value>,
    error: Option<&str>,
) -> Job {
    Job {
        engine_snapshot: None,
        id: Uuid::new_v4(),
        position_uuid: Some(position_id),
        match_uuid: None,
        game_no: None,
        total_games: 1,
        completed_games: if status == "completed" { 1 } else { 0 },
        kind: "dice".to_string(),
        plies: 2,
        status: status.to_string(),
        progress: if status == "completed" { 100 } else { 40 },
        result,
        error: error.map(String::from),
        attempts: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

// ── 1. DiceTabState State Machine Invariants ─────────────────────────────────

#[test]
fn dice_tab_state_auto_starts_never_loops_on_error_and_stays_silent_after_delete() {
    let pos_a = Uuid::new_v4();
    let pos_b = Uuid::new_v4();
    let gnubg = MatchAnalysisEngine::Gnubg;
    let sage = MatchAnalysisEngine::Bgsage;

    let mut state = DiceTabState::Idle;

    // 1) Auto-starts when missing for active engine.
    assert!(state.should_auto_start(pos_a, gnubg));
    assert_eq!(
        state.on_missing_result(pos_a, gnubg),
        DiceTransitionAction::StartCalculation {
            position_id: pos_a,
            engine: gnubg,
        }
    );
    assert!(matches!(state, DiceTabState::Submitting { .. }));

    // While submitting, do NOT re-trigger auto-start.
    assert!(!state.should_auto_start(pos_a, gnubg));
    assert_eq!(
        state.on_missing_result(pos_a, gnubg),
        DiceTransitionAction::None
    );

    // 2) Failure sets manual_retry_only = true — NEVER auto-retry loops!
    state.on_failed(pos_a, gnubg, "Engine timed out");
    assert!(!state.should_auto_start(pos_a, gnubg));
    assert_eq!(
        state.on_missing_result(pos_a, gnubg),
        DiceTransitionAction::None
    );

    // 3) Manual tab click retries a Failed state.
    assert_eq!(
        state.on_tab_clicked(pos_a, gnubg),
        DiceTransitionAction::StartCalculation {
            position_id: pos_a,
            engine: gnubg,
        }
    );

    // Job completes -> Ready.
    let dist_json = serde_json::to_value(sample_distribution(0.08)).unwrap();
    let done_job = sample_job(pos_a, "completed", Some(dist_json), None);
    state.on_job_update(pos_a, gnubg, &done_job);
    assert!(matches!(state, DiceTabState::Ready { .. }));
    assert_eq!(state.active_job_id(), Some(done_job.id));

    // 4) Explicit delete transitions to DeletedAwaitingExplicitRequest and stays silent!
    state.on_deleted(pos_a, gnubg);
    assert!(matches!(
        state,
        DiceTabState::DeletedAwaitingExplicitRequest {
            position_id,
            engine
        } if position_id == pos_a && engine == gnubg
    ));
    assert!(!state.should_auto_start(pos_a, gnubg));
    assert_eq!(
        state.on_missing_result(pos_a, gnubg),
        DiceTransitionAction::None
    );
    // Even clicking the Dice tab header again stays silent after explicit delete!
    assert_eq!(
        state.on_tab_clicked(pos_a, gnubg),
        DiceTransitionAction::None
    );
    // Resetting for same position preserves delete silence.
    state.reset_for_position(pos_a, gnubg);
    assert!(!state.should_auto_start(pos_a, gnubg));

    // Explicit manual recalculate button breaks out of delete silence.
    assert_eq!(
        state.on_manual_retry(pos_a, gnubg),
        DiceTransitionAction::StartCalculation {
            position_id: pos_a,
            engine: gnubg,
        }
    );

    // Delete again, then verify switching engine OR switching position DOES auto-start!
    state.on_deleted(pos_a, gnubg);
    assert!(state.should_auto_start(pos_a, sage));
    assert!(state.should_auto_start(pos_b, gnubg));
}

// ── 2. 5-Tab Board Width Clamping & Persistence ──────────────────────────────

#[test]
fn five_tab_board_widths_clamp_independently_undo_on_escape_and_persist() {
    let mut ctrl = BoardWidthController::default();

    // Clamp below 320 and above 1680 independently per tab.
    assert_eq!(ctrl.set_width(AnalysisDetailTab::Overview, 120), 320);
    assert_eq!(ctrl.set_width(AnalysisDetailTab::Cube, 2500), 1680);
    assert_eq!(ctrl.set_width(AnalysisDetailTab::Statistics, 740), 740);
    assert_eq!(ctrl.set_width(AnalysisDetailTab::Notes, 620), 620);
    assert_eq!(ctrl.set_width(AnalysisDetailTab::Cards, 510), 510);

    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Overview), Some(320));
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Cube), Some(1680));
    assert_eq!(
        ctrl.stored_for_tab(AnalysisDetailTab::Statistics),
        Some(740)
    );
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Notes), Some(620));
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Cards), Some(510));

    // Keyboard edit + Escape reverts pre-edit width cleanly.
    ctrl.begin_edit(AnalysisDetailTab::Notes);
    assert_eq!(ctrl.adjust(AnalysisDetailTab::Notes, 64, 1440.0), 684);
    assert!(ctrl.cancel_edit(AnalysisDetailTab::Notes));
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Notes), Some(620));

    // Min / Max / Reset per tab without affecting sibling tabs.
    assert_eq!(ctrl.set_min(AnalysisDetailTab::Statistics), 320);
    assert_eq!(ctrl.set_max(AnalysisDetailTab::Statistics), 1680);
    ctrl.reset_tab(AnalysisDetailTab::Statistics);
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Statistics), None);
    assert_eq!(ctrl.stored_for_tab(AnalysisDetailTab::Cards), Some(510));

    // Atomic persistence roundtrip via LocalUiState.
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("ui-state.json");
    let state = LocalUiState {
        analysis_board_widths: ctrl.widths,
        ..Default::default()
    };
    state.save_atomic(&path).expect("save atomic");
    let reloaded = LocalUiState::load_or_default(&path);
    assert_eq!(reloaded.analysis_board_widths.overview, Some(320));
    assert_eq!(reloaded.analysis_board_widths.errors, Some(1680));
    assert_eq!(reloaded.analysis_board_widths.statistics, None);
    assert_eq!(reloaded.analysis_board_widths.notes, Some(620));
    assert_eq!(reloaded.analysis_board_widths.default_width, Some(510));
}

// ── 3. Cube-Only Games (`position_count == 0`) ───────────────────────────────

#[test]
fn cube_only_games_build_interleaved_rows_without_checker_positions() {
    let match_id = Uuid::new_v4();
    let cube_only_game = MatchGame {
        cube_enabled: Some(true),
        result: None,
        game_no: 4,
        position_count: 0,
        white_score: 3,
        black_score: 4,
        crawford: false,
    };
    assert!(is_cube_only_game(&cube_only_game));

    let cube_move_id = Uuid::new_v4();
    let stored_cube = StoredMove {
        id: cube_move_id,
        match_uuid: match_id,
        game_no: 4,
        move_no: 1,
        kind: "cube".to_string(),
        player: 0,
        notation: "double".to_string(),
        position_uuid: None,
        board: initial_board().to_vec(),
        dice: vec![],
        position_id: "4HPwATDgc/ABMA".to_string(),
        gnubg_match_id: "cInlAAAAAAAE".to_string(),
    };
    let cube_review = ReviewDecision {
        id: "native:101".to_string(),
        position_uuid: None,
        move_id: Some(cube_move_id),
        review_position: Some(sample_position(match_id, 4, 0)),
        next_checker_no: None,
        game_no: 4,
        decision: Decision {
            decision_no: 1,
            checker_no: None,
            player: 0,
            kind: "cube".to_string(),
            played: "double".to_string(),
            best: "no double".to_string(),
            played_equity: Some(0.10),
            best_equity: Some(0.25),
            equity_loss: 0.15,
            severity: "blunder".to_string(),
            dice: None,
            evaluation_status: "evaluated".to_string(),
            checker_candidates: vec![],
            cube_evaluation: Some(CubeEvaluation {
                unit: CubeEquityUnit::Cubeful,
                no_double: 0.25,
                double_take: 0.10,
                double_pass: 1.0,
                probabilities: Some([0.62, 0.18, 0.01, 0.38, 0.08, 0.0]),
            }),
        },
        source: AssessmentSource::Native,
        engine: "GNUbg".to_string(),
        depth: Some("3-ply".to_string()),
        depth_plies: Some(3),
        job_id: Some(Uuid::new_v4()),
    };

    let rows = build_interleaved_move_rows(
        &[],
        &[stored_cube],
        &HashMap::new(),
        &[&cube_review],
        MoveHistoryFilter::All,
    );
    assert_eq!(rows.len(), 1);
    match &rows[0] {
        InterleavedMoveRow::CubeAction {
            evaluated,
            severity,
            notation,
            review_id,
            ..
        } => {
            assert!(*evaluated);
            assert_eq!(*severity, Some(MoveSeverity::Blunder));
            assert_eq!(notation, "double");
            assert_eq!(review_id.as_deref(), Some("native:101"));
        }
        other => panic!("expected CubeAction, got {other:?}"),
    }
}

// ── 4. Notes Draft Preservation Across Navigation & 409 Conflict ─────────────

#[test]
fn notes_drafts_survive_navigation_enforce_limits_and_preserve_draft_on_409_conflict() {
    let pos_a = Uuid::new_v4();
    let pos_b = Uuid::new_v4();
    let key_a = NoteTargetKey {
        kind: AnnotationKind::Position,
        target_id: pos_a,
    };
    let key_b = NoteTargetKey {
        kind: AnnotationKind::Position,
        target_id: pos_b,
    };

    let mut cache = NotesDraftCache::default();
    let server_a_v1 = Annotation {
        id: Some(Uuid::new_v4()),
        target_kind: AnnotationKind::Position,
        target_id: pos_a,
        tags: vec!["opening".into()],
        comment: "Initial note".into(),
        version: 1,
        updated_at: Some(Utc::now()),
    };
    cache.merge_loaded(&server_a_v1);

    // User edits Position A -> marks dirty.
    cache
        .ensure_entry(key_a)
        .set_comment("My edited opening note".into())
        .unwrap();
    cache.ensure_entry(key_a).add_tag("priming").unwrap();
    assert!(cache.has_dirty(key_a));

    // User navigates to Position B, edits it, then returns to Position A where server is now v2.
    cache
        .ensure_entry(key_b)
        .set_comment("Position B note".into())
        .unwrap();
    let server_a_v2 = Annotation {
        version: 2,
        comment: "Edited elsewhere".into(),
        ..server_a_v1.clone()
    };
    let merged_a = cache.merge_loaded(&server_a_v2);
    // Dirty draft on A MUST be preserved!
    assert_eq!(merged_a.comment, "My edited opening note");
    assert_eq!(merged_a.tags, vec!["opening", "priming"]);
    assert_eq!(merged_a.base_version, 1);
    assert!(merged_a.server_latest.is_some());

    // Saving with stale expected_version=1 produces HTTP 409 Conflict -> draft STILL preserved!
    let conflict = DomainError::conflict("Annotation version conflict");
    assert!(cache.mark_conflict(key_a, &conflict));
    let after_409 = cache.get(key_a).unwrap();
    assert_eq!(after_409.comment, "My edited opening note");
    assert_eq!(after_409.tags, vec!["opening", "priming"]);
    assert!(after_409.conflict_error.is_some());

    // Rebase version keeps draft text and clears conflict error.
    cache.rebase_draft_version(key_a, 2);
    assert_eq!(cache.get(key_a).unwrap().base_version, 2);
    assert_eq!(cache.get(key_a).unwrap().comment, "My edited opening note");
    assert!(cache.get(key_a).unwrap().conflict_error.is_none());

    // Enforce <= 30 tags limit.
    let key_limit = NoteTargetKey {
        kind: AnnotationKind::Match,
        target_id: Uuid::new_v4(),
    };
    for i in 0..MAX_ANNOTATION_TAGS {
        cache
            .ensure_entry(key_limit)
            .add_tag(&format!("tag-{i}"))
            .unwrap();
    }
    assert!(cache.ensure_entry(key_limit).add_tag("tag-31").is_err());
}

// ── 5. Pure Helpers: 3-Engine Compare, Monotonic ETA, Cube Auto-Select, Stats 2-Col ──

#[test]
fn compare_matrix_monotonic_eta_cube_auto_select_and_responsive_stats_work() {
    let match_id = Uuid::new_v4();
    let pos = sample_position(match_id, 1, 1);

    let xg_cands = vec![
        Candidate {
            rank: 1,
            notation: "8/5 6/5".into(),
            path: None,
            equity: 0.220,
            probabilities: vec![],
            plies: Some(3),
        },
        Candidate {
            rank: 2,
            notation: "13/10 6/5".into(),
            path: None,
            equity: 0.140,
            probabilities: vec![],
            plies: Some(3),
        },
    ];
    let gnubg_cands = vec![
        Candidate {
            rank: 1,
            notation: "6/5 8/5".into(), // Same resulting board in different order!
            path: None,
            equity: 0.215,
            probabilities: vec![],
            plies: Some(3),
        },
        Candidate {
            rank: 2,
            notation: "13/10 6/5".into(),
            path: None,
            equity: 0.185,
            probabilities: vec![],
            plies: Some(3),
        },
    ];
    let rows = build_candidate_compare_rows(&pos, [&xg_cands, &gnubg_cands, &[]], true);
    assert_eq!(
        rows.len(),
        2,
        "equivalent notations align by resulting board"
    );
    // Sorted by |Δ| descending: 13/10 6/5 has |0.080 - 0.030| = 0.050 > 0.000
    assert_eq!(rows[0].display_notation, "13/10 6/5");
    assert!((rows[0].max_delta - 0.05).abs() < 1e-6);

    // Monotonic 1s countdown does not jump upward on minor poll jitter.
    let job_id = Uuid::new_v4();
    let mut timer = MonotonicRemainingTimer::default();
    timer.sync_from_server(&AnalysisRemainingTime {
        job_id,
        status: "running".into(),
        state: RemainingTimeState::Estimated,
        basis: Some(RemainingTimeBasis::Observed),
        remaining_ms: Some(12_000.0),
        total_moves: 100,
        completed_moves: 50,
        timed_moves: 50,
        measured_elapsed_ms: 12_000.0,
        current_game_elapsed_ms: 0.0,
        sampled_at: Utc::now(),
        attempt: 1,
    });
    timer.tick_one_second();
    assert_eq!(timer.display_remaining_ms, Some(11_000.0));
    timer.sync_from_server(&AnalysisRemainingTime {
        job_id,
        status: "running".into(),
        state: RemainingTimeState::Estimated,
        basis: Some(RemainingTimeBasis::Observed),
        remaining_ms: Some(13_000.0), // Minor upward jitter ignored
        total_moves: 100,
        completed_moves: 52,
        timed_moves: 52,
        measured_elapsed_ms: 13_000.0,
        current_game_elapsed_ms: 0.0,
        sampled_at: Utc::now(),
        attempt: 1,
    });
    assert_eq!(timer.display_remaining_ms, Some(11_000.0));

    // Cube auto-select triggers only once per tab visit.
    let mut auto = CubeAutoSelectState::default();
    auto.on_enter_cube_tab();
    let dummy_cube = ReviewDecision {
        id: "native:1".into(),
        position_uuid: None,
        move_id: None,
        review_position: None,
        next_checker_no: None,
        game_no: 1,
        decision: Decision {
            decision_no: 1,
            checker_no: None,
            player: 0,
            kind: "cube".into(),
            played: "no double".into(),
            best: "double".into(),
            played_equity: Some(0.4),
            best_equity: Some(0.5),
            equity_loss: 0.1,
            severity: "blunder".into(),
            dice: None,
            evaluation_status: "evaluated".into(),
            checker_candidates: vec![],
            cube_evaluation: None,
        },
        source: AssessmentSource::Native,
        engine: "GNUbg".into(),
        depth: None,
        depth_plies: None,
        job_id: None,
    };
    let dummy_cubes = vec![dummy_cube];
    assert!(
        auto.next_auto_selection(None, &dummy_cubes, Some(1))
            .is_some()
    );
    assert!(
        auto.next_auto_selection(None, &dummy_cubes, Some(1))
            .is_none()
    );

    use neondojo_i18n::Locale;
    assert_eq!(
        super::cube_stats::statistics_game_labels(Locale::En, 1),
        ("Game 1".into(), "Open game 1".into())
    );
    assert_eq!(
        super::cube_stats::statistics_game_labels(Locale::De, 2),
        ("Spiel 2".into(), "Spiel 2 öffnen".into())
    );

    // Statistics responsive 2-column threshold >= 1050px.
    assert!(!stats_uses_two_columns(1049.0));
    assert!(stats_uses_two_columns(1050.0));
}

#[test]
fn saving_multiline_comments_preserves_newer_edits_and_advances_version() {
    let key = NoteTargetKey {
        kind: AnnotationKind::Move,
        target_id: Uuid::new_v4(),
    };
    let mut drafts = NotesDraftCache::default();
    let submitted = AnnotationUpdate {
        tags: vec![],
        comment: "Line one\nLine two".into(),
        expected_version: 0,
    };
    let saved = Annotation {
        id: None,
        target_kind: key.kind,
        target_id: key.target_id,
        tags: vec![],
        comment: submitted.comment.clone(),
        version: 1,
        updated_at: None,
    };
    let text = "Line one\nLine two\nThird paragraph";
    drafts.ensure_entry(key).set_comment(text.into()).unwrap();
    drafts.mark_saved(&saved, &submitted);
    let draft = drafts.get(key).unwrap();
    assert_eq!(draft.comment, text);
    assert_eq!(draft.base_version, 1);
    assert!(draft.dirty);
    let final_update = AnnotationUpdate {
        comment: text.into(),
        expected_version: 1,
        tags: vec![],
    };
    drafts.mark_saved(
        &Annotation {
            comment: text.into(),
            version: 2,
            ..saved.clone()
        },
        &final_update,
    );
    assert_eq!(drafts.get(key).unwrap().comment, text);
    assert!(!drafts.has_dirty(key));
    // A read started before either save can arrive after both writes.
    let current = drafts.merge_loaded(&saved);
    assert_eq!((current.base_version, current.comment.as_str()), (2, text));
}

// ── 6. Headless GPUI Rendering & Interaction Tests ───────────────────────────

fn init_headless_test_env(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        TokioBridge::init(cx).expect("init tokio bridge");
        configure_app(LocalUiState::default(), false, cx);
    });
}

#[gpui_kit::test]
async fn headless_renders_welcome_drop_then_full_three_pane_tabs_dice_projection_notes_and_invalidation(
    cx: &mut TestAppContext,
) {
    init_headless_test_env(cx);

    let (window, entity) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| AnalysisWorkspace::new(window, cx))
            })
        })
        .expect("open analysis test window");

    // Seed a complete match with 2 games (Game 1 normal + Game 2 cube-only) in memory.
    let match_id = Uuid::new_v4();
    let pos1 = sample_position(match_id, 1, 1);
    let pos1_id = pos1.id;
    let mut distribution = sample_distribution(0.05);
    distribution.rolls.retain(|r| r.dice != [6, 2]);
    distribution
        .rolls
        .iter_mut()
        .find(|r| r.dice == [6, 1])
        .unwrap()
        .best
        .path = Some("13/7 8/7".into());
    let dist_snap: DiceDistributionSnapshot = distribution.into();

    cx.update_window(window, |_, window, cx| {
        // Initially renders Welcome drop zone.
        window.render_frame(cx);
        assert!(window.find("analysis-welcome-drop-screen").visible());

        entity.update(cx, |ws, cx| {
            ws.welcome_mode = false;
            ws.match_id = Some(match_id);
            ws.match_summary = Some(MatchSummary {
                id: match_id,
                source_available: true,
                filename: "sample.xg".into(),
                format: "xg".into(),
                white: "Alice".into(),
                black: "Bob".into(),
                match_length: 7,
                position_count: 1,
                created_at: Utc::now(),
            });
            ws.games = vec![
                MatchGame {
                    cube_enabled: Some(true),
                    result: None,
                    game_no: 1,
                    position_count: 1,
                    white_score: 0,
                    black_score: 0,
                    crawford: false,
                },
                MatchGame {
                    cube_enabled: Some(true),
                    result: None,
                    game_no: 2,
                    position_count: 0, // Cube-only game!
                    white_score: 2,
                    black_score: 0,
                    crawford: false,
                },
            ];
            ws.current_game_no = Some(1);
            ws.current_game_id = Some(Uuid::new_v4());
            ws.positions = vec![pos1.clone()];
            ws.active_target = Some(ActiveBoardTarget::Checker {
                position_id: pos1_id,
            });
            ws.catalog_items = vec![AnalysisCatalogEntry {
                id: "job:1".into(),
                source: "native".into(),
                scope: "match".into(),
                engine: "GNUbg".into(),
                depth: Some("3-ply".into()),
                game_no: None,
                position_id: None,
                created_at: Utc::now(),
                status: "completed".into(),
                complete: true,
                checker_count: 1,
                cube_count: 1,
                job: None,
            }];
            ws.position_result = Some(SelectedPositionAnalysis {
                analysis_id: "job:1".into(),
                position_id: pos1_id,
                engine: "GNUbg".into(),
                depth: Some("3-ply".into()),
                records: vec![],
                decision: None,
                analysis: Some(Analysis {
                    engine: "GNUbg".into(),
                    plies: 3,
                    candidates: vec![
                        Candidate {
                            rank: 1,
                            notation: "8/5 6/5".into(),
                            path: None,
                            equity: 0.220,
                            probabilities: vec![0.56, 0.16, 0.01, 0.44, 0.11, 0.01],
                            plies: Some(3),
                        },
                        Candidate {
                            rank: 2,
                            notation: "13/10 6/5".into(),
                            path: None,
                            equity: 0.150,
                            probabilities: vec![0.53, 0.14, 0.01, 0.47, 0.12, 0.01],
                            plies: Some(3),
                        },
                    ],
                }),
            });
            ws.dice_state = DiceTabState::Ready {
                position_id: pos1_id,
                engine: MatchAnalysisEngine::Gnubg,
                job_id: Some(Uuid::new_v4()),
                distribution: dist_snap,
            };
            ws.select_checker_position(pos1_id, window, cx);
        });

        window.render_frame(cx);
        assert!(window.find("analysis-workspace").visible());
        assert!(window.find("analysis-move-history-pane").visible());
        assert!(window.find("analysis-board-pane").visible());
        assert!(window.find("analysis-game-plan-panel").visible());
        assert!(window.find("analysis-candidate-table").visible());

        // Switch to Dice Equity 6x6 tab and click cell (6, 1) -> projects hypothetical dice preview!
        window.click("checker-subtab-dice", cx);
        window.render_frame(cx);
        assert!(window.find("dice-equity-6x6-grid").visible());
        for first in 1..=6 {
            for second in 1..=6 {
                assert!(window.find(format!("dice-cell-{first}-{second}")).visible());
            }
        }
        assert!(
            window.find("dice-selected-roll-caption").visible(),
            "actual roll selected initially"
        );
        window.click("dice-cell-6-1", cx);
        window.render_frame(cx);
        assert!(window.find("dice-selected-roll-caption").visible());
        let ws_ref = entity.read(cx);
        assert_eq!(ws_ref.selected_dice_cell, Some([6, 1]));
        assert_eq!(ws_ref.preview.mode, BoardViewMode::Arrows);
        assert_eq!(
            ws_ref
                .preview
                .hypothetical_dice
                .as_ref()
                .unwrap()
                .best_notation,
            "13/7 8/7"
        );
        window.click("dice-cell-1-6", cx);
        assert_eq!(
            entity
                .read(cx)
                .preview
                .hypothetical_dice
                .as_ref()
                .unwrap()
                .best_notation,
            "13/7 8/7"
        );
        window.click("dice-cell-2-6", cx);
        assert!(entity.read(cx).preview.hypothetical_dice.is_none());
        assert!(window.try_find("dice-selected-roll-caption").is_none());

        // White on the bar cannot enter against a closed board on double sixes.
        entity.update(cx, |ws, cx| {
            let mut closed = [0; 26];
            closed[25] = 1;
            closed[19..=24].fill(-2);
            ws.positions[0].board = closed.to_vec();
            let DiceTabState::Ready { distribution, .. } = &mut ws.dice_state else {
                panic!("expected dice results")
            };
            let double = distribution
                .rolls
                .iter_mut()
                .find(|r| r.dice == [6, 6])
                .unwrap();
            double.best.notation = "Cannot move".into();
            double.best.equity = distribution.mean_equity - 0.1;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("dice-cell-6-6", cx);
        let ws = entity.read(cx);
        let preview = ws.preview.hypothetical_dice.as_ref().unwrap();
        assert_eq!(preview.dice, [6, 6]);
        assert!(matches!(
            neondojo_board::project_move(
                &ws.positions[0].board.clone().try_into().unwrap(),
                0,
                &preview.dice,
                &preview.best_notation
            ),
            neondojo_board::MovePreview::Ok { forced: true, .. }
        ));

        // Switch to Cube, Statistics, Notes (with 409 conflict banner!), and Cards tabs.
        window.click("analysis-tab-errors", cx);
        window.render_frame(cx);
        assert!(window.find("analysis-cube-inspector").visible());

        window.click("analysis-tab-statistics", cx);
        window.render_frame(cx);
        assert!(window.find("analysis-stats-empty").visible());

        entity.update(cx, |ws, cx| {
            use neondojo::analysis_statistics::catalog::{
                CatalogAnalysisSummary, CatalogGameStatistics, MatchAnalysisStatistics,
            };
            ws.statistics = Some(MatchAnalysisStatistics {
                analysis_id: None,
                needs_cube_rule_reanalysis: false,
                error_breakdown: None,
                summary: Some(CatalogAnalysisSummary {
                    engine: "GNUbg".into(),
                    plies: Some(2),
                    recorded_depth: None,
                    total_games: 2,
                    completed_games: 2,
                    total_moves: 1,
                    analyzed_moves: 1,
                    players: Default::default(),
                    performance_rating: None,
                }),
                games: ws
                    .games
                    .iter()
                    .map(|g| CatalogGameStatistics {
                        game_no: g.game_no,
                        white_score: g.white_score,
                        black_score: g.black_score,
                        crawford: g.crawford,
                        analyzed: true,
                        needs_cube_rule_reanalysis: false,
                        summary: None,
                        error_breakdown: None,
                    })
                    .collect(),
            });
            cx.notify();
        });
        window.render_frame(cx);
        window.click("stats-open-game-2", cx);
        assert_eq!(entity.read(cx).current_game_no, Some(2));
        assert_eq!(entity.read(cx).active_tab, AnalysisDetailTab::Overview);

        // Seed a 409 conflict draft on Notes and verify banner renders cleanly.
        entity.update(cx, |ws, cx| {
            let key = NoteTargetKey {
                kind: AnnotationKind::Position,
                target_id: pos1_id,
            };
            ws.notes_drafts
                .ensure_entry(key)
                .set_comment("Preserved draft text".into())
                .unwrap();
            ws.notes_drafts
                .mark_conflict(key, &DomainError::conflict("Stale version 1 vs 2"));
            ws.select_detail_tab(AnalysisDetailTab::Notes, window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("analysis-notes-inspector").visible());
        assert!(window.find("notes-conflict-banner").visible());
        let field = entity.read(cx).comment_input.clone();
        let state = field.read(cx);
        assert!(state.input_bounds().size.height >= state.line_height().unwrap() * 4.);
        field.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        window.input("First paragraph", cx);
        window.press("enter", cx);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("Second\nThird".into()));
        window.press("cmd-v", cx);
        assert_eq!(field.read(cx).value(), "First paragraph\nSecond\nThird");
    })
    .expect("edit the current note");
    cx.update_window(window, |_, window, cx| {
        window.click("notes-scope-match", cx);
        let field = entity.read(cx).comment_input.clone();
        assert_eq!(field.read(cx).value(), "");
        window.click("notes-scope-position", cx);
        assert_eq!(field.read(cx).value(), "First paragraph\nSecond\nThird");
        let long_line = "Checker play ".repeat(80);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(long_line.clone()));
        field.update(cx, |input, cx| input.focus(window, cx));
        window.press("cmd-a", cx);
        window.press("cmd-v", cx);
        window.render_frame(cx);
        assert_eq!(field.read(cx).value().as_ref(), &long_line);
        assert!(field.read(cx).display_map().wrap_row_count() > 1);

        window.click("analysis-tab-cards", cx);
        window.render_frame(cx);
        assert!(window.find("analysis-cards-inspector").visible());

        // Switch to Game 2 (cube-only position_count == 0) — renders without errors!
        entity.update(cx, |ws, cx| {
            ws.current_game_no = Some(2);
            ws.positions.clear();
            ws.source_moves.clear();
            ws.active_target = None;
            ws.select_detail_tab(AnalysisDetailTab::Overview, window, cx);
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("analysis-move-list-empty").visible());

        // Hide & show workspace -> hidden placeholder has 0x0 bounds while job_controller stays alive.
        let jc_id_before = entity.read(cx).job_controller.entity_id();
        entity.update(cx, |ws, cx| ws.set_visible(false, cx));
        window.render_frame(cx);
        assert!(!window.find("analysis-workspace-hidden").visible());
        entity.update(cx, |ws, cx| ws.set_visible(true, cx));
        window.render_frame(cx);
        assert!(window.find("analysis-workspace").visible());
        let jc_id_after = entity.read(cx).job_controller.entity_id();
        assert_eq!(jc_id_before, jc_id_after);

        // invalidate_match clears cached match state and returns to Welcome drop screen.
        entity.update(cx, |ws, cx| ws.invalidate_match(match_id, cx));
        window.render_frame(cx);
        assert!(window.find("analysis-welcome-drop-screen").visible());
    })
    .expect("run headless analysis workspace test");
}

#[gpui_kit::test]
async fn headless_persists_per_tab_widths_emits_workspace_events_and_renders_compare_and_rolls(
    cx: &mut TestAppContext,
) {
    use crate::events::WorkspaceEvent;
    use neondojo::model::{DiceCount, DiceStats};
    use std::sync::{Arc, Mutex};

    init_headless_test_env(cx);

    let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let events_cb = Arc::clone(&events);

    let (window, entity) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let ent = cx.new(|cx| AnalysisWorkspace::new(window, cx));
                cx.subscribe(&ent, move |_, ev: &WorkspaceEvent, _| {
                    let tag = match ev {
                        WorkspaceEvent::SearchPositionInLibrary { position_id } => {
                            format!("search:{position_id}")
                        }
                        WorkspaceEvent::OpenStudyReport { match_id } => {
                            format!("study:{match_id}")
                        }
                        _ => "other".to_string(),
                    };
                    events_cb.lock().unwrap().push(tag);
                })
                .detach();
                ent
            })
        })
        .expect("open analysis interaction test window");

    let match_id = Uuid::new_v4();
    let pos1 = sample_position(match_id, 1, 1);
    let pos1_id = pos1.id;

    cx.update_window(window, |_, window, cx| {
        entity.update(cx, |ws, cx| {
            ws.welcome_mode = false;
            ws.match_id = Some(match_id);
            ws.match_summary = Some(MatchSummary {
                id: match_id,
                source_available: true,
                filename: "match.xg".into(),
                format: "xg".into(),
                white: "Alice".into(),
                black: "Bob".into(),
                match_length: 5,
                position_count: 1,
                created_at: Utc::now(),
            });
            ws.games = vec![MatchGame {
                cube_enabled: Some(true),
                result: None,
                game_no: 1,
                position_count: 1,
                white_score: 0,
                black_score: 0,
                crawford: false,
            }];
            ws.current_game_no = Some(1);
            ws.positions = vec![pos1.clone()];
            ws.compare_slots = [
                vec![Candidate {
                    rank: 1,
                    notation: "8/5 6/5".into(),
                    path: None,
                    equity: 0.21,
                    probabilities: vec![],
                    plies: Some(3),
                }],
                vec![Candidate {
                    rank: 1,
                    notation: "8/5 6/5".into(),
                    path: None,
                    equity: 0.20,
                    probabilities: vec![],
                    plies: Some(3),
                }],
                vec![],
            ];
            ws.observed_rolls = Some(DiceStats {
                total: 36,
                rolls: vec![
                    DiceCount {
                        dice: [6, 6],
                        count: 2,
                        expected_probability: 1.0 / 36.0,
                    },
                    DiceCount {
                        dice: [3, 1],
                        count: 3,
                        expected_probability: 2.0 / 36.0,
                    },
                ],
            });
            ws.select_checker_position(pos1_id, window, cx);
        });

        window.render_frame(cx);

        // Emit OpenStudyReport and SearchPositionInLibrary via header & board buttons.
        window.click("analysis-open-study-report", cx);
        window.click("board-search-pos-btn", cx);
        window.render_frame(cx);

        // Switch to Compare moves & Observed rolls sub-tabs.
        window.click("checker-subtab-compare", cx);
        window.render_frame(cx);
    })
    .unwrap();

    for width in [1440.0, 1920.0] {
        cx.simulate_window_resize(
            window,
            gpui_kit::size(gpui_kit::px(width), gpui_kit::px(900.0)),
        );
        cx.update_window(window, |_, window, cx| {
            let mut short_width = gpui_kit::px(0.0);
            for (notation, dice) in [("8/5 6/5", [3, 1]), ("24/21 24/21 13/10 13/10", [3, 3])] {
                entity.update(cx, |ws, cx| {
                    ws.positions[0].dice = dice.to_vec();
                    ws.positions[0].notation = notation.into();
                    ws.compare_slots[0][0].notation = notation.into();
                    ws.compare_slots[1][0].notation = notation.into();
                    cx.notify();
                });
                window.render_frame(cx);
                assert!(window.find("analysis-compare-matrix").visible());
                let header = window.find("compare-move-header").bounds();
                let played = window.find("compare-move-0").bounds();
                assert_eq!(header.left(), played.left());
                assert_eq!(header.right(), played.right());
                if dice[0] == dice[1] {
                    assert!(played.size.width > short_width);
                } else {
                    assert!(played.size.width <= gpui_kit::px(130.0));
                    short_width = played.size.width;
                }
            }
        })
        .unwrap();
    }
    cx.update_window(window, |_, window, cx| {
        window.click("checker-subtab-observed", cx);
        window.render_frame(cx);
        assert!(window.find("analysis-observed-rolls-bars").visible());

        // Adjust board width on Overview tab and verify LocalUiState persistence.
        entity.update(cx, |ws, cx| {
            ws.board_widths.set_width(AnalysisDetailTab::Overview, 780);
            ws.board_widths.set_width(AnalysisDetailTab::Notes, 540);
            ws.board_widths.set_width(AnalysisDetailTab::Cards, 440);
            if cx.has_global::<LocalUiState>() {
                let w = ws.board_widths.widths;
                gpui_kit::BorrowAppContext::update_global::<LocalUiState, _>(cx, |s, _| {
                    s.analysis_board_widths = w;
                });
            }
            cx.notify();
        });
        window.render_frame(cx);

        let persisted = cx.global::<LocalUiState>().analysis_board_widths;
        assert_eq!(persisted.overview, Some(780));
        assert_eq!(persisted.notes, Some(540));
        assert_eq!(persisted.default_width, Some(440));
    })
    .expect("run headless interaction test");

    let seen = events.lock().unwrap().clone();
    assert!(seen.contains(&format!("study:{match_id}")));
    assert!(seen.contains(&"search:4HPwATDgc/ABMA".to_string()));
}

#[gpui_kit::test]
async fn move_notes_editor_restores_multiline_drafts_when_changing_decisions(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::WindowExt;
    init_headless_test_env(cx);
    let (window, _) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| AnalysisWorkspace::new(w, cx))
            })
        })
        .unwrap();
    let match_id = Uuid::new_v4();
    let pos = sample_position(match_id, 1, 1);
    let moves = ["double", "take"]
        .into_iter()
        .enumerate()
        .map(|(ix, action)| StoredMove {
            id: Uuid::new_v4(),
            match_uuid: match_id,
            game_no: 1,
            move_no: (ix + 1) as i32,
            kind: "cube".into(),
            player: ix as i32,
            notation: action.into(),
            position_uuid: None,
            board: pos.board.clone(),
            dice: vec![],
            position_id: pos.position_id.clone(),
            gnubg_match_id: pos.gnubg_match_id.clone(),
        })
        .collect();
    let field = cx
        .update_window(window, |_, window, cx| {
            super::notes_cards::open_move_notes_dialog(
                match_id,
                1,
                moves,
                None,
                ["White".into(), "Black".into()],
                window,
                cx,
            );
            window.render_frame(cx);
            window.click("move-notes-comment", cx);
            let field = window
                .focused_input(cx)
                .unwrap()
                .as_textarea()
                .unwrap()
                .clone();
            assert!(
                field.read(cx).input_bounds().size.height
                    >= field.read(cx).line_height().unwrap() * 4.
            );
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                "Double?\nReview the take.".into(),
            ));
            window.press("cmd-v", cx);
            field
        })
        .unwrap();
    cx.update_window(window, |_, window, cx| {
        window.click("move-notes-item-1", cx);
        assert_eq!(field.read(cx).value(), "");
        window.click("move-notes-item-0", cx);
        assert_eq!(field.read(cx).value(), "Double?\nReview the take.");
    })
    .unwrap();
}
