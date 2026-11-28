//! Core Analysis workspace state models, pure state machines (`DiceTabState`,
//! `BoardWidthController`, `NotesDraftCache`, `CubeAutoSelectState`,
//! `MonotonicRemainingTimer`), interleaved move-history rows, and HRTB-safe
//! `BackendFacade` async helper.

use crate::local_state::AnalysisBoardWidths;
use gpui_kit::{App, ClickEvent, Window};
use neondojo::{
    analysis_catalog::AnalysisCatalogEntry,
    analysis_timing::{AnalysisRemainingTime, RemainingTimeState},
    annotations::{Annotation, AnnotationKind, AnnotationUpdate},
    api::MatchAnalysisEngine,
    domain_error::{DomainError, ErrorKind},
    engine::{Candidate, Distribution},
    facade::BackendFacade,
    model::{Job, MatchGame, MoveSeverity, Position},
    move_assessments::MoveAssessment,
    move_catalog::StoredMove,
    review_decisions::ReviewDecision,
};
use neondojo_board::default_analysis_board_width;
use neondojo_rules::options::analysis_board_width_bounds;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::rc::Rc;
use uuid::Uuid;

/// Shared GPUI UI callback aliases avoiding repetitive `Rc<dyn Fn(...)>` signatures.
pub type ClickCb = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
pub type ActionCb<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;
pub type Action2Cb<A, B> = Rc<dyn Fn(A, B, &mut Window, &mut App)>;
pub type Action3Cb<A, B, C> = Rc<dyn Fn(A, B, C, &mut Window, &mut App)>;

/// Maximum tags per annotation enforced by backend (`annotations::MAX_TAGS`).
pub const MAX_ANNOTATION_TAGS: usize = 30;
/// Maximum character length per tag.
pub const MAX_TAG_CHARS: usize = 64;
/// Maximum character length per comment.
pub const MAX_COMMENT_CHARS: usize = 20_000;
/// Minimum inspector width in pixels at which Statistics switches to 2 columns.
pub const STATS_TWO_COLUMN_MIN_WIDTH: f32 = 1050.0;
/// Left move-history pane fixed width in pixels.
pub const MOVE_LIST_PANE_WIDTH: f32 = 272.0;

/// Runs a `BackendFacade` future via `tokio::task::spawn_blocking` +
/// `Handle::block_on` to avoid cross-crate SQLx HRTB `Send` inference issues.
pub async fn run_facade<T, F, Fut>(f: F) -> Result<T, DomainError>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, DomainError>>,
{
    let handle = tokio::runtime::Handle::current();
    match tokio::task::spawn_blocking(move || handle.block_on(f())).await {
        Ok(res) => res,
        Err(err) => Err(DomainError::unavailable(format!(
            "Background task interrupted: {err}"
        ))),
    }
}

/// Top-level right-pane inspector tabs matching NeonDojo's 5 Analysis sub-tabs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalysisDetailTab {
    #[default]
    Overview,
    Cube,
    Statistics,
    Notes,
    Cards,
}

impl AnalysisDetailTab {
    pub const ALL: [Self; 5] = [
        Self::Overview,
        Self::Cube,
        Self::Statistics,
        Self::Notes,
        Self::Cards,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Cube => "errors",
            Self::Statistics => "statistics",
            Self::Notes => "notes",
            Self::Cards => "cards",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "overview" => Some(Self::Overview),
            "errors" | "cube" => Some(Self::Cube),
            "statistics" => Some(Self::Statistics),
            "notes" => Some(Self::Notes),
            "cards" => Some(Self::Cards),
            _ => None,
        }
    }

    /// Tabs available when `source_available` is false (retained saved position).
    pub const fn available_when_retained(self) -> bool {
        matches!(self, Self::Overview | Self::Notes | Self::Cards)
    }
}

/// Sub-tabs inside the Overview / Checker inspector panel.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckerSubTab {
    #[default]
    Candidates,
    Compare,
    Dice,
    Observed,
}

impl CheckerSubTab {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Candidates => "candidates",
            Self::Compare => "compare",
            Self::Dice => "dice",
            Self::Observed => "observed",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "candidates" => Some(Self::Candidates),
            "compare" => Some(Self::Compare),
            "dice" => Some(Self::Dice),
            "observed" => Some(Self::Observed),
            _ => None,
        }
    }
}

/// Move history severity filter in the left pane.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveHistoryFilter {
    #[default]
    All,
    Errors,
    Doubtful,
    Bad,
    Blunder,
    VeryBad,
}

impl MoveHistoryFilter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Errors,
        Self::Doubtful,
        Self::Bad,
        Self::Blunder,
        Self::VeryBad,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Errors => "errors",
            Self::Doubtful => "doubtful",
            Self::Bad => "bad",
            Self::Blunder => "blunder",
            Self::VeryBad => "very_bad",
        }
    }

    pub fn matches_severity(self, severity: Option<MoveSeverity>) -> bool {
        match self {
            Self::All => true,
            Self::Errors => matches!(
                severity,
                Some(
                    MoveSeverity::Doubtful
                        | MoveSeverity::Bad
                        | MoveSeverity::Blunder
                        | MoveSeverity::VeryBad
                )
            ),
            Self::Doubtful => severity == Some(MoveSeverity::Doubtful),
            Self::Bad => severity == Some(MoveSeverity::Bad),
            Self::Blunder => severity == Some(MoveSeverity::Blunder),
            Self::VeryBad => severity == Some(MoveSeverity::VeryBad),
        }
    }
}

/// Active board target in Analysis: either a checker position or an explicit/skipped cube decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveBoardTarget {
    Checker {
        position_id: Uuid,
    },
    Cube {
        review_id: String,
        decision_no: i32,
        /// Linked checker position to return to when clicking "Back to checker move".
        return_position_id: Option<Uuid>,
    },
}

/// One row in the interleaved `uniform_list` move history (cube actions + checker moves).
#[derive(Debug, Clone)]
pub enum InterleavedMoveRow {
    Checker {
        position: Position,
        assessment: Option<MoveAssessmentSnapshot>,
    },
    CubeAction {
        move_id: Option<Uuid>,
        game_no: i32,
        move_no: i32,
        player: i32,
        notation: String,
        review_id: Option<String>,
        decision_no: Option<i32>,
        severity: Option<MoveSeverity>,
        equity_loss: Option<f64>,
        evaluated: bool,
        return_position_id: Option<Uuid>,
    },
}

/// Lightweight cloneable assessment snapshot for `MoveHistoryRow`.
#[derive(Debug, Clone)]
pub struct MoveAssessmentSnapshot {
    pub severity: MoveSeverity,
    pub equity_loss: f64,
    pub best_move: Option<String>,
    pub imported: bool,
    pub engine: String,
    pub depth: Option<String>,
}

impl From<&MoveAssessment> for MoveAssessmentSnapshot {
    fn from(a: &MoveAssessment) -> Self {
        Self {
            severity: a.severity,
            equity_loss: a.equity_loss,
            best_move: a.best_move.clone(),
            imported: matches!(
                a.source,
                neondojo::move_assessments::AssessmentSource::Imported
            ),
            engine: a.engine.clone(),
            depth: a.depth.clone(),
        }
    }
}

/// Builds interleaved cube-action and checker rows for a single game, supporting
/// `position_count == 0` cube-only games cleanly.
pub fn build_interleaved_move_rows(
    positions: &[Position],
    source_moves: &[StoredMove],
    assessments: &HashMap<Uuid, MoveAssessmentSnapshot>,
    cube_reviews: &[&ReviewDecision],
    filter: MoveHistoryFilter,
) -> Vec<InterleavedMoveRow> {
    let mut rows = Vec::new();
    let pos_by_id: HashMap<Uuid, &Position> = positions.iter().map(|p| (p.id, p)).collect();

    let match_cube_review = |mv: &StoredMove| -> Option<&ReviewDecision> {
        cube_reviews.iter().copied().find(|r| {
            r.move_id == Some(mv.id)
                || (r.game_no == mv.game_no
                    && r.decision.kind == "cube"
                    && r.decision.player == mv.player
                    && r.decision.played.eq_ignore_ascii_case(&mv.notation)
                    && r.review_position
                        .as_ref()
                        .is_some_and(|rp| rp.position_id == mv.position_id))
        })
    };

    if !source_moves.is_empty() {
        let mut last_checker_id: Option<Uuid> = None;
        for mv in source_moves {
            if mv.kind == "checker" {
                if let Some(pid) = mv.position_uuid
                    && let Some(&pos) = pos_by_id.get(&pid)
                {
                    let assessment = assessments.get(&pid).cloned();
                    let sev = assessment.as_ref().map(|a| a.severity);
                    last_checker_id = Some(pid);
                    if filter.matches_severity(sev) {
                        rows.push(InterleavedMoveRow::Checker {
                            position: pos.clone(),
                            assessment,
                        });
                    }
                }
            } else if mv.kind == "cube" {
                let review = match_cube_review(mv);
                let sev = review.and_then(|r| parse_severity(&r.decision.severity));
                if filter.matches_severity(sev) {
                    rows.push(InterleavedMoveRow::CubeAction {
                        move_id: Some(mv.id),
                        game_no: mv.game_no,
                        move_no: mv.move_no,
                        player: mv.player,
                        notation: mv.notation.clone(),
                        review_id: review.map(|r| r.id.clone()),
                        decision_no: review.map(|r| r.decision.decision_no),
                        severity: sev,
                        equity_loss: review.map(|r| r.decision.equity_loss),
                        evaluated: review.is_some(),
                        return_position_id: last_checker_id,
                    });
                }
            }
        }
        return rows;
    }

    // Fallback when move catalog is still loading: render checker positions and any cube reviews.
    if positions.is_empty() {
        for review in cube_reviews {
            let sev = parse_severity(&review.decision.severity);
            if filter.matches_severity(sev) {
                rows.push(InterleavedMoveRow::CubeAction {
                    move_id: review.move_id,
                    game_no: review.game_no,
                    move_no: review.decision.decision_no,
                    player: review.decision.player,
                    notation: review.decision.played.clone(),
                    review_id: Some(review.id.clone()),
                    decision_no: Some(review.decision.decision_no),
                    severity: sev,
                    equity_loss: Some(review.decision.equity_loss),
                    evaluated: true,
                    return_position_id: None,
                });
            }
        }
        return rows;
    }

    for pos in positions {
        let assessment = assessments.get(&pos.id).cloned();
        let sev = assessment.as_ref().map(|a| a.severity);
        if filter.matches_severity(sev) {
            rows.push(InterleavedMoveRow::Checker {
                position: pos.clone(),
                assessment,
            });
        }
    }
    rows
}

/// Parses a serialized severity string into `MoveSeverity`.
pub fn parse_severity(raw: &str) -> Option<MoveSeverity> {
    match raw {
        "none" => Some(MoveSeverity::None),
        "doubtful" => Some(MoveSeverity::Doubtful),
        "bad" => Some(MoveSeverity::Bad),
        "blunder" => Some(MoveSeverity::Blunder),
        "very_bad" => Some(MoveSeverity::VeryBad),
        _ => None,
    }
}

/// Checks whether a game is a cube-only game (`position_count == 0`).
pub const fn is_cube_only_game(game: &MatchGame) -> bool {
    game.position_count == 0
}

// ── Dice Equity State Machine ────────────────────────────────────────────────

/// Explicit state machine for the Dice Equity 6×6 tab enforcing ALL invariants:
/// 1. Auto-starts a dice calculation for the active engine when none exists.
/// 2. NEVER auto-retry loops on error (`Failed { manual_retry_only: true }`).
/// 3. Explicit tab click or Retry button clears the error / delete block and retries.
/// 4. Explicit delete transitions to `DeletedAwaitingExplicitRequest`, staying
///    silent across renders/revisits until the user manually recalculates.
#[derive(Debug, Default, Clone)]
pub enum DiceTabState {
    #[default]
    Idle,
    /// User explicitly deleted the dice calculation for `(position_id, engine)`.
    /// Must stay silent and NOT auto-start until explicitly recalculated.
    DeletedAwaitingExplicitRequest {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    },
    /// Awaiting job creation response from `facade.create_position_job`.
    Submitting {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    },
    /// Active queued/running dice job tracked via `watch_job_events`.
    Running {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
        job_id: Uuid,
        progress: i32,
        status: String,
    },
    /// Completed 21-pair distribution ready for the 6×6 matrix.
    Ready {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
        job_id: Option<Uuid>,
        distribution: DiceDistributionSnapshot,
    },
    /// Failed or cancelled dice calculation. Never auto-retried; requires explicit
    /// user action (tab re-click or Retry button).
    Failed {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
        error: String,
        manual_retry_only: bool,
    },
}

/// Cloneable snapshot of `neondojo::engine::Distribution` for GPUI rendering.
#[derive(Debug, Clone)]
pub struct DiceDistributionSnapshot {
    pub engine: String,
    pub plies: i32,
    pub mean_equity: f64,
    pub rolls: Vec<DiceRollEntry>,
}

#[derive(Debug, Clone)]
pub struct DiceRollEntry {
    pub dice: [i32; 2],
    pub weight: i32,
    pub probability: f64,
    pub best: Candidate,
}

impl From<Distribution> for DiceDistributionSnapshot {
    fn from(dist: Distribution) -> Self {
        Self {
            engine: dist.engine,
            plies: dist.plies,
            mean_equity: dist.mean_equity,
            rolls: dist
                .rolls
                .into_iter()
                .map(|r| DiceRollEntry {
                    dice: r.dice,
                    weight: r.weight,
                    probability: r.probability,
                    best: r.best,
                })
                .collect(),
        }
    }
}

/// Matches both orderings of a dice combination.
pub(super) fn same_roll([a, b]: [i32; 2], [c, d]: [i32; 2]) -> bool {
    (a == c && b == d) || (a == d && b == c)
}

impl DiceDistributionSnapshot {
    /// Looks up the unordered pair `(max(a, b), min(a, b))` or `(min, max)` for cell `(row, col)`.
    pub fn find_roll(&self, d1: i32, d2: i32) -> Option<&DiceRollEntry> {
        self.rolls.iter().find(|r| same_roll(r.dice, [d1, d2]))
    }
}

/// Action returned by `DiceTabState` transitions telling the workspace whether
/// to submit a dice calculation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiceTransitionAction {
    None,
    StartCalculation {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    },
}

impl DiceTabState {
    /// Returns true if an auto-start should be triggered for `(position_id, engine)`
    /// when the Dice tab is visible and no stored completed dice job was found.
    pub fn should_auto_start(&self, position_id: Uuid, engine: MatchAnalysisEngine) -> bool {
        match self {
            Self::Idle => true,
            Self::DeletedAwaitingExplicitRequest {
                position_id: p,
                engine: e,
            } => *p != position_id || *e != engine,
            Self::Submitting {
                position_id: p,
                engine: e,
            }
            | Self::Running {
                position_id: p,
                engine: e,
                ..
            }
            | Self::Ready {
                position_id: p,
                engine: e,
                ..
            } => *p != position_id || *e != engine,
            Self::Failed {
                position_id: p,
                engine: e,
                manual_retry_only,
                ..
            } => {
                if *p == position_id && *e == engine && *manual_retry_only {
                    false
                } else {
                    *p != position_id || *e != engine
                }
            }
        }
    }

    /// Evaluates whether entering/staying on the Dice tab with no stored result
    /// should automatically start calculation.
    pub fn on_missing_result(
        &mut self,
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    ) -> DiceTransitionAction {
        if self.should_auto_start(position_id, engine) {
            *self = Self::Submitting {
                position_id,
                engine,
            };
            DiceTransitionAction::StartCalculation {
                position_id,
                engine,
            }
        } else {
            DiceTransitionAction::None
        }
    }

    /// Called when the user explicitly clicks the Dice tab while already in `Failed`,
    /// or clicks the manual Retry / Recalculate button (including after Delete).
    pub fn on_manual_retry(
        &mut self,
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    ) -> DiceTransitionAction {
        *self = Self::Submitting {
            position_id,
            engine,
        };
        DiceTransitionAction::StartCalculation {
            position_id,
            engine,
        }
    }

    /// Called when the user clicks the Dice tab header directly.
    /// Retries a failed calculation, but respects `DeletedAwaitingExplicitRequest`
    /// unless the user clicks the explicit "Calculate dice equity" button.
    pub fn on_tab_clicked(
        &mut self,
        position_id: Uuid,
        engine: MatchAnalysisEngine,
    ) -> DiceTransitionAction {
        match self {
            Self::Failed {
                position_id: p,
                engine: e,
                ..
            } if *p == position_id && *e == engine => self.on_manual_retry(position_id, engine),
            Self::DeletedAwaitingExplicitRequest {
                position_id: p,
                engine: e,
            } if *p == position_id && *e == engine => DiceTransitionAction::None,
            _ => self.on_missing_result(position_id, engine),
        }
    }

    /// Called when a stored or newly created job update arrives for `(position_id, engine)`.
    pub fn on_job_update(&mut self, position_id: Uuid, engine: MatchAnalysisEngine, job: &Job) {
        match job.status.as_str() {
            "completed" => {
                if let Some(result) = job.result.as_ref()
                    && let Ok(dist) = serde_json::from_value::<Distribution>(result.clone())
                {
                    *self = Self::Ready {
                        position_id,
                        engine,
                        job_id: Some(job.id),
                        distribution: dist.into(),
                    };
                } else {
                    *self = Self::Failed {
                        position_id,
                        engine,
                        error: "Completed dice job returned invalid payload".to_string(),
                        manual_retry_only: true,
                    };
                }
            }
            "failed" => {
                *self = Self::Failed {
                    position_id,
                    engine,
                    error: job
                        .error
                        .clone()
                        .unwrap_or_else(|| "Dice calculation failed".to_string()),
                    manual_retry_only: true,
                };
            }
            other => {
                *self = Self::Running {
                    position_id,
                    engine,
                    job_id: job.id,
                    progress: job.progress,
                    status: other.to_string(),
                };
            }
        }
    }

    /// Called when submitting or loading a dice calculation fails.
    pub fn on_failed(
        &mut self,
        position_id: Uuid,
        engine: MatchAnalysisEngine,
        error: impl Into<String>,
    ) {
        *self = Self::Failed {
            position_id,
            engine,
            error: error.into(),
            manual_retry_only: true,
        };
    }

    /// Called after the user explicitly deletes the stored dice job.
    pub fn on_deleted(&mut self, position_id: Uuid, engine: MatchAnalysisEngine) {
        *self = Self::DeletedAwaitingExplicitRequest {
            position_id,
            engine,
        };
    }

    /// Resets transient state when switching to a different position/engine,
    /// preserving `DeletedAwaitingExplicitRequest` only for the matching tuple.
    pub fn reset_for_position(&mut self, position_id: Uuid, engine: MatchAnalysisEngine) {
        let keep = match self {
            Self::DeletedAwaitingExplicitRequest {
                position_id: p,
                engine: e,
            }
            | Self::Ready {
                position_id: p,
                engine: e,
                ..
            }
            | Self::Running {
                position_id: p,
                engine: e,
                ..
            }
            | Self::Failed {
                position_id: p,
                engine: e,
                ..
            } => *p == position_id && *e == engine,
            Self::Idle | Self::Submitting { .. } => false,
        };
        if !keep {
            *self = Self::Idle;
        }
    }

    pub fn active_job_id(&self) -> Option<Uuid> {
        match self {
            Self::Running { job_id, .. } => Some(*job_id),
            Self::Ready { job_id, .. } => *job_id,
            _ => None,
        }
    }
}

// ── 5-Tab Board Width Controller ─────────────────────────────────────────────

/// Manages 5 independent per-tab Analysis board widths persisted in
/// `LocalUiState::analysis_board_widths`, clamped strictly to `320..=1680 px`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BoardWidthController {
    pub widths: AnalysisBoardWidths,
    /// Width snapshot captured when keyboard focus enters the splitter so Escape can revert.
    pre_edit_width: Option<Option<u32>>,
}

impl BoardWidthController {
    pub fn from_persisted(mut widths: AnalysisBoardWidths) -> Self {
        widths.clamp_all();
        Self {
            widths,
            pre_edit_width: None,
        }
    }

    pub fn bounds() -> (u32, u32) {
        let (min_w, max_w) = analysis_board_width_bounds();
        (min_w.max(0) as u32, max_w.max(min_w) as u32)
    }

    pub fn clamp(width: i32) -> u32 {
        let (min_w, max_w) = Self::bounds();
        width.clamp(min_w as i32, max_w as i32) as u32
    }

    pub fn stored_for_tab(&self, tab: AnalysisDetailTab) -> Option<u32> {
        match tab {
            AnalysisDetailTab::Overview => self.widths.overview,
            AnalysisDetailTab::Cube => self.widths.errors,
            AnalysisDetailTab::Statistics => self.widths.statistics,
            AnalysisDetailTab::Notes => self.widths.notes,
            AnalysisDetailTab::Cards => self.widths.default_width,
        }
    }

    fn slot_mut(&mut self, tab: AnalysisDetailTab) -> &mut Option<u32> {
        match tab {
            AnalysisDetailTab::Overview => &mut self.widths.overview,
            AnalysisDetailTab::Cube => &mut self.widths.errors,
            AnalysisDetailTab::Statistics => &mut self.widths.statistics,
            AnalysisDetailTab::Notes => &mut self.widths.notes,
            AnalysisDetailTab::Cards => &mut self.widths.default_width,
        }
    }

    /// Resolves the concrete board width in pixels for `tab`, using the stored
    /// per-tab override or falling back to NeonDojo's responsive default formula.
    pub fn effective_width(&self, tab: AnalysisDetailTab, viewport_width: f32) -> f32 {
        let (min_w, max_w) = Self::bounds();
        let max_avail = (viewport_width - MOVE_LIST_PANE_WIDTH - 340.0).max(min_w as f32);
        if let Some(saved) = self.stored_for_tab(tab) {
            (saved as f32).clamp(min_w as f32, max_w as f32)
        } else {
            default_analysis_board_width(viewport_width, max_avail)
        }
    }

    pub fn set_width(&mut self, tab: AnalysisDetailTab, width: i32) -> u32 {
        let clamped = Self::clamp(width);
        *self.slot_mut(tab) = Some(clamped);
        clamped
    }

    pub fn adjust(&mut self, tab: AnalysisDetailTab, delta: i32, viewport_width: f32) -> u32 {
        if self.pre_edit_width.is_none() {
            self.pre_edit_width = Some(self.stored_for_tab(tab));
        }
        let current = self.effective_width(tab, viewport_width).round() as i32;
        self.set_width(tab, current + delta)
    }

    pub fn set_min(&mut self, tab: AnalysisDetailTab) -> u32 {
        if self.pre_edit_width.is_none() {
            self.pre_edit_width = Some(self.stored_for_tab(tab));
        }
        let (min_w, _) = Self::bounds();
        self.set_width(tab, min_w as i32)
    }

    pub fn set_max(&mut self, tab: AnalysisDetailTab) -> u32 {
        if self.pre_edit_width.is_none() {
            self.pre_edit_width = Some(self.stored_for_tab(tab));
        }
        let (_, max_w) = Self::bounds();
        self.set_width(tab, max_w as i32)
    }

    pub fn reset_tab(&mut self, tab: AnalysisDetailTab) {
        *self.slot_mut(tab) = None;
        self.pre_edit_width = None;
    }

    pub fn reset_all(&mut self) {
        self.widths = AnalysisBoardWidths::default();
        self.pre_edit_width = None;
    }

    pub fn begin_edit(&mut self, tab: AnalysisDetailTab) {
        self.pre_edit_width = Some(self.stored_for_tab(tab));
    }

    pub fn cancel_edit(&mut self, tab: AnalysisDetailTab) -> bool {
        if let Some(prev) = self.pre_edit_width.take() {
            *self.slot_mut(tab) = prev;
            true
        } else {
            false
        }
    }

    pub fn commit_edit(&mut self) {
        self.pre_edit_width = None;
    }
}

// ── Persistent Notes Draft Cache & 409 Conflict Preservation ─────────────────

/// Key identifying a unique annotation target (`Match`, `Game`, `Position`, or `Move`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteTargetKey {
    pub kind: AnnotationKind,
    pub target_id: Uuid,
}

impl NoteTargetKey {
    const fn kind_rank(kind: AnnotationKind) -> u8 {
        match kind {
            AnnotationKind::Match => 0,
            AnnotationKind::Game => 1,
            AnnotationKind::Position => 2,
            AnnotationKind::Move => 3,
        }
    }
}

impl PartialOrd for NoteTargetKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NoteTargetKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Self::kind_rank(self.kind)
            .cmp(&Self::kind_rank(other.kind))
            .then_with(|| self.target_id.cmp(&other.target_id))
    }
}

impl std::hash::Hash for NoteTargetKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Self::kind_rank(self.kind).hash(state);
        self.target_id.hash(state);
    }
}

/// Persisted in-memory draft for an annotation target so switching games,
/// positions, or note scopes never loses unsaved edits or 409 conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    pub tags: Vec<String>,
    pub comment: String,
    pub base_version: i64,
    pub dirty: bool,
    pub conflict_error: Option<String>,
    pub server_latest: Option<ServerNoteSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNoteSnapshot {
    pub tags: Vec<String>,
    pub comment: String,
    pub version: i64,
}

impl NoteDraft {
    pub fn from_annotation(ann: &Annotation) -> Self {
        Self {
            tags: ann.tags.clone(),
            comment: ann.comment.clone(),
            base_version: ann.version,
            dirty: false,
            conflict_error: None,
            server_latest: None,
        }
    }

    pub fn add_tag(&mut self, raw: &str) -> Result<(), String> {
        let normalized = raw.trim().to_string();
        if normalized.is_empty() {
            return Err("Tag cannot be empty".to_string());
        }
        if normalized.chars().count() > MAX_TAG_CHARS {
            return Err(format!("Tag exceeds {MAX_TAG_CHARS} characters"));
        }
        if self
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&normalized))
        {
            return Ok(());
        }
        if self.tags.len() >= MAX_ANNOTATION_TAGS {
            return Err(format!("At most {MAX_ANNOTATION_TAGS} tags allowed"));
        }
        self.tags.push(normalized);
        self.dirty = true;
        Ok(())
    }

    pub fn remove_tag(&mut self, tag: &str) {
        let before = self.tags.len();
        self.tags.retain(|t| t != tag);
        if self.tags.len() != before {
            self.dirty = true;
        }
    }

    pub fn set_comment(&mut self, comment: String) -> Result<(), String> {
        if comment.chars().count() > MAX_COMMENT_CHARS {
            return Err(format!("Comment exceeds {MAX_COMMENT_CHARS} characters"));
        }
        if self.comment != comment {
            self.comment = comment;
            self.dirty = true;
        }
        Ok(())
    }
}

/// Return the submitted text and tags along with the server's normalized result.
pub(super) async fn save_note(
    facade: BackendFacade,
    key: NoteTargetKey,
    submitted: AnnotationUpdate,
) -> Result<(Annotation, AnnotationUpdate), DomainError> {
    run_facade(move || async move {
        let saved = facade
            .save_annotations(
                key.kind,
                key.target_id,
                AnnotationUpdate {
                    tags: submitted.tags.clone(),
                    comment: submitted.comment.clone(),
                    expected_version: submitted.expected_version,
                },
            )
            .await?;
        Ok((saved, submitted))
    })
    .await
}

/// Workspace-scoped cache keeping drafts alive across game/position/tab navigation.
#[derive(Debug, Default, Clone)]
pub struct NotesDraftCache {
    drafts: BTreeMap<NoteTargetKey, NoteDraft>,
}

impl NotesDraftCache {
    pub fn get(&self, key: NoteTargetKey) -> Option<&NoteDraft> {
        self.drafts.get(&key)
    }

    pub fn get_mut(&mut self, key: NoteTargetKey) -> Option<&mut NoteDraft> {
        self.drafts.get_mut(&key)
    }

    pub fn ensure_entry(&mut self, key: NoteTargetKey) -> &mut NoteDraft {
        self.drafts.entry(key).or_insert_with(|| NoteDraft {
            tags: Vec::new(),
            comment: String::new(),
            base_version: 0,
            dirty: false,
            conflict_error: None,
            server_latest: None,
        })
    }

    /// Merges a freshly loaded server annotation: if an unsaved dirty draft
    /// already exists for `key`, the user's draft is preserved untouched!
    pub fn merge_loaded(&mut self, ann: &Annotation) -> &NoteDraft {
        let key = NoteTargetKey {
            kind: ann.target_kind,
            target_id: ann.target_id,
        };
        let should_replace = self
            .drafts
            .get(&key)
            .is_none_or(|d| !d.dirty && ann.version >= d.base_version);
        if should_replace {
            self.drafts.insert(key, NoteDraft::from_annotation(ann));
        } else if let Some(draft) = self.drafts.get_mut(&key) {
            // Track latest server snapshot if it drifted while user had local edits.
            if ann.version > draft.base_version {
                draft.server_latest = Some(ServerNoteSnapshot {
                    tags: ann.tags.clone(),
                    comment: ann.comment.clone(),
                    version: ann.version,
                });
            }
        }
        self.drafts.get(&key).expect("entry just inserted")
    }

    /// Keep edits made while the write was in flight, using the returned version for the next save.
    pub fn mark_saved(&mut self, ann: &Annotation, submitted: &AnnotationUpdate) {
        let key = NoteTargetKey {
            kind: ann.target_kind,
            target_id: ann.target_id,
        };
        let Some(draft) = self.drafts.get_mut(&key) else {
            return;
        };
        let changed_comment = draft.comment != submitted.comment;
        let changed_tags = draft.tags != submitted.tags;
        if !changed_comment {
            draft.comment.clone_from(&ann.comment);
        }
        if !changed_tags {
            draft.tags.clone_from(&ann.tags);
        }
        draft.dirty = changed_comment || changed_tags;
        draft.base_version = ann.version;
        draft.server_latest = None;
        draft.conflict_error = None;
    }

    /// Records an HTTP 409 `ErrorKind::Conflict` while preserving the user's unsaved draft.
    pub fn mark_conflict(&mut self, key: NoteTargetKey, error: &DomainError) -> bool {
        if error.kind != ErrorKind::Conflict {
            return false;
        }
        let draft = self.ensure_entry(key);
        draft.dirty = true;
        draft.conflict_error = Some(error.to_string());
        true
    }

    /// Adopts the latest server revision after a 409 conflict ("Load latest").
    pub fn accept_server_latest(&mut self, key: NoteTargetKey, latest: &Annotation) {
        self.drafts.insert(key, NoteDraft::from_annotation(latest));
    }

    /// Adopts the latest server version number while keeping the user's edited text/tags
    /// so the next Save overwrites cleanly with the updated `expected_version`.
    pub fn rebase_draft_version(&mut self, key: NoteTargetKey, latest_version: i64) {
        if let Some(draft) = self.drafts.get_mut(&key) {
            draft.base_version = latest_version;
            draft.conflict_error = None;
        }
    }

    pub fn has_dirty(&self, key: NoteTargetKey) -> bool {
        self.drafts.get(&key).is_some_and(|d| d.dirty)
    }

    pub fn clear_for_match(&mut self) {
        self.drafts.clear();
    }
}

// ── Cube Auto-Select Once-Per-Tab-Visit State ────────────────────────────────

/// Tracks whether the Cube tab has already auto-selected the next cube decision
/// during the current visit so manual clicks or scrolling aren't hijacked.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CubeAutoSelectState {
    auto_selected_this_visit: bool,
}

impl CubeAutoSelectState {
    pub fn on_enter_cube_tab(&mut self) {
        self.auto_selected_this_visit = false;
    }

    pub fn on_leave_cube_tab(&mut self) {
        self.auto_selected_this_visit = false;
    }

    /// Returns the first cube decision ID to auto-select once per visit if none
    /// is currently selected.
    pub fn next_auto_selection<'a>(
        &mut self,
        current_selected_review_id: Option<&str>,
        decisions: &'a [ReviewDecision],
        current_game_no: Option<i32>,
    ) -> Option<&'a ReviewDecision> {
        if self.auto_selected_this_visit {
            return None;
        }
        if let Some(sel) = current_selected_review_id
            && decisions.iter().any(|d| d.id == sel)
        {
            self.auto_selected_this_visit = true;
            return None;
        }
        let candidate = current_game_no
            .and_then(|g| decisions.iter().find(|d| d.game_no >= g))
            .or_else(|| decisions.first());
        if candidate.is_some() {
            self.auto_selected_this_visit = true;
        }
        candidate
    }
}

// ── Monotonic 1s Remaining-Time Estimator ────────────────────────────────────

/// Cloneable timer phase mirroring `neondojo::analysis_timing::RemainingTimeState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerPhase {
    Queued,
    Waiting,
    Estimated,
    Reestimating,
    Unavailable,
    Finishing,
    Completed,
    Failed,
}

impl From<&RemainingTimeState> for TimerPhase {
    fn from(state: &RemainingTimeState) -> Self {
        match state {
            RemainingTimeState::Queued => Self::Queued,
            RemainingTimeState::Waiting => Self::Waiting,
            RemainingTimeState::Estimated => Self::Estimated,
            RemainingTimeState::Reestimating => Self::Reestimating,
            RemainingTimeState::Unavailable => Self::Unavailable,
            RemainingTimeState::Finishing => Self::Finishing,
            RemainingTimeState::Completed => Self::Completed,
            RemainingTimeState::Failed => Self::Failed,
        }
    }
}

/// Tracks a running match-analysis job's remaining time smoothly at 1-second
/// intervals without jumping upward between server polls (unless a new attempt
/// starts or state leaves `Estimated`).
#[derive(Debug, Default, Clone)]
pub struct MonotonicRemainingTimer {
    pub job_id: Option<Uuid>,
    pub attempt: i32,
    pub state: Option<TimerPhase>,
    pub display_remaining_ms: Option<f64>,
    pub basis_observed: bool,
    pub completed_moves: i64,
    pub total_moves: i64,
}

impl MonotonicRemainingTimer {
    pub fn sync_from_server(&mut self, sample: &AnalysisRemainingTime) {
        let same_run = self.job_id == Some(sample.job_id) && self.attempt == sample.attempt;
        self.job_id = Some(sample.job_id);
        self.attempt = sample.attempt;
        self.state = Some(TimerPhase::from(&sample.state));
        self.basis_observed = matches!(
            sample.basis,
            Some(neondojo::analysis_timing::RemainingTimeBasis::Observed)
        );
        self.completed_moves = sample.completed_moves;
        self.total_moves = sample.total_moves;

        match sample.remaining_ms {
            Some(server_ms) if server_ms.is_finite() && server_ms > 0.0 => {
                if same_run && let Some(prev) = self.display_remaining_ms {
                    // Enforce monotonic non-increasing countdown within the same attempt
                    // unless server estimate jumped dramatically (>15s) due to rate shift.
                    if server_ms > prev && (server_ms - prev) < 15_000.0 {
                        self.display_remaining_ms = Some(prev);
                    } else {
                        self.display_remaining_ms = Some(server_ms);
                    }
                } else {
                    self.display_remaining_ms = Some(server_ms);
                }
            }
            _ => {
                self.display_remaining_ms = None;
            }
        }
    }

    /// Decrements the displayed countdown by 1 second when in `Estimated` state.
    pub fn tick_one_second(&mut self) {
        if matches!(self.state, Some(TimerPhase::Estimated))
            && let Some(ms) = self.display_remaining_ms
        {
            self.display_remaining_ms = Some((ms - 1_000.0).max(0.0));
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

// ── 3-Engine Candidate Comparison Model ──────────────────────────────────────

/// Slot index in the 3-engine comparison matrix: XG (imported), GNUbg, NeonGammon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompareEngineSlot {
    Xg,
    Gnubg,
    NeonGammon,
}

impl CompareEngineSlot {
    pub const ALL: [Self; 3] = [Self::Xg, Self::Gnubg, Self::NeonGammon];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Xg => "XG / Imported",
            Self::Gnubg => "GNUbg",
            Self::NeonGammon => "NeonGammon",
        }
    }
}

/// One candidate cell inside a 3-engine comparison row.
#[derive(Debug, Clone)]
pub struct CompareSlotValue {
    pub rank: i32,
    pub notation: String,
    pub equity: f64,
    pub loss: f64,
    pub plies: Option<i32>,
}

/// One canonical move aligned across up to 3 engines by resulting board state.
#[derive(Debug, Clone)]
pub struct CandidateCompareRow {
    pub canonical_key: String,
    pub display_notation: String,
    pub played: bool,
    pub slots: [Option<CompareSlotValue>; 3],
    /// Maximum pairwise equity/loss spread `|Δ|` across present engines.
    pub max_delta: f64,
}

/// Computes canonical move alignment across up to 3 candidate lists by resulting board.
pub fn build_candidate_compare_rows(
    position: &Position,
    slot_candidates: [&[Candidate]; 3],
    sort_by_delta: bool,
) -> Vec<CandidateCompareRow> {
    let board_arr: Option<[i32; 26]> = position.board.as_slice().try_into().ok();
    let played_key = board_arr
        .as_ref()
        .and_then(|b| project_board_key(b, position.player, &position.dice, &position.notation))
        .unwrap_or_else(|| position.notation.trim().to_lowercase());

    let mut by_key: BTreeMap<String, CandidateCompareRow> = BTreeMap::new();

    for (slot_idx, list) in slot_candidates.iter().enumerate() {
        let best_eq = list
            .iter()
            .find(|c| c.rank == 1)
            .or_else(|| list.first())
            .map(|c| c.equity)
            .unwrap_or(0.0);

        for cand in *list {
            let key = board_arr
                .as_ref()
                .and_then(|b| project_board_key(b, position.player, &position.dice, &cand.notation))
                .unwrap_or_else(|| cand.notation.trim().to_lowercase());
            let loss = (best_eq - cand.equity).max(0.0);
            let entry = by_key
                .entry(key.clone())
                .or_insert_with(|| CandidateCompareRow {
                    canonical_key: key.clone(),
                    display_notation: cand.notation.clone(),
                    played: key == played_key,
                    slots: [None, None, None],
                    max_delta: 0.0,
                });
            if entry.slots[slot_idx].is_none() {
                entry.slots[slot_idx] = Some(CompareSlotValue {
                    rank: cand.rank,
                    notation: cand.notation.clone(),
                    equity: cand.equity,
                    loss,
                    plies: cand.plies,
                });
            }
        }
    }

    let mut rows: Vec<CandidateCompareRow> = by_key
        .into_values()
        .map(|mut row| {
            let losses: Vec<f64> = row.slots.iter().flatten().map(|s| s.loss).collect();
            if losses.len() >= 2 {
                let min_l = losses.iter().copied().fold(f64::INFINITY, f64::min);
                let max_l = losses.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                row.max_delta = (max_l - min_l).abs();
            }
            row
        })
        .collect();

    if sort_by_delta {
        rows.sort_by(|a, b| {
            b.max_delta
                .total_cmp(&a.max_delta)
                .then_with(|| best_rank_of(a).cmp(&best_rank_of(b)))
        });
    } else {
        rows.sort_by(|a, b| {
            best_rank_of(a)
                .cmp(&best_rank_of(b))
                .then_with(|| b.max_delta.total_cmp(&a.max_delta))
        });
    }
    rows
}

fn best_rank_of(row: &CandidateCompareRow) -> i32 {
    row.slots
        .iter()
        .flatten()
        .map(|s| s.rank)
        .min()
        .unwrap_or(i32::MAX)
}

fn project_board_key(
    board: &[i32; 26],
    player: i32,
    dice: &[i32],
    notation: &str,
) -> Option<String> {
    let player_u8 = u8::try_from(player).ok()?;
    let dice_u8: Vec<u8> = dice.iter().filter_map(|&d| u8::try_from(d).ok()).collect();
    match neondojo_board::project_move(board, player_u8, &dice_u8, notation) {
        neondojo_board::MovePreview::Ok { board: after, .. } => Some(
            after
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        neondojo_board::MovePreview::Unavailable(_) => None,
    }
}

/// Maps an analysis catalog entry's engine string to `MatchAnalysisEngine` without silent fallback.
pub fn catalog_entry_engine(entry: &AnalysisCatalogEntry) -> Option<MatchAnalysisEngine> {
    let lower = entry.engine.to_ascii_lowercase();
    if lower.contains("neongammon") || lower.contains("sage") || lower == "bgsage" {
        Some(MatchAnalysisEngine::Bgsage)
    } else if lower.contains("gnu") || lower == "gnubg" {
        Some(MatchAnalysisEngine::Gnubg)
    } else {
        None
    }
}

/// Formats a compact human-readable duration (e.g. `1m 24s`, `42s`, `< 1s`).
pub fn format_eta_ms(ms: f64) -> String {
    if !ms.is_finite() || ms < 0.0 {
        return "—".to_string();
    }
    let total_secs = (ms / 1_000.0).ceil() as u64;
    if total_secs == 0 {
        return "< 1s".to_string();
    }
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}h {mins}m")
    } else if mins > 0 {
        format!("{mins}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Filter criteria for the Cube tab decision list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubeFilterState {
    pub errors_only: bool,
    pub player: Option<i32>,
    pub severity: Option<MoveSeverity>,
    pub game_no: Option<i32>,
}

impl Default for CubeFilterState {
    fn default() -> Self {
        Self {
            errors_only: true,
            player: None,
            severity: None,
            game_no: None,
        }
    }
}

impl CubeFilterState {
    pub fn reset(&mut self) {
        *self = Self {
            errors_only: false,
            player: None,
            severity: None,
            game_no: None,
        };
    }

    pub fn has_active_filters(&self) -> bool {
        self.errors_only
            || self.player.is_some()
            || self.severity.is_some()
            || self.game_no.is_some()
    }
}
