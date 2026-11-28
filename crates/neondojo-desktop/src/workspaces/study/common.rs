//! Shared domain helpers, interactive checker-turn controller, selection cap,
//! formatting, and Tokio/SQLx-safe facade dispatch for the Study workspace.

use crate::local_state::LocalUiState;
use crate::review::cube_details::{CubeEquityUnit, CubeEvaluationData};
use crate::tokio_bridge::{ReadTask, TokioBridge};
use gpui_kit::{App, BorrowAppContext, Task, Window};
use std::collections::btree_map::Entry;
use std::rc::Rc;

/// Zero-argument GPUI UI callback alias.
pub type UiAction = Rc<dyn Fn(&mut Window, &mut App)>;
/// Single-argument GPUI UI callback alias.
pub type UiCallback<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;
/// Two-argument GPUI UI callback alias.
pub type UiCallback2<A, B> = Rc<dyn Fn(A, B, &mut Window, &mut App)>;
/// Three-argument GPUI UI callback alias.
pub type UiCallback3<A, B, C> = Rc<dyn Fn(A, B, C, &mut Window, &mut App)>;
/// Four-argument GPUI UI callback alias.
pub type UiCallback4<A, B, C, D> = Rc<dyn Fn(A, B, C, D, &mut Window, &mut App)>;
/// Five-argument GPUI UI callback alias.
pub type UiCallback5<A, B, C, D, E> = Rc<dyn Fn(A, B, C, D, E, &mut Window, &mut App)>;
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::learning::{CardBrowseItem, CardKind, Rating};
use neondojo::match_analysis::{CubeEquityUnit as BackendCubeUnit, CubeEvaluation};
use neondojo::match_assessment::TopicName;
use neondojo::model::Position;
use neondojo::play::{CheckerTurn, PlayMode, PlayStep};
use neondojo::practice::evaluation::PracticeVerdict;
use neondojo::recurring_mistakes::MistakeKind;
use neondojo_board::{
    BoardDiceInteraction, BoardInteractionState, BoardPosition, DieUsage, MovePreview,
    apply_absolute_steps, project_move,
};
use neondojo_i18n::{I18n, Locale, Param, format_int};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use tokio::task::JoinError;
use uuid::Uuid;

/// Sentinel deck ID representing all cards across the collection.
pub const ALL_CARDS_DECK: &str = "__all__";
/// Sentinel deck ID representing cards without any deck membership.
pub const WITHOUT_DECK_ID: &str = "__none__";
/// Cards fetched per browser page.
pub const CARDS_PER_PAGE: i64 = 50;
/// Strict cross-page multi-select cap matching backend `MAX_SELECTION = 200`.
pub const CARD_SELECTION_LIMIT: usize = 200;
/// Maximum daily new-card allowance matching `backend/src/learning/daily.rs`.
pub const MAX_NEW_CARDS_PER_DAY: i32 = 1000;
/// Default card preview pane width in logical pixels.
pub const DEFAULT_CARD_PREVIEW_WIDTH: u32 = 440;
/// Allowed bounds for the resizable card preview pane.
pub const MIN_CARD_PREVIEW_WIDTH: u32 = 240;
pub const MAX_CARD_PREVIEW_WIDTH: u32 = 1680;
/// Deck count threshold above which the deck filter input is displayed.
pub const DECK_FILTER_THRESHOLD: usize = 12;

/// The 5 canonical sub-tabs inside the Study workspace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StudyTab {
    #[default]
    Overview,
    Flashcards,
    Reports,
    Mistakes,
    History,
}

impl StudyTab {
    pub const ALL: &'static [Self] = &[
        Self::Overview,
        Self::Flashcards,
        Self::Reports,
        Self::Mistakes,
        Self::History,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Flashcards => "flashcards",
            Self::Reports => "reports",
            Self::Mistakes => "mistakes",
            Self::History => "history",
        }
    }

    pub const fn element_id(self) -> &'static str {
        match self {
            Self::Overview => "study-tab-overview",
            Self::Flashcards => "study-tab-flashcards",
            Self::Reports => "study-tab-reports",
            Self::Mistakes => "study-tab-mistakes",
            Self::History => "study-tab-history",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.id() == id)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Overview => I18n::t(locale, "study-overview", "tab"),
            Self::Flashcards => I18n::t(locale, "study", "cards"),
            Self::Reports => I18n::t(locale, "study", "reports"),
            Self::Mistakes => I18n::t(locale, "study", "mistakes"),
            Self::History => match locale {
                Locale::De => "Übungsverlauf",
                Locale::En => "Practice History",
            },
        }
    }
}

/// Clamps `new_cards_per_day` strictly into the backend-enforced `0..=1000` range.
pub fn clamp_new_cards_per_day(value: i32) -> i32 {
    value.clamp(0, MAX_NEW_CARDS_PER_DAY)
}

/// Parses user input for `new_cards_per_day`, clamping values into `0..=1000`.
pub fn parse_and_clamp_new_cards_per_day(raw: &str) -> Option<i32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: i32 = trimmed.parse().ok()?;
    Some(clamp_new_cards_per_day(parsed))
}

/// Clamps the resizable card preview pane width into `240..=1680` px.
pub fn clamp_preview_width(width: u32) -> u32 {
    width.clamp(MIN_CARD_PREVIEW_WIDTH, MAX_CARD_PREVIEW_WIDTH)
}

/// Metadata retained per selected card across paginated browser pages so batch
/// delete and tag mutations supply optimistic-concurrency versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCardMeta {
    pub id: Uuid,
    pub version: i64,
    pub tags_version: i64,
    pub deck_ids: Vec<Uuid>,
}

/// Cross-page multi-select state enforcing the strict 200-card backend cap.
#[derive(Debug, Default, Clone)]
pub struct CardSelectionSet {
    items: BTreeMap<Uuid, SelectedCardMeta>,
    cap_hit: bool,
}

impl CardSelectionSet {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, id: Uuid) -> bool {
        self.items.contains_key(&id)
    }

    pub fn cap_hit(&self) -> bool {
        self.cap_hit
    }

    pub fn clear_cap_warning(&mut self) {
        self.cap_hit = false;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.cap_hit = false;
    }

    /// Toggles selection of a card, returning `true` if now selected.
    /// Enforces [`CARD_SELECTION_LIMIT`] (`200`).
    pub fn toggle(&mut self, meta: SelectedCardMeta) -> bool {
        if self.items.remove(&meta.id).is_some() {
            self.cap_hit = false;
            return false;
        }
        if self.items.len() >= CARD_SELECTION_LIMIT {
            self.cap_hit = true;
            return false;
        }
        self.items.insert(meta.id, meta);
        self.cap_hit = false;
        true
    }

    /// Selects every card on the current page up to the 200-card cap.
    /// Returns how many additional cards were added.
    pub fn select_batch(&mut self, batch: impl IntoIterator<Item = SelectedCardMeta>) -> usize {
        let mut added = 0;
        for meta in batch {
            let at_cap = self.items.len() >= CARD_SELECTION_LIMIT;
            match self.items.entry(meta.id) {
                Entry::Occupied(mut occupied) => {
                    occupied.insert(meta);
                }
                Entry::Vacant(vacant) => {
                    if at_cap {
                        self.cap_hit = true;
                        break;
                    }
                    vacant.insert(meta);
                    added += 1;
                }
            }
        }
        added
    }

    /// Deselects a list of card IDs (for example all rows on the current page).
    pub fn deselect_ids(&mut self, ids: impl IntoIterator<Item = Uuid>) {
        for id in ids {
            self.items.remove(&id);
        }
        if self.items.len() < CARD_SELECTION_LIMIT {
            self.cap_hit = false;
        }
    }

    /// Updates retained versions/decks for cards visible on the freshly loaded page.
    pub fn refresh_from_page(&mut self, page_items: &[CardBrowseItem]) {
        for item in page_items {
            if let Some(entry) = self.items.get_mut(&item.card.id) {
                entry.version = item.card.version;
                entry.tags_version = item.browse.tags_version;
                entry.deck_ids = item.browse.decks.iter().map(|d| d.id).collect();
            }
        }
    }

    pub fn ids(&self) -> Vec<Uuid> {
        self.items.keys().copied().collect()
    }

    pub fn metas(&self) -> Vec<SelectedCardMeta> {
        self.items.values().cloned().collect()
    }
}

/// Persists the updated `LocalUiState` global (atomic disk save in production,
/// in-memory only under `#[cfg(test)]` so tests never touch user App Support).
pub fn update_local_ui_state(cx: &mut App, mutate: impl FnOnce(&mut LocalUiState)) {
    if cx.has_global::<LocalUiState>() {
        cx.update_global::<LocalUiState, _>(|state, _| mutate(state));
        #[cfg(not(test))]
        {
            let snapshot = cx.global::<LocalUiState>().clone();
            let path = LocalUiState::default_path();
            if let Err(err) = snapshot.save_atomic(&path) {
                tracing::warn!(%err, "Failed to persist LocalUiState");
            }
        }
    }
}

/// Spawns an abortable backend read wrapped inside `tokio::task::spawn_blocking`
/// + `Handle::block_on` to avoid cross-crate SQLx HRTB `Send` inference issues.
pub fn spawn_facade_read<F, Fut, R>(cx: &App, make_fut: F) -> ReadTask<R>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let handle = TokioBridge::handle(cx);
    TokioBridge::spawn_read(cx, async move {
        let h = handle.clone();
        tokio::task::spawn_blocking(move || h.block_on(make_fut()))
            .await
            .expect("facade read task panicked")
    })
}

/// Spawns a non-abortable backend write wrapped inside `tokio::task::spawn_blocking`
/// + `Handle::block_on` to avoid cross-crate SQLx HRTB `Send` inference issues.
pub fn spawn_facade_write<F, Fut, R>(cx: &App, make_fut: F) -> Task<Result<R, JoinError>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let handle = TokioBridge::handle(cx);
    TokioBridge::spawn_write(cx, async move {
        let h = handle.clone();
        tokio::task::spawn_blocking(move || h.block_on(make_fut()))
            .await
            .expect("facade write task panicked")
    })
}

/// Converts a backend [`Position`] into a [`BoardPosition`] for [`neondojo_board::BoardView`].
pub fn position_to_board_position(pos: &Position) -> BoardPosition {
    let mut board = [0i32; 26];
    for (slot, value) in board.iter_mut().zip(pos.board.iter().copied()) {
        *slot = value;
    }
    BoardPosition {
        board,
        player: u8::try_from(pos.player).unwrap_or(0).min(1),
        dice: pos
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok())
            .filter(|&d| (1..=6).contains(&d))
            .collect(),
        gnubg_match_id: pos.gnubg_match_id.clone(),
        notation: pos.notation.clone(),
    }
}

/// Converts a backend [`CubeEvaluation`] into shared [`CubeEvaluationData`].
pub fn cube_eval_to_view_data(eval: &CubeEvaluation) -> CubeEvaluationData {
    CubeEvaluationData {
        no_double: eval.no_double,
        double_take: eval.double_take,
        double_pass: eval.double_pass,
        unit: match eval.unit {
            BackendCubeUnit::Cubeful => CubeEquityUnit::NormalizedCubeful,
            BackendCubeUnit::Mwc => CubeEquityUnit::MatchWinningChance,
        },
        probabilities: eval.probabilities,
    }
}

/// Converts a backend `Position` + move notation into a [`MovePreview`] for arrows.
pub fn preview_for_notation(pos: &Position, notation: &str) -> Option<MovePreview> {
    let bp = position_to_board_position(pos);
    let trimmed = notation.trim();
    if trimmed.is_empty() {
        return None;
    }
    let preview = project_move(&bp.board, bp.player, &bp.dice, trimmed);
    preview.is_ok().then_some(preview)
}

/// Maps internal engine identifiers (`gnubg`, `bgsage`) to user-facing engine names.
pub fn engine_display_name(engine_id: &str) -> &'static str {
    match engine_id {
        "bgsage" => "NeonGammon",
        "gnubg" => "GNUbg",
        _ => "GNUbg",
    }
}

/// Interactive click-to-move checker turn controller shared by FSRS Review,
/// Practice Rounds, Transfer Practice, and Recurring Mistake Practice.
#[derive(Debug, Clone, Default)]
pub struct CheckerExerciseController {
    pub board_before: [i32; 26],
    pub player: u8,
    pub dice: [i32; 2],
    pub legal_turns: Vec<CheckerTurn>,
    pub chosen_steps: Vec<PlayStep>,
    pub chosen_dice: Vec<i32>,
    pub selected_source: Option<i32>,
}

impl CheckerExerciseController {
    /// Initializes interactive legal turns synchronously from `position` using `facade`
    /// (or directly from `neondojo::play::compute_checker_turns` without DB access).
    pub fn from_position(position: &Position, facade: Option<&BackendFacade>) -> Self {
        let bp = position_to_board_position(position);
        let dice = [
            position.dice.first().copied().unwrap_or(0),
            position.dice.get(1).copied().unwrap_or(0),
        ];
        let legal_turns = if (1..=6).contains(&dice[0]) && (1..=6).contains(&dice[1]) {
            if let Some(f) = facade {
                f.checker_turns(&bp.board, i32::from(bp.player), dice)
                    .unwrap_or_default()
            } else {
                neondojo::backgammon::checker_paths(&bp.board, i32::from(bp.player), dice)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|path| {
                        let absolute = |pt: i32| if bp.player == 0 { pt } else { 25 - pt };
                        let (steps, used_dice): (Vec<_>, Vec<_>) = path
                            .steps
                            .into_iter()
                            .map(|(from, to, die)| {
                                (
                                    PlayStep {
                                        from: absolute(from as i32),
                                        to: absolute(if to == 0 { -1 } else { to as i32 }),
                                    },
                                    die,
                                )
                            })
                            .unzip();
                        CheckerTurn {
                            steps,
                            dice: used_dice,
                        }
                    })
                    .collect()
            }
        } else {
            Vec::new()
        };

        Self {
            board_before: bp.board,
            player: bp.player,
            dice,
            legal_turns,
            chosen_steps: Vec::new(),
            chosen_dice: Vec::new(),
            selected_source: None,
        }
    }

    /// Returns `true` if no legal checker moves exist (forced pass / dance).
    pub fn is_forced_pass(&self) -> bool {
        self.legal_turns.is_empty()
            || (self.legal_turns.len() == 1 && self.legal_turns[0].steps.is_empty())
    }

    /// Returns `true` if the current prefix forms a complete legal turn.
    pub fn is_complete(&self) -> bool {
        if self.is_forced_pass() {
            return true;
        }
        self.legal_turns
            .iter()
            .any(|turn| turn.steps.len() == self.chosen_steps.len() && self.matches_prefix(turn))
    }

    /// Returns the board state after applying `chosen_steps`.
    pub fn current_board(&self) -> [i32; 26] {
        let pairs: Vec<(i32, i32)> = self.chosen_steps.iter().map(|s| (s.from, s.to)).collect();
        apply_absolute_steps(&self.board_before, &pairs, self.player)
    }

    /// Pops the last entered checker step (`Z` shortcut).
    pub fn undo_step(&mut self) -> bool {
        self.selected_source = None;
        self.chosen_dice.pop();
        self.chosen_steps.pop().is_some()
    }

    /// Clears all entered steps.
    pub fn reset(&mut self) {
        self.chosen_steps.clear();
        self.chosen_dice.clear();
        self.selected_source = None;
    }

    /// Handles a board point click: selects a source, or commits a step to a
    /// legal destination (or auto-commits when a source has a single destination).
    pub fn handle_point_click(&mut self, point: i32) -> bool {
        // Clicking a legal destination of the currently selected source commits that step.
        if let Some(src) = self.selected_source {
            if let Some(step_die) = self.next_step_die(src, point) {
                self.chosen_steps.push(PlayStep {
                    from: src,
                    to: point,
                });
                self.chosen_dice.push(step_die);
                self.selected_source = None;
                return true;
            }
            if src == point {
                // Clicking selected source again advances using its primary destination (highest die).
                let dests = self.destinations_from(src);
                if let Some(&first_dest) = dests.first()
                    && let Some(die) = self.next_step_die(src, first_dest)
                {
                    self.chosen_steps.push(PlayStep {
                        from: src,
                        to: first_dest,
                    });
                    self.chosen_dice.push(die);
                    self.selected_source = None;
                    return true;
                }
                self.selected_source = None;
                return true;
            }
        }

        // Otherwise check if clicked point is a legal source.
        let dests = self.destinations_from(point);
        if dests.is_empty() {
            return false;
        }
        if dests.len() == 1 {
            let dest = dests[0];
            if let Some(die) = self.next_step_die(point, dest) {
                self.chosen_steps.push(PlayStep {
                    from: point,
                    to: dest,
                });
                self.chosen_dice.push(die);
                self.selected_source = None;
                return true;
            }
        }
        self.selected_source = Some(point);
        true
    }

    /// Builds [`BoardInteractionState`] for rendering clickable points, hover
    /// destinations, and used-die indicators on [`neondojo_board::BoardView`].
    pub fn interaction_state(&self) -> BoardInteractionState {
        let sources = self.legal_sources();
        let destinations = self
            .selected_source
            .map(|src| self.destinations_from(src))
            .unwrap_or_default();
        let mut enabled: BTreeSet<i32> = sources.iter().copied().collect();
        enabled.extend(destinations.iter().copied());

        let used = self.compute_die_usage();
        BoardInteractionState {
            enabled_points: enabled.into_iter().collect(),
            source_points: sources,
            destination_points: destinations,
            hovered_source: self.selected_source,
            dice: Some(BoardDiceInteraction {
                can_swap: false,
                can_confirm: self.is_complete(),
                can_roll: false,
                can_pass: self.is_forced_pass(),
                used,
                previous_roll: None,
                opening: None,
                rolling: false,
            }),
            can_double: false,
            cube_interactive: false,
        }
    }

    fn matches_prefix(&self, turn: &CheckerTurn) -> bool {
        if turn.steps.len() < self.chosen_steps.len() {
            return false;
        }
        self.chosen_steps
            .iter()
            .zip(turn.steps.iter())
            .all(|(a, b)| a.from == b.from && a.to == b.to)
    }

    fn legal_sources(&self) -> Vec<i32> {
        let idx = self.chosen_steps.len();
        let mut set = BTreeSet::new();
        for turn in &self.legal_turns {
            if self.matches_prefix(turn)
                && let Some(step) = turn.steps.get(idx)
            {
                set.insert(step.from);
            }
        }
        set.into_iter().collect()
    }

    fn destinations_from(&self, source: i32) -> Vec<i32> {
        let idx = self.chosen_steps.len();
        let mut set = BTreeSet::new();
        for turn in &self.legal_turns {
            if self.matches_prefix(turn)
                && let Some(step) = turn.steps.get(idx)
                && step.from == source
            {
                set.insert(step.to);
            }
        }
        set.into_iter().collect()
    }

    fn next_step_die(&self, from: i32, to: i32) -> Option<i32> {
        let idx = self.chosen_steps.len();
        for turn in &self.legal_turns {
            if self.matches_prefix(turn)
                && let Some(step) = turn.steps.get(idx)
                && step.from == from
                && step.to == to
            {
                return turn.dice.get(idx).copied();
            }
        }
        None
    }

    fn compute_die_usage(&self) -> [DieUsage; 2] {
        let [d0, d1] = self.dice;
        if d0 == d1 && d0 > 0 {
            let count = self.chosen_dice.len();
            return [
                match count {
                    0 => DieUsage::Unused,
                    1 => DieUsage::Half,
                    _ => DieUsage::Used,
                },
                match count {
                    0..=2 => DieUsage::Unused,
                    3 => DieUsage::Half,
                    _ => DieUsage::Used,
                },
            ];
        }
        let mut used = [DieUsage::Unused, DieUsage::Unused];
        for &die in &self.chosen_dice {
            if die == d0 && used[0] == DieUsage::Unused {
                used[0] = DieUsage::Used;
            } else if die == d1 && used[1] == DieUsage::Unused {
                used[1] = DieUsage::Used;
            }
        }
        used
    }
}

/// Formats an FSRS interval in seconds into concise localized units (e.g. `10m`, `4d`, `2mo`).
pub fn format_interval(locale: Locale, seconds: i64) -> String {
    let secs = seconds.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let mins = (secs + 30) / 60;
        match locale {
            Locale::De => format!("{mins} Min."),
            Locale::En => format!("{mins}m"),
        }
    } else if secs < 86_400 {
        let hours = (secs + 1800) / 3600;
        match locale {
            Locale::De => format!("{hours} Std."),
            Locale::En => format!("{hours}h"),
        }
    } else if secs < 86_400 * 30 {
        let days = (secs + 43_200) / 86_400;
        match locale {
            Locale::De => format!("{days} T."),
            Locale::En => format!("{days}d"),
        }
    } else {
        let months = (secs + 86_400 * 15) / (86_400 * 30);
        match locale {
            Locale::De => format!("{months} Mon."),
            Locale::En => format!("{months}mo"),
        }
    }
}

pub fn game_plan_label(locale: Locale, plan: GamePlan) -> &'static str {
    let key = match plan {
        GamePlan::Opening => "opening",
        GamePlan::PureRace => "pure_race",
        GamePlan::Racing => "racing",
        GamePlan::Attacking => "attacking",
        GamePlan::Priming => "priming",
        GamePlan::Anchoring => "anchoring",
    };
    I18n::t(locale, "game-plan", key)
}

pub fn game_plan_from_str(raw: &str) -> Option<GamePlan> {
    match raw {
        "opening" => Some(GamePlan::Opening),
        "pure_race" => Some(GamePlan::PureRace),
        "racing" => Some(GamePlan::Racing),
        "attacking" => Some(GamePlan::Attacking),
        "priming" => Some(GamePlan::Priming),
        "anchoring" => Some(GamePlan::Anchoring),
        _ => None,
    }
}

pub const ALL_GAME_PLANS: &[GamePlan] = &[
    GamePlan::Opening,
    GamePlan::PureRace,
    GamePlan::Racing,
    GamePlan::Attacking,
    GamePlan::Priming,
    GamePlan::Anchoring,
];

pub fn mistake_kind_label(locale: Locale, kind: MistakeKind) -> &'static str {
    let key = match kind {
        MistakeKind::Checker => "checker",
        MistakeKind::MissedDouble => "missed_double",
        MistakeKind::PrematureDouble => "premature_double",
        MistakeKind::WrongTake => "wrong_take",
        MistakeKind::WrongPass => "wrong_pass",
        MistakeKind::UnclassifiedCube => "unclassified_cube",
    };
    I18n::t(locale, "recurring-mistakes", key)
}

pub fn mistake_kind_from_str(raw: &str) -> Option<MistakeKind> {
    match raw {
        "checker" => Some(MistakeKind::Checker),
        "missed_double" => Some(MistakeKind::MissedDouble),
        "premature_double" => Some(MistakeKind::PrematureDouble),
        "wrong_take" => Some(MistakeKind::WrongTake),
        "wrong_pass" => Some(MistakeKind::WrongPass),
        "unclassified_cube" => Some(MistakeKind::UnclassifiedCube),
        _ => None,
    }
}

pub fn card_kind_label(locale: Locale, kind: CardKind) -> &'static str {
    let key = match kind {
        CardKind::Anki => "anki",
        CardKind::Checker => "checker",
        CardKind::CubeOffer => "cube_offer",
        CardKind::CubeResponse => "cube_response",
    };
    I18n::t(locale, "card-search", key)
}

pub fn card_kind_str_label(locale: Locale, kind: &str) -> &'static str {
    match kind {
        "anki" => I18n::t(locale, "card-search", "anki"),
        "checker" => I18n::t(locale, "card-search", "checker"),
        "cube_offer" => I18n::t(locale, "card-search", "cube_offer"),
        "cube_response" => I18n::t(locale, "card-search", "cube_response"),
        _ => I18n::t(locale, "card-search", "allKinds"),
    }
}

pub fn topic_name_label(locale: Locale, topic: TopicName) -> &'static str {
    let key = match topic {
        TopicName::Cube => "cube",
        TopicName::Checker => "checker",
        TopicName::Opening => "opening",
        TopicName::BearOff => "bear_off",
        TopicName::Hit => "hit",
        TopicName::Backgame => "backgame",
        TopicName::Race => "race",
        TopicName::Prime => "prime",
        TopicName::Contact => "contact",
        TopicName::Anchor => "anchor",
    };
    I18n::t(locale, "match-assessment", key)
}

pub fn play_mode_str_label(locale: Locale, mode: &str) -> &'static str {
    match mode {
        "training" => I18n::t(locale, "play-mode", "training"),
        "coach" => I18n::t(locale, "play-mode", "coach"),
        "tutor" => I18n::t(locale, "play-mode", "tutor"),
        _ => I18n::t(locale, "play-mode", "competition"),
    }
}

pub fn play_mode_label(locale: Locale, mode: PlayMode) -> &'static str {
    match mode {
        PlayMode::Competition => I18n::t(locale, "play-mode", "competition"),
        PlayMode::Training => I18n::t(locale, "play-mode", "training"),
        PlayMode::Coach => I18n::t(locale, "play-mode", "coach"),
        PlayMode::Tutor => I18n::t(locale, "play-mode", "tutor"),
    }
}

pub fn rating_label(locale: Locale, rating: Rating) -> &'static str {
    match rating {
        Rating::Again => I18n::t(locale, "learning", "again"),
        Rating::Hard => I18n::t(locale, "learning", "hard"),
        Rating::Good => I18n::t(locale, "learning", "good"),
        Rating::Easy => I18n::t(locale, "learning", "easy"),
    }
}

pub fn verdict_label(locale: Locale, verdict: PracticeVerdict) -> &'static str {
    match verdict {
        PracticeVerdict::Best => I18n::t(locale, "recurring-mistakes", "best"),
        PracticeVerdict::Playable => I18n::t(locale, "learning", "verdictPlayable"),
        PracticeVerdict::Error => I18n::t(locale, "learning", "verdictError"),
        PracticeVerdict::Forced => I18n::t(locale, "app", "decision.cannotMove"),
    }
}

pub fn format_cards_count(locale: Locale, count: i64) -> String {
    I18n::format(
        locale,
        "learning",
        "cards",
        &[("count", Param::from(count))],
    )
}

pub fn format_due_count(locale: Locale, count: i64) -> String {
    I18n::format(
        locale,
        "learning",
        "due",
        &[("count", Param::from(format_int(locale, count)))],
    )
}
