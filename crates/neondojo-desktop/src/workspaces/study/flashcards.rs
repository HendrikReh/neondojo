//! Study → Flashcards & Anki sub-tab:
//! - Hierarchical `::` deck selector (with filter input when `> 12` decks, persisted `deck_id`)
//! - `LearningCardBrowser` (50/page pagination, cross-page multi-select capped at 200,
//!   resizable preview pane persisted in `LocalUiState::card_preview_width`, 9 filters +
//!   `AppliedFilterChips`, tag editor, saved `CardTopics`, scoped `AnkiCardFaceView` ONLY for
//!   `card.kind == "anki"`)
//! - `DeckLibraryDialogs` (deck CRUD with 409 refresh, batch add/move/remove/delete,
//!   `DuplicateCardsDialog` merge)
//! - Native `.apkg` file pickers (`cx.prompt_for_new_path` / multi-file `cx.prompt_for_paths`)
//! - AnkiConnect send/sync, per-deck auto-sync, and `AnkiSyncOverview` modal subscribed to
//!   `facade.subscribe_anki_sync_updates()`.

use super::common::{
    ALL_CARDS_DECK, ALL_GAME_PLANS, CARD_SELECTION_LIMIT, CARDS_PER_PAGE, CardSelectionSet,
    DECK_FILTER_THRESHOLD, DEFAULT_CARD_PREVIEW_WIDTH, MAX_CARD_PREVIEW_WIDTH,
    MIN_CARD_PREVIEW_WIDTH, SelectedCardMeta, UiAction, UiCallback, UiCallback2, UiCallback3,
    UiCallback4, UiCallback5, WITHOUT_DECK_ID, card_kind_str_label, clamp_preview_width,
    format_cards_count, format_due_count, game_plan_label, position_to_board_position,
    preview_for_notation,
};
use crate::anki_webview::{AnkiCardFaceRequest, AnkiCardFaceView, AnkiCardSide};
use crate::i18n::current_locale;
use crate::keybindings::CTX_SPLITTER;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::filter_chips::{AppliedFilterChip, AppliedFilterChips};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::pagination::NumberedSearchPagination;
use gpui_kit::{
    App, AppContext, CursorStyle, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::game_plan::GamePlan;
use neondojo::learning::{
    AnkiConnectStatus, AnkiSyncHistoryEntry, AutoSyncSettings, CardAnswer, CardBrowseItem,
    CardKind, CardPage, CardQuery, CardQuestion, CardReviewState, CardTopic, Deck, DuplicatePage,
    LinkedAnkiDeck, OverviewStatus, SyncOutcome,
};
use neondojo_board::{BoardView, BoardViewMode};
use neondojo_i18n::{
    I18n, Locale, Param, format_datetime_short_local, format_int, localize_decision,
};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Grouped deck hierarchy entry matching `web/src/workspaces/study/flashcards/deck-groups.ts`.
#[derive(Debug, Clone)]
pub struct DeckGroup<'a> {
    pub path: String,
    pub decks: Vec<(&'a Deck, String)>,
}

/// Groups decks by Anki `::` parent paths while preserving original order
/// within each group and filtering by `filter` (always keeping `keep_id` visible).
pub fn group_decks<'a>(
    decks: &'a [Deck],
    filter: &str,
    keep_id: Option<Uuid>,
) -> Vec<DeckGroup<'a>> {
    let needle = filter.trim().to_ascii_lowercase();
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<(&'a Deck, String)>> = HashMap::new();

    for deck in decks {
        if !needle.is_empty()
            && Some(deck.id) != keep_id
            && !deck.name.to_ascii_lowercase().contains(&needle)
        {
            continue;
        }
        let mut parts: Vec<&str> = deck.name.split("::").map(str::trim).collect();
        let label = parts
            .pop()
            .filter(|s| !s.is_empty())
            .unwrap_or(&deck.name)
            .to_string();
        let path = parts
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" › ");
        if !map.contains_key(&path) {
            order.push(path.clone());
        }
        map.entry(path).or_default().push((deck, label));
    }

    let mut groups: Vec<DeckGroup<'a>> = order
        .into_iter()
        .filter_map(|path| {
            let decks = map.remove(&path)?;
            Some(DeckGroup { path, decks })
        })
        .collect();
    // Top-level decks (`path == ""`) first, then hierarchical paths in original order.
    groups.sort_by_key(|g| !g.path.is_empty());
    groups
}

/// All 9 search filters of `LearningCardBrowser`.
#[derive(Debug, Default, Clone)]
pub struct CardFilterState {
    pub q: String,
    pub player: String,
    pub kind: Option<CardKind>,
    pub status: Option<CardReviewState>,
    pub game_plan: Option<GamePlan>,
    pub tag: String,
    pub topics: String,
    pub comment: String,
    pub position_id: String,
}

impl CardFilterState {
    pub fn to_query(&self, selected_deck: &str, page: usize) -> CardQuery {
        let (deck_id, without_deck) = match selected_deck {
            ALL_CARDS_DECK => (None, None),
            WITHOUT_DECK_ID => (None, Some(true)),
            other => (Uuid::parse_str(other).ok(), None),
        };
        let opt_str = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        CardQuery {
            after: None,
            page: Some(page.max(1) as i64),
            limit: Some(CARDS_PER_PAGE),
            deck_id,
            without_deck,
            q: opt_str(&self.q),
            player: opt_str(&self.player),
            tag: opt_str(&self.tag),
            topics: opt_str(&self.topics),
            game_plan: self.game_plan,
            comment: opt_str(&self.comment),
            position_id: opt_str(&self.position_id),
            kind: self.kind,
            status: self.status,
        }
    }

    pub fn from_topic_filter(query: &CardQuery) -> Self {
        Self {
            q: query.q.clone().unwrap_or_default(),
            player: query.player.clone().unwrap_or_default(),
            kind: query.kind,
            status: query.status,
            game_plan: query.game_plan,
            tag: query.tag.clone().unwrap_or_default(),
            topics: query.topics.clone().unwrap_or_default(),
            comment: query.comment.clone().unwrap_or_default(),
            position_id: query.position_id.clone().unwrap_or_default(),
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn applied_chips(&self, locale: Locale) -> Vec<AppliedFilterChip> {
        let mut chips = Vec::new();
        if !self.q.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("q", I18n::t(locale, "card-search", "search"))
                    .value(self.q.clone()),
            );
        }
        if !self.player.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("player", I18n::t(locale, "learning", "player"))
                    .value(self.player.clone()),
            );
        }
        if let Some(kind) = self.kind {
            let label = match kind {
                CardKind::Anki => "anki",
                CardKind::Checker => "checker",
                CardKind::CubeOffer => "cube_offer",
                CardKind::CubeResponse => "cube_response",
            };
            chips.push(
                AppliedFilterChip::new("kind", I18n::t(locale, "card-search", "kind"))
                    .value(I18n::t(locale, "card-search", label)),
            );
        }
        if let Some(status) = self.status {
            let key = match status {
                CardReviewState::New => "new",
                CardReviewState::Due => "due",
                CardReviewState::Reviewed => "reviewed",
            };
            chips.push(
                AppliedFilterChip::new("status", I18n::t(locale, "card-search", "status"))
                    .value(I18n::t(locale, "card-search", key)),
            );
        }
        if let Some(plan) = self.game_plan {
            chips.push(
                AppliedFilterChip::new("game_plan", I18n::t(locale, "game-plan", "title"))
                    .value(game_plan_label(locale, plan)),
            );
        }
        if !self.tag.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("tag", I18n::t(locale, "learning", "tags"))
                    .value(self.tag.clone()),
            );
        }
        if !self.topics.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("topics", I18n::t(locale, "card-practice", "topics"))
                    .value(self.topics.clone()),
            );
        }
        if !self.comment.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("comment", I18n::t(locale, "learning", "notes"))
                    .value(self.comment.clone()),
            );
        }
        if !self.position_id.trim().is_empty() {
            chips.push(
                AppliedFilterChip::new("position_id", I18n::t(locale, "app", "app.positionId"))
                    .value(self.position_id.clone()),
            );
        }
        chips
    }
}

/// Complete Flashcards & Anki browser/deck/sync state.
pub struct FlashcardsState {
    pub decks: Vec<Deck>,
    pub decks_loading: bool,
    pub decks_error: Option<String>,
    pub selected_deck: String,
    pub deck_filter_input: Entity<InputState>,
    pub new_deck_input: Entity<InputState>,
    pub show_create_deck: bool,

    // Card browser + 9 filters + saved topics + tag editor
    pub filters: CardFilterState,
    pub show_advanced_filters: bool,
    pub q_input: Entity<InputState>,
    pub player_input: Entity<InputState>,
    pub tag_input: Entity<InputState>,
    pub topics_input: Entity<InputState>,
    pub comment_input: Entity<InputState>,
    pub position_id_input: Entity<InputState>,
    pub topic_name_input: Entity<InputState>,
    pub edit_tag_input: Entity<InputState>,
    pub page: usize,
    pub card_page: Option<CardPage>,
    pub cards_loading: bool,
    pub cards_error: Option<String>,
    pub selection: CardSelectionSet,
    pub saved_topics: Vec<CardTopic>,
    pub suggested_tags: Vec<String>,

    // Resizable preview pane
    pub preview_width: u32,
    pub splitter_focus: FocusHandle,
    pub preview_card_id: Option<Uuid>,
    pub preview_answer: Option<CardAnswer>,
    pub preview_answer_loading: bool,
    pub preview_board_view: Entity<BoardView>,
    pub preview_anki_view: Entity<AnkiCardFaceView>,

    // Anki transfer & AnkiConnect status
    pub transfer_status: Option<String>,
    pub transfer_error: Option<String>,
    pub transfer_busy: bool,
    pub anki_connect: Option<AnkiConnectStatus>,
    pub deck_auto_sync: Option<AutoSyncSettings>,
    pub show_tools_drawer: bool,
}

impl FlashcardsState {
    pub fn new(
        initial_deck_id: String,
        initial_preview_width: Option<u32>,
        preview_board_view: Entity<BoardView>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let locale = current_locale(cx);
        let deck_filter_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "learning", "filterDecks"))
        });
        let new_deck_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "learning", "deckName"))
        });
        let q_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "card-search", "search"))
        });
        let player_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "learning", "player"))
        });
        let tag_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(I18n::t(locale, "learning", "tags")));
        let topics_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "card-practice", "topicFilter"))
        });
        let comment_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "learning", "notes"))
        });
        let position_id_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "app", "app.positionId"))
        });
        let topic_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "card-practice", "topicName"))
        });
        let edit_tag_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "card-practice", "addTag"))
        });
        let preview_anki_view = cx.new(|cx| AnkiCardFaceView::new(None, cx));

        Self {
            decks: Vec::new(),
            decks_loading: false,
            decks_error: None,
            selected_deck: if initial_deck_id.trim().is_empty() {
                ALL_CARDS_DECK.to_string()
            } else {
                initial_deck_id
            },
            deck_filter_input,
            new_deck_input,
            show_create_deck: false,
            filters: CardFilterState::default(),
            show_advanced_filters: false,
            q_input,
            player_input,
            tag_input,
            topics_input,
            comment_input,
            position_id_input,
            topic_name_input,
            edit_tag_input,
            page: 1,
            card_page: None,
            cards_loading: false,
            cards_error: None,
            selection: CardSelectionSet::default(),
            saved_topics: Vec::new(),
            suggested_tags: Vec::new(),
            preview_width: clamp_preview_width(
                initial_preview_width.unwrap_or(DEFAULT_CARD_PREVIEW_WIDTH),
            ),
            splitter_focus: cx.focus_handle(),
            preview_card_id: None,
            preview_answer: None,
            preview_answer_loading: false,
            preview_board_view,
            preview_anki_view,
            transfer_status: None,
            transfer_error: None,
            transfer_busy: false,
            anki_connect: None,
            deck_auto_sync: None,
            show_tools_drawer: false,
        }
    }

    /// Returns the concrete UUID of the selected deck, or `None` for All / Without Deck.
    pub fn selected_deck_uuid(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.selected_deck).ok()
    }

    pub fn selected_deck_record(&self) -> Option<&Deck> {
        let id = self.selected_deck_uuid()?;
        self.decks.iter().find(|d| d.id == id)
    }

    pub fn preview_item(&self) -> Option<&CardBrowseItem> {
        let id = self.preview_card_id?;
        self.card_page
            .as_ref()?
            .items
            .iter()
            .find(|item| item.card.id == id)
    }

    pub fn sync_filters_from_inputs(&mut self, cx: &App) {
        self.filters.q = self.q_input.read(cx).value().to_string();
        self.filters.player = self.player_input.read(cx).value().to_string();
        self.filters.tag = self.tag_input.read(cx).value().to_string();
        self.filters.topics = self.topics_input.read(cx).value().to_string();
        self.filters.comment = self.comment_input.read(cx).value().to_string();
        self.filters.position_id = self.position_id_input.read(cx).value().to_string();
    }

    pub fn push_filters_to_inputs(&self, window: &mut Window, cx: &mut App) {
        let f = &self.filters;
        self.q_input
            .update(cx, |s, cx| s.set_value(f.q.clone(), window, cx));
        self.player_input
            .update(cx, |s, cx| s.set_value(f.player.clone(), window, cx));
        self.tag_input
            .update(cx, |s, cx| s.set_value(f.tag.clone(), window, cx));
        self.topics_input
            .update(cx, |s, cx| s.set_value(f.topics.clone(), window, cx));
        self.comment_input
            .update(cx, |s, cx| s.set_value(f.comment.clone(), window, cx));
        self.position_id_input
            .update(cx, |s, cx| s.set_value(f.position_id.clone(), window, cx));
    }

    /// Synchronizes preview `BoardView` or strictly scoped `AnkiCardFaceView` (`kind == "anki"` only).
    pub fn sync_preview_visuals(&self, visible: bool, cx: &mut App) {
        let Some(item) = self.preview_item() else {
            self.preview_anki_view
                .update(cx, |v, cx| v.set_request(None, cx));
            return;
        };
        if item.card.kind == "anki" {
            let side = if self.preview_answer.is_some() {
                AnkiCardSide::Back
            } else {
                AnkiCardSide::Front
            };
            self.preview_anki_view.update(cx, |v, cx| {
                v.set_visible(visible, cx);
                v.set_request(Some(AnkiCardFaceRequest::new(item.card.id, side)), cx);
            });
            return;
        }
        self.preview_anki_view
            .update(cx, |v, cx| v.set_request(None, cx));
        if let Some(pos) = &item.card.position {
            let bp = position_to_board_position(pos);
            let preview = if item.card.kind == "checker" {
                self.preview_answer
                    .as_ref()
                    .and_then(|a| a.solution.as_ref())
                    .and_then(|s| {
                        s.decision
                            .as_ref()
                            .map(|d| d.best.as_str())
                            .or_else(|| s.candidates.first().map(|c| c.notation.as_str()))
                    })
                    .and_then(|n| preview_for_notation(pos, n))
            } else {
                None
            };
            let mode = if preview.is_some() {
                BoardViewMode::Arrows
            } else {
                BoardViewMode::Before
            };
            self.preview_board_view.update(cx, |bv, cx| {
                bv.set_players(
                    item.card.players[0].clone(),
                    item.card.players[1].clone(),
                    cx,
                );
                bv.set_cube_offer_pending(item.card.kind == "cube_response", cx);
                bv.set_show_pip_counts(true, cx);
                bv.set_position(bp, cx);
                bv.set_interaction(None, cx);
                bv.set_preview(preview, mode, cx);
            });
        }
    }

    pub fn last_page(&self) -> usize {
        let total = self.card_page.as_ref().map_or(0, |p| p.total.max(0)) as usize;
        total.div_ceil(CARDS_PER_PAGE as usize).max(1)
    }
}

/// Callbacks wired from `StudyWorkspace` into the Flashcards browser renderer.
pub struct FlashcardsCallbacks {
    pub on_select_deck: UiCallback<String>,
    pub on_toggle_create_deck: UiAction,
    pub on_create_deck: UiCallback<String>,
    pub on_study_deck: UiCallback<Uuid>,
    pub on_start_round: UiCallback<i64>,
    pub on_resume_round: UiCallback<Uuid>,
    pub on_open_manage_decks: UiAction,
    pub on_open_duplicates: UiAction,
    pub on_open_anki_sync_overview: UiAction,
    pub on_export_apkg: UiCallback<Uuid>,
    pub on_import_deck_apkg: UiCallback<Uuid>,
    pub on_import_external_apkg: UiAction,
    pub on_send_to_anki: UiCallback<Uuid>,
    pub on_sync_with_anki: UiCallback<Uuid>,
    pub on_toggle_auto_sync: UiCallback2<Uuid, bool>,
    pub on_apply_filters: UiAction,
    pub on_toggle_advanced: UiAction,
    pub on_set_kind: UiCallback<Option<CardKind>>,
    pub on_set_status: UiCallback<Option<CardReviewState>>,
    pub on_set_game_plan: UiCallback<Option<GamePlan>>,
    pub on_remove_chip: UiCallback<String>,
    pub on_clear_filters: UiAction,
    pub on_save_topic: UiCallback<String>,
    pub on_apply_topic: UiCallback<CardQuery>,
    pub on_delete_topic: UiCallback<Uuid>,
    pub on_select_page: UiCallback<usize>,
    pub on_toggle_card_select: UiCallback<SelectedCardMeta>,
    pub on_select_all_page: UiAction,
    pub on_clear_selection: UiAction,
    pub on_preview_card: UiCallback<Uuid>,
    pub on_reveal_preview: UiCallback<Uuid>,
    pub on_open_pips_modal: UiCallback<CardQuestion>,
    pub on_open_score_variants: UiCallback<Uuid>,
    pub on_open_timed_preview: UiAction,
    pub on_open_batch_action: UiCallback<BatchCardAction>,
    pub on_apply_tags: UiCallback2<bool, String>, // true = add, false = remove
    pub on_adjust_preview_width: UiCallback<i32>,
    pub on_toggle_tools: UiAction,
}

/// Batch card library operations launched from selection toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchCardAction {
    AddToDeck,
    MoveToDeck,
    RemoveFromDeck,
    DeletePermanently,
}

/// Renders Study → Flashcards & Anki (`LearningCardBrowser` + Deck sidebar + Resizable Preview).
pub fn render_flashcards_browser(
    state: &FlashcardsState,
    resumable_round_id: Option<Uuid>,
    transfer_section: Option<gpui_kit::AnyElement>,
    palette: NeonPalette,
    cb: FlashcardsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let selected_deck = state.selected_deck_record();
    let deck_uuid = state.selected_deck_uuid();

    // ── Header toolbar: Deck actions, Study Now, Practice Round, .apkg & AnkiConnect
    let mut toolbar = div()
        .id("study-flashcards-toolbar")
        .test_support()
        .h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .pb(px(10.0))
        .border_b_1()
        .border_color(palette.border);

    let mut left_actions = div().h_flex().flex_wrap().items_center().gap(px(6.0));
    if let Some(id) = deck_uuid {
        let on_study = cb.on_study_deck.clone();
        left_actions = left_actions.child(
            ActionButton::new("flashcards-study-now")
                .icon(ActionIconName::Cards)
                .label(I18n::t(locale, "learning", "studyNow"))
                .primary()
                .palette(palette)
                .on_click(move |_, window, cx| on_study(id, window, cx)),
        );
    }
    let on_round_10 = cb.on_start_round.clone();
    let on_round_20 = cb.on_start_round.clone();
    left_actions = left_actions
        .child(
            ActionButton::new("flashcards-practice-10")
                .icon(ActionIconName::Play)
                .label(format!(
                    "{} (10)",
                    I18n::t(locale, "card-practice", "start")
                ))
                .palette(palette)
                .on_click(move |_, window, cx| on_round_10(10, window, cx)),
        )
        .child(
            ActionButton::new("flashcards-practice-20")
                .label("20")
                .palette(palette)
                .on_click(move |_, window, cx| on_round_20(20, window, cx)),
        )
        .when_some(resumable_round_id, |this, rid| {
            let on_resume = cb.on_resume_round.clone();
            this.child(
                ActionButton::new("flashcards-resume-round")
                    .icon(ActionIconName::Resume)
                    .label(I18n::t(locale, "card-practice", "resume"))
                    .palette(palette)
                    .on_click(move |_, window, cx| on_resume(rid, window, cx)),
            )
        })
        .child({
            let on_timed = cb.on_open_timed_preview.clone();
            ActionButton::new("flashcards-timed-preview")
                .icon(ActionIconName::View)
                .label(I18n::t(locale, "card-practice", "preview"))
                .palette(palette)
                .on_click(move |_, window, cx| on_timed(window, cx))
        });

    let mut right_actions = div().h_flex().flex_wrap().items_center().gap(px(6.0));
    if let Some(id) = deck_uuid {
        let on_exp = cb.on_export_apkg.clone();
        let on_imp = cb.on_import_deck_apkg.clone();
        let on_send = cb.on_send_to_anki.clone();
        let on_sync = cb.on_sync_with_anki.clone();
        right_actions = right_actions
            .child(
                ActionButton::new("flashcards-export-apkg")
                    .icon(ActionIconName::Export)
                    .label(I18n::t(locale, "learning", "export"))
                    .disabled(state.transfer_busy)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_exp(id, window, cx)),
            )
            .child(
                ActionButton::new("flashcards-import-deck-apkg")
                    .icon(ActionIconName::Import)
                    .label(I18n::t(locale, "card-practice", "ankiImport"))
                    .disabled(state.transfer_busy)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_imp(id, window, cx)),
            )
            .child(
                ActionButton::new("flashcards-send-anki")
                    .icon(ActionIconName::Link)
                    .label(I18n::t(locale, "card-practice", "sendAnki"))
                    .disabled(state.transfer_busy)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_send(id, window, cx)),
            )
            .child(
                ActionButton::new("flashcards-sync-anki")
                    .icon(ActionIconName::Refresh)
                    .label(I18n::t(locale, "card-practice", "syncAnkiReviews"))
                    .disabled(state.transfer_busy)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_sync(id, window, cx)),
            );
    }
    right_actions = right_actions
        .child({
            let on_ext = cb.on_import_external_apkg.clone();
            ActionButton::new("flashcards-import-external-apkg")
                .icon(ActionIconName::Import)
                .label(I18n::t(locale, "card-practice", "externalAnkiImport"))
                .disabled(state.transfer_busy)
                .palette(palette)
                .on_click(move |_, window, cx| on_ext(window, cx))
        })
        .child({
            let on_sync_ov = cb.on_open_anki_sync_overview.clone();
            ActionButton::new("flashcards-anki-overview")
                .icon(ActionIconName::Link)
                .label(I18n::t(locale, "card-practice", "ankiOverview"))
                .palette(palette)
                .on_click(move |_, window, cx| on_sync_ov(window, cx))
        })
        .child({
            let on_manage = cb.on_open_manage_decks.clone();
            ActionButton::new("flashcards-manage-decks")
                .icon(ActionIconName::Deck)
                .label(I18n::t(locale, "deck-library", "decks"))
                .palette(palette)
                .on_click(move |_, window, cx| on_manage(window, cx))
        })
        .child({
            let on_dups = cb.on_open_duplicates.clone();
            ActionButton::new("flashcards-duplicates")
                .icon(ActionIconName::Duplicates)
                .label(I18n::t(locale, "deck-library", "duplicates"))
                .palette(palette)
                .on_click(move |_, window, cx| on_dups(window, cx))
        })
        .child({
            let on_tools = cb.on_toggle_tools.clone();
            ActionButton::new("flashcards-study-tools")
                .icon(ActionIconName::Filter)
                .label(I18n::t(locale, "card-practice", "studyTools"))
                .active(state.show_tools_drawer)
                .palette(palette)
                .on_click(move |_, window, cx| on_tools(window, cx))
        });

    toolbar = toolbar.child(left_actions).child(right_actions);

    // ── Left Sidebar: Hierarchical Deck Selector (filter input when > 12 decks)
    let filter_text = state.deck_filter_input.read(cx).value().to_string();
    let deck_groups = group_decks(&state.decks, &filter_text, deck_uuid);

    let mut deck_sidebar = div()
        .id("study-deck-selector")
        .test_support()
        .v_flex()
        .gap(px(6.0))
        .w(px(230.0))
        .flex_shrink_0()
        .p(px(10.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "learning",
                    "chooseDeck",
                )))
                .child({
                    let on_toggle = cb.on_toggle_create_deck.clone();
                    ActionButton::new("deck-create-toggle")
                        .icon(ActionIconName::Add)
                        .tooltip(I18n::t(locale, "learning", "newDeck"))
                        .variant(ActionButtonVariant::Ghost)
                        .dense_nav()
                        .palette(palette)
                        .on_click(move |_, window, cx| on_toggle(window, cx))
                }),
        );

    if state.show_create_deck {
        let on_create = cb.on_create_deck.clone();
        let inp = state.new_deck_input.clone();
        deck_sidebar = deck_sidebar.child(
            div()
                .v_flex()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(palette.surface_alt)
                .child(Input::new(&state.new_deck_input).id("new-deck-name-input"))
                .child(
                    ActionButton::new("new-deck-submit")
                        .icon(ActionIconName::Confirm)
                        .label(I18n::t(locale, "learning", "create"))
                        .primary()
                        .palette(palette)
                        .on_click(move |_, window, cx| {
                            let name = inp.read(cx).value().trim().to_string();
                            if !name.is_empty() {
                                on_create(name, window, cx);
                            }
                        }),
                ),
        );
    }

    // Show deck filter input when > 12 decks (requirement!)
    if state.decks.len() > DECK_FILTER_THRESHOLD {
        deck_sidebar = deck_sidebar.child(
            div()
                .id("deck-filter-wrapper")
                .test_support()
                .child(Input::new(&state.deck_filter_input).id("deck-filter-input")),
        );
    }

    let render_deck_row = |el_id: SharedString,
                           label: String,
                           meta: Option<String>,
                           target: String,
                           active: bool,
                           on_sel: UiCallback<String>| {
        let hover_bg = palette.surface_alt;
        div()
            .id(el_id)
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .bg(if active {
                palette.selected
            } else {
                palette.surface
            })
            .hover(move |s| s.bg(hover_bg))
            .on_click(move |_, window, cx| on_sel(target.clone(), window, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_label()
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if active { palette.accent } else { palette.text })
                    .child(label),
            )
            .when_some(meta, |this, m| {
                this.child(
                    div()
                        .text_micro()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(m),
                )
            })
    };

    deck_sidebar = deck_sidebar
        .child(render_deck_row(
            "deck-option-all".into(),
            I18n::t(locale, "card-search", "allCards").to_string(),
            None,
            ALL_CARDS_DECK.to_string(),
            state.selected_deck == ALL_CARDS_DECK,
            cb.on_select_deck.clone(),
        ))
        .child(render_deck_row(
            "deck-option-none".into(),
            I18n::t(locale, "card-search", "withoutDeck").to_string(),
            None,
            WITHOUT_DECK_ID.to_string(),
            state.selected_deck == WITHOUT_DECK_ID,
            cb.on_select_deck.clone(),
        ));

    for group in deck_groups {
        if !group.path.is_empty() {
            deck_sidebar = deck_sidebar.child(
                div()
                    .pt(px(6.0))
                    .px(px(6.0))
                    .text_micro()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.muted)
                    .child(group.path),
            );
        }
        for (deck, label) in group.decks {
            let id_str = deck.id.to_string();
            let active = state.selected_deck == id_str;
            let meta = if deck.due_count > 0 {
                format!("{} / {}", deck.due_count, deck.card_count)
            } else {
                deck.card_count.to_string()
            };
            deck_sidebar = deck_sidebar.child(render_deck_row(
                SharedString::from(format!("deck-option-{}", deck.id)),
                label,
                Some(meta),
                id_str,
                active,
                cb.on_select_deck.clone(),
            ));
        }
    }

    // Auto-sync status strip inside sidebar when a specific deck is chosen
    if let (Some(id), Some(auto)) = (deck_uuid, &state.deck_auto_sync) {
        let enabled = auto.profile.is_some();
        let on_toggle_auto = cb.on_toggle_auto_sync.clone();
        deck_sidebar = deck_sidebar.child(
            div()
                .v_flex()
                .gap(px(4.0))
                .pt(px(8.0))
                .mt(px(4.0))
                .border_t_1()
                .border_color(palette.border)
                .child(
                    ActionButton::new("deck-auto-sync-toggle")
                        .icon(ActionIconName::Link)
                        .label(I18n::t(locale, "card-practice", "ankiAutoSync"))
                        .active(enabled)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_toggle_auto(id, !enabled, window, cx)),
                )
                .when_some(auto.profile.clone(), |this, prof| {
                    this.child(
                        div()
                            .text_micro()
                            .text_color(palette.muted)
                            .child(I18n::format(
                                locale,
                                "card-practice",
                                "ankiAutoProfile",
                                &[("profile", Param::from(prof.as_str()))],
                            )),
                    )
                }),
        );
    }

    // ── Center Column: 9-Filter Bar + Topics + Batch Selection Bar + Paginated Table
    let on_apply = cb.on_apply_filters.clone();
    let on_adv = cb.on_toggle_advanced.clone();
    let mut filter_bar = div()
        .v_flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(Input::new(&state.q_input).id("card-filter-q")),
                )
                .child(
                    div()
                        .w(px(150.0))
                        .child(Input::new(&state.player_input).id("card-filter-player")),
                )
                .child(
                    ActionButton::new("card-filter-apply")
                        .icon(ActionIconName::Search)
                        .label(I18n::t(locale, "card-search", "search"))
                        .primary()
                        .palette(palette)
                        .on_click(move |_, window, cx| on_apply(window, cx)),
                )
                .child(
                    ActionButton::new("card-filter-advanced-toggle")
                        .icon(ActionIconName::Filter)
                        .label(I18n::t(locale, "card-search", "kind"))
                        .active(state.show_advanced_filters)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_adv(window, cx)),
                ),
        );

    if state.show_advanced_filters {
        let mut kind_row = div().h_flex().flex_wrap().gap(px(4.0)).child({
            let on_k = cb.on_set_kind.clone();
            ActionButton::new("card-kind-all")
                .label(I18n::t(locale, "card-search", "allKinds"))
                .active(state.filters.kind.is_none())
                .palette(palette)
                .on_click(move |_, window, cx| on_k(None, window, cx))
        });
        for (k, key) in [
            (CardKind::Checker, "checker"),
            (CardKind::CubeOffer, "cube_offer"),
            (CardKind::CubeResponse, "cube_response"),
            (CardKind::Anki, "anki"),
        ] {
            let on_k = cb.on_set_kind.clone();
            let active = matches!(
                (state.filters.kind, k),
                (Some(CardKind::Checker), CardKind::Checker)
                    | (Some(CardKind::CubeOffer), CardKind::CubeOffer)
                    | (Some(CardKind::CubeResponse), CardKind::CubeResponse)
                    | (Some(CardKind::Anki), CardKind::Anki)
            );
            kind_row = kind_row.child(
                ActionButton::new(SharedString::from(format!("card-kind-{key}")))
                    .label(I18n::t(locale, "card-search", key))
                    .active(active)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_k(Some(k), window, cx)),
            );
        }

        let mut status_row = div().h_flex().flex_wrap().gap(px(4.0)).child({
            let on_s = cb.on_set_status.clone();
            ActionButton::new("card-status-all")
                .label(I18n::t(locale, "card-search", "allStatuses"))
                .active(state.filters.status.is_none())
                .palette(palette)
                .on_click(move |_, window, cx| on_s(None, window, cx))
        });
        for (st, key) in [
            (CardReviewState::New, "new"),
            (CardReviewState::Due, "due"),
            (CardReviewState::Reviewed, "reviewed"),
        ] {
            let on_s = cb.on_set_status.clone();
            let active = matches!(
                (state.filters.status, st),
                (Some(CardReviewState::New), CardReviewState::New)
                    | (Some(CardReviewState::Due), CardReviewState::Due)
                    | (Some(CardReviewState::Reviewed), CardReviewState::Reviewed)
            );
            status_row = status_row.child(
                ActionButton::new(SharedString::from(format!("card-status-{key}")))
                    .label(I18n::t(locale, "card-search", key))
                    .active(active)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_s(Some(st), window, cx)),
            );
        }

        let mut plan_row = div().h_flex().flex_wrap().gap(px(4.0)).child({
            let on_p = cb.on_set_game_plan.clone();
            ActionButton::new("card-plan-any")
                .label(I18n::t(locale, "recurring-mistakes", "any"))
                .active(state.filters.game_plan.is_none())
                .palette(palette)
                .on_click(move |_, window, cx| on_p(None, window, cx))
        });
        for &plan in ALL_GAME_PLANS {
            let on_p = cb.on_set_game_plan.clone();
            plan_row = plan_row.child(
                ActionButton::new(SharedString::from(format!("card-plan-{plan:?}")))
                    .label(game_plan_label(locale, plan))
                    .active(state.filters.game_plan == Some(plan))
                    .palette(palette)
                    .on_click(move |_, window, cx| on_p(Some(plan), window, cx)),
            );
        }

        filter_bar = filter_bar
            .child(kind_row)
            .child(status_row)
            .child(plan_row)
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(140.0))
                            .child(Input::new(&state.tag_input).id("card-filter-tag")),
                    )
                    .child(
                        div()
                            .w(px(180.0))
                            .child(Input::new(&state.topics_input).id("card-filter-topics")),
                    )
                    .child(
                        div()
                            .w(px(150.0))
                            .child(Input::new(&state.comment_input).id("card-filter-comment")),
                    )
                    .child(
                        div()
                            .w(px(160.0))
                            .child(Input::new(&state.position_id_input).id("card-filter-pos-id")),
                    ),
            );
    }

    let chips = state.filters.applied_chips(locale);
    if !chips.is_empty() {
        let on_rm = cb.on_remove_chip.clone();
        let on_clr = cb.on_clear_filters.clone();
        filter_bar = filter_bar.child(
            AppliedFilterChips::new("card-applied-filters", chips)
                .palette(palette)
                .on_remove(move |id, window, cx| on_rm(id.to_string(), window, cx))
                .on_clear_all(move |window, cx| on_clr(window, cx)),
        );
    }

    // Optional Tools Drawer: Saved CardTopics + Transfer Practice
    let mut tools_section = div().v_flex().gap(px(10.0));
    if state.show_tools_drawer {
        let on_save_topic = cb.on_save_topic.clone();
        let topic_inp = state.topic_name_input.clone();
        let mut topics_card = div()
            .id("study-saved-topics")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .child(div().text_panel().text_color(palette.text).child(I18n::t(
                locale,
                "card-practice",
                "topics",
            )))
            .child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(220.0))
                            .child(Input::new(&state.topic_name_input).id("card-topic-name-input")),
                    )
                    .child(
                        ActionButton::new("card-topic-save")
                            .icon(ActionIconName::Save)
                            .label(I18n::t(locale, "card-practice", "saveTopic"))
                            .palette(palette)
                            .on_click(move |_, window, cx| {
                                let name = topic_inp.read(cx).value().trim().to_string();
                                if !name.is_empty() {
                                    on_save_topic(name, window, cx);
                                }
                            }),
                    ),
            );

        if !state.saved_topics.is_empty() {
            let mut topic_chips = div().h_flex().flex_wrap().gap(px(6.0));
            for topic in &state.saved_topics {
                let apply_cb = cb.on_apply_topic.clone();
                let del_cb = cb.on_delete_topic.clone();
                let filter_clone = CardQuery {
                    after: topic.filter.after,
                    page: topic.filter.page,
                    limit: topic.filter.limit,
                    deck_id: topic.filter.deck_id,
                    without_deck: topic.filter.without_deck,
                    q: topic.filter.q.clone(),
                    player: topic.filter.player.clone(),
                    tag: topic.filter.tag.clone(),
                    topics: topic.filter.topics.clone(),
                    game_plan: topic.filter.game_plan,
                    comment: topic.filter.comment.clone(),
                    position_id: topic.filter.position_id.clone(),
                    kind: topic.filter.kind,
                    status: topic.filter.status,
                };
                let tid = topic.id;
                topic_chips = topic_chips.child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(palette.border)
                        .bg(palette.surface_alt)
                        .child(
                            ActionButton::new(SharedString::from(format!("apply-topic-{tid}")))
                                .label(topic.name.clone())
                                .variant(ActionButtonVariant::Ghost)
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    let q = CardQuery {
                                        after: filter_clone.after,
                                        page: filter_clone.page,
                                        limit: filter_clone.limit,
                                        deck_id: filter_clone.deck_id,
                                        without_deck: filter_clone.without_deck,
                                        q: filter_clone.q.clone(),
                                        player: filter_clone.player.clone(),
                                        tag: filter_clone.tag.clone(),
                                        topics: filter_clone.topics.clone(),
                                        game_plan: filter_clone.game_plan,
                                        comment: filter_clone.comment.clone(),
                                        position_id: filter_clone.position_id.clone(),
                                        kind: filter_clone.kind,
                                        status: filter_clone.status,
                                    };
                                    apply_cb(q, window, cx)
                                }),
                        )
                        .child(
                            ActionButton::new(SharedString::from(format!("delete-topic-{tid}")))
                                .icon(ActionIconName::Close)
                                .tooltip(I18n::t(locale, "card-practice", "deleteTopic"))
                                .variant(ActionButtonVariant::Ghost)
                                .dense_nav()
                                .palette(palette)
                                .on_click(move |_, window, cx| del_cb(tid, window, cx)),
                        ),
                );
            }
            topics_card = topics_card.child(topic_chips);
        }
        tools_section = tools_section.child(topics_card);
        if let Some(transfer) = transfer_section {
            tools_section = tools_section.child(transfer);
        }
    }

    // Batch Selection Action Bar + Tag Editor
    let selected_count = state.selection.len();
    let mut selection_bar = div()
        .h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(8.0));

    let total_cards = state.card_page.as_ref().map_or(0, |p| p.total);
    selection_bar = selection_bar.child(
        div()
            .h_flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_meta()
                    .tabular_nums()
                    .text_color(palette.muted)
                    .child(I18n::format(
                        locale,
                        "card-search",
                        "found",
                        &[("count", Param::from(total_cards))],
                    )),
            )
            .child({
                let on_all = cb.on_select_all_page.clone();
                ActionButton::new("cards-select-page")
                    .icon(ActionIconName::SelectAll)
                    .label(I18n::t(locale, "app", "common.selectAll"))
                    .variant(ActionButtonVariant::Ghost)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_all(window, cx))
            }),
    );

    if selected_count > 0 {
        let on_add = cb.on_open_batch_action.clone();
        let on_move = cb.on_open_batch_action.clone();
        let on_rm = cb.on_open_batch_action.clone();
        let on_del = cb.on_open_batch_action.clone();
        let on_clear = cb.on_clear_selection.clone();
        let on_add_tag = cb.on_apply_tags.clone();
        let on_rm_tag = cb.on_apply_tags.clone();
        let tag_inp_add = state.edit_tag_input.clone();
        let tag_inp_rm = state.edit_tag_input.clone();

        selection_bar = selection_bar.child(
            div()
                .id("card-selection-actions")
                .test_support()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_label()
                        .tabular_nums()
                        .text_color(palette.accent)
                        .child(format!("{selected_count}/{CARD_SELECTION_LIMIT}")),
                )
                .child(
                    ActionButton::new("cards-batch-add")
                        .icon(ActionIconName::Add)
                        .label(I18n::t(locale, "deck-library", "addCards"))
                        .palette(palette)
                        .on_click(move |_, window, cx| {
                            on_add(BatchCardAction::AddToDeck, window, cx)
                        }),
                )
                .when(deck_uuid.is_some() && state.decks.len() >= 2, |this| {
                    this.child(
                        ActionButton::new("cards-batch-move")
                            .icon(ActionIconName::Move)
                            .label(I18n::format(
                                locale,
                                "deck-library",
                                "move",
                                &[("count", Param::from(selected_count as i64))],
                            ))
                            .palette(palette)
                            .on_click(move |_, window, cx| {
                                on_move(BatchCardAction::MoveToDeck, window, cx)
                            }),
                    )
                })
                .when(deck_uuid.is_some(), |this| {
                    this.child(
                        ActionButton::new("cards-batch-remove")
                            .label(I18n::t(locale, "deck-library", "removeCards"))
                            .palette(palette)
                            .on_click(move |_, window, cx| {
                                on_rm(BatchCardAction::RemoveFromDeck, window, cx)
                            }),
                    )
                })
                .child(
                    ActionButton::new("cards-batch-delete")
                        .icon(ActionIconName::Delete)
                        .label(I18n::t(locale, "deck-library", "deleteCards"))
                        .danger()
                        .palette(palette)
                        .on_click(move |_, window, cx| {
                            on_del(BatchCardAction::DeletePermanently, window, cx)
                        }),
                )
                .child(
                    div()
                        .w(px(120.0))
                        .child(Input::new(&state.edit_tag_input).id("batch-tag-input")),
                )
                .child(
                    ActionButton::new("cards-tag-add")
                        .icon(ActionIconName::Tags)
                        .label(I18n::t(locale, "card-practice", "addTag"))
                        .palette(palette)
                        .on_click(move |_, window, cx| {
                            let tag = tag_inp_add.read(cx).value().trim().to_string();
                            if !tag.is_empty() {
                                on_add_tag(true, tag, window, cx);
                            }
                        }),
                )
                .child(
                    ActionButton::new("cards-tag-remove")
                        .label(I18n::t(locale, "card-practice", "removeTag"))
                        .palette(palette)
                        .on_click(move |_, window, cx| {
                            let tag = tag_inp_rm.read(cx).value().trim().to_string();
                            if !tag.is_empty() {
                                on_rm_tag(false, tag, window, cx);
                            }
                        }),
                )
                .child(
                    ActionButton::new("cards-selection-clear")
                        .icon(ActionIconName::Clear)
                        .variant(ActionButtonVariant::Ghost)
                        .dense_nav()
                        .palette(palette)
                        .on_click(move |_, window, cx| on_clear(window, cx)),
                ),
        );
    }

    // Card table list (50/page)
    let mut table = div()
        .id("study-card-browser-table")
        .test_support()
        .v_flex()
        .flex_1()
        .rounded(px(8.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .overflow_hidden();

    if let Some(err) = &state.cards_error {
        table = table.child(div().p(px(12.0)).child(
            StatusBanner::new("cards-load-err", BannerKind::Error, err.clone()).palette(palette),
        ));
    } else if state.cards_loading && state.card_page.is_none() {
        table = table.child(
            div()
                .p(px(20.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "learning", "loading")),
        );
    } else if let Some(page_data) = &state.card_page {
        if page_data.items.is_empty() {
            table = table.child(
                div().p(px(12.0)).child(
                    EmptyState::new(
                        "cards-empty-state",
                        I18n::t(locale, "learning", "title"),
                        I18n::t(locale, "card-search", "noResults"),
                    )
                    .icon(ActionIconName::Cards)
                    .palette(palette),
                ),
            );
        } else {
            for (idx, item) in page_data.items.iter().enumerate() {
                let card_id = item.card.id;
                let checked = state.selection.contains(card_id);
                let active = state.preview_card_id == Some(card_id);
                let meta = SelectedCardMeta {
                    id: card_id,
                    version: item.card.version,
                    tags_version: item.browse.tags_version,
                    deck_ids: item.browse.decks.iter().map(|d| d.id).collect(),
                };
                let on_toggle = cb.on_toggle_card_select.clone();
                let on_prev = cb.on_preview_card.clone();
                let hover_bg = palette.surface_alt;
                let prompt_summary = if item.card.prompt.trim().is_empty() {
                    format!("{} vs {}", item.card.players[0], item.card.players[1])
                } else {
                    item.card.prompt.clone()
                };

                table = table.child(
                    div()
                        .id(SharedString::from(format!("card-row-{idx}")))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(7.0))
                        .border_b_1()
                        .border_color(palette.border)
                        .bg(if active {
                            palette.selected
                        } else {
                            palette.surface
                        })
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover_bg))
                        .on_click(move |_, window, cx| on_prev(card_id, window, cx))
                        .child(
                            div()
                                .id(SharedString::from(format!("card-select-{idx}")))
                                .test_support()
                                .w(px(22.0))
                                .h(px(22.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(if checked {
                                    palette.accent
                                } else {
                                    palette.control_border
                                })
                                .bg(if checked {
                                    palette.accent
                                } else {
                                    palette.surface
                                })
                                .h_flex()
                                .items_center()
                                .justify_center()
                                .text_micro()
                                .text_color(palette.accent_text)
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    on_toggle(meta.clone(), window, cx);
                                })
                                .child(if checked { "✓" } else { "" }),
                        )
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w(px(0.0))
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_table()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(palette.text)
                                        .truncate()
                                        .child(prompt_summary),
                                )
                                .child(
                                    div()
                                        .text_meta()
                                        .text_color(palette.muted)
                                        .truncate()
                                        .child(format!(
                                            "{} · {}",
                                            card_kind_str_label(locale, &item.card.kind),
                                            I18n::format(
                                                locale,
                                                "learning",
                                                "reviewCount",
                                                &[(
                                                    "count",
                                                    Param::from(i64::from(item.card.review_count))
                                                )],
                                            )
                                        )),
                                ),
                        )
                        .when(!item.browse.tags.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_micro()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(palette.surface_alt)
                                    .text_color(palette.muted)
                                    .child(item.browse.tags.join(", ")),
                            )
                        }),
                );
            }
        }
    } else {
        table = table.child(
            div().p(px(12.0)).child(
                EmptyState::new(
                    "cards-disconnected-state",
                    I18n::t(locale, "learning", "title"),
                    I18n::t(locale, "learning", "description"),
                )
                .icon(ActionIconName::Cards)
                .palette(palette),
            ),
        );
    }

    let last_page = state.last_page();
    let on_page = cb.on_select_page.clone();
    let center_col = div()
        .v_flex()
        .flex_1()
        .min_w(px(340.0))
        .gap(px(10.0))
        .child(filter_bar)
        .when(state.show_tools_drawer, |this| this.child(tools_section))
        .when(state.selection.cap_hit(), |this| {
            this.child(
                StatusBanner::new(
                    "card-selection-cap-banner",
                    BannerKind::Warning,
                    format!("Selection limit reached ({CARD_SELECTION_LIMIT} cards maximum)."),
                )
                .palette(palette),
            )
        })
        .child(selection_bar)
        .child(table)
        .when(last_page > 1, |this| {
            this.child(
                div().h_flex().justify_end().child(
                    NumberedSearchPagination::new("cards-pagination", state.page, last_page)
                        .palette(palette)
                        .on_select(move |p, window, cx| on_page(p, window, cx)),
                ),
            )
        });

    // ── Resizable Splitter Handle (`CTX_SPLITTER`, persists `LocalUiState::card_preview_width`)
    let on_shrink = cb.on_adjust_preview_width.clone();
    let on_grow = cb.on_adjust_preview_width.clone();
    let splitter = div()
        .id("card-preview-splitter")
        .test_support()
        .key_context(CTX_SPLITTER)
        .track_focus(&state.splitter_focus)
        .v_flex()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .w(px(14.0))
        .cursor(CursorStyle::ResizeLeftRight)
        .child(
            ActionButton::new("card-preview-grow")
                .icon(ActionIconName::Back)
                .tooltip(I18n::t(locale, "card-search", "resizePreview"))
                .variant(ActionButtonVariant::Ghost)
                .dense_nav()
                .palette(palette)
                .on_click(move |_, window, cx| on_grow(40, window, cx)),
        )
        .child(
            div()
                .w(px(2.0))
                .h(px(64.0))
                .rounded(px(1.0))
                .bg(palette.border_strong),
        )
        .child(
            ActionButton::new("card-preview-shrink")
                .icon(ActionIconName::Next)
                .tooltip(I18n::t(locale, "card-search", "resizePreview"))
                .variant(ActionButtonVariant::Ghost)
                .dense_nav()
                .palette(palette)
                .on_click(move |_, window, cx| on_shrink(-40, window, cx)),
        );

    // ── Right Resizable Preview Pane
    let preview_width = state
        .preview_width
        .clamp(MIN_CARD_PREVIEW_WIDTH, MAX_CARD_PREVIEW_WIDTH);
    let mut preview_pane = div()
        .id("study-card-preview-pane")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .w(px(preview_width as f32))
        .flex_shrink_0()
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "learning",
                    "cardPreview",
                )))
                .child(
                    div()
                        .text_micro()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(format!("{preview_width}px")),
                ),
        );

    if let Some(item) = state.preview_item() {
        let card = &item.card;
        let card_id = card.id;
        let card_clone = card.clone();
        let on_reveal = cb.on_reveal_preview.clone();
        let on_pips = cb.on_open_pips_modal.clone();
        let on_score = cb.on_open_score_variants.clone();

        preview_pane = preview_pane
            .child(if card.kind == "anki" {
                state.preview_anki_view.clone().into_any_element()
            } else {
                div()
                    .h(px(300.0))
                    .w_full()
                    .child(state.preview_board_view.clone())
                    .into_any_element()
            })
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("preview-show-answer")
                            .icon(ActionIconName::Solution)
                            .label(I18n::t(locale, "learning", "reveal"))
                            .disabled(state.preview_answer_loading)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_reveal(card_id, window, cx)),
                    )
                    .when(card.kind != "anki" && card.position.is_some(), |this| {
                        this.child(
                            ActionButton::new("preview-pips-practice")
                                .label(I18n::t(locale, "card-practice", "pips"))
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_pips(card_clone.clone(), window, cx)
                                }),
                        )
                    })
                    .when(
                        matches!(card.kind.as_str(), "cube_offer" | "cube_response"),
                        |this| {
                            this.child(
                                ActionButton::new("preview-score-variants")
                                    .icon(ActionIconName::Double)
                                    .label(I18n::t(locale, "card-practice", "score"))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| on_score(card_id, window, cx)),
                            )
                        },
                    ),
            );

        if let Some(ans) = &state.preview_answer {
            let best_str = ans
                .solution
                .as_ref()
                .and_then(|s| {
                    s.decision
                        .as_ref()
                        .map(|d| d.best.as_str())
                        .or_else(|| s.candidates.first().map(|c| c.notation.as_str()))
                })
                .unwrap_or("");
            if !best_str.is_empty() {
                preview_pane = preview_pane.child(
                    div()
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(palette.surface_alt)
                        .text_body()
                        .text_color(palette.text)
                        .child(format!(
                            "{}: {}",
                            I18n::t(locale, "learning", "best"),
                            localize_decision(locale, best_str)
                        )),
                );
            }
            if !ans.notes.trim().is_empty() {
                preview_pane = preview_pane.child(
                    div()
                        .text_meta()
                        .text_color(palette.muted)
                        .child(ans.notes.clone()),
                );
            }
        }
    } else {
        preview_pane =
            preview_pane.child(div().text_body().text_color(palette.muted).child(I18n::t(
                locale,
                "learning",
                "previewHint",
            )));
    }

    let mut root = div()
        .id("study-flashcards-panel")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w_full()
        .child(toolbar);

    if let Some(status) = &state.transfer_status {
        root = root.child(
            StatusBanner::new(
                "flashcards-transfer-status",
                BannerKind::Success,
                status.clone(),
            )
            .palette(palette),
        );
    }
    if let Some(err) = &state.transfer_error {
        root = root.child(
            StatusBanner::new("flashcards-transfer-err", BannerKind::Error, err.clone())
                .palette(palette),
        );
    }
    if let Some(deck) = selected_deck {
        root = root.child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .text_meta()
                .text_color(palette.muted)
                .child(deck.name.clone())
                .child("·")
                .child(format_cards_count(locale, deck.card_count))
                .child("·")
                .child(format_due_count(locale, deck.due_count)),
        );
    }

    root.child(
        div()
            .h_flex()
            .items_start()
            .gap(px(12.0))
            .w_full()
            .child(deck_sidebar)
            .child(center_col)
            .child(splitter)
            .child(preview_pane),
    )
}

// ── Deck Library Dialogs (Deck CRUD, Batch Operations, Duplicates) ───────────

/// State entity for Deck Management dialog (rename / delete with 409 refresh).
pub struct DeckManageModalState {
    pub decks: Vec<Deck>,
    pub selected_id: Option<Uuid>,
    pub rename_input: Entity<InputState>,
    pub confirm_delete: bool,
    pub busy: bool,
    pub notice: Option<String>,
    pub error: Option<String>,
}

pub fn open_deck_management_dialog(
    state: Entity<DeckManageModalState>,
    on_rename: UiCallback4<Entity<DeckManageModalState>, Uuid, String, i64>,
    on_delete: UiCallback3<Entity<DeckManageModalState>, Uuid, i64>,
    on_refresh: UiCallback<Entity<DeckManageModalState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let st_body = state.clone();
    let config =
        ModalConfig::new(I18n::t(locale, "deck-library", "decks")).size(ModalSize::Standard);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = st_body.read(cx);
            let selected = st
                .selected_id
                .and_then(|id| st.decks.iter().find(|d| d.id == id))
                .cloned();

            let mut list = div().v_flex().gap(px(4.0));
            for deck in &st.decks {
                let is_sel = st.selected_id == Some(deck.id);
                let did = deck.id;
                let dname = deck.name.clone();
                let st_click = st_body.clone();
                list = list.child(
                    ActionButton::new(SharedString::from(format!("manage-deck-{did}")))
                        .label(I18n::format(
                            locale,
                            "deck-management",
                            "details",
                            &[
                                ("name", Param::from(deck.name.as_str())),
                                ("count", Param::from(deck.card_count)),
                            ],
                        ))
                        .active(is_sel)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            st_click.update(cx, |s, cx| {
                                s.selected_id = Some(did);
                                s.confirm_delete = false;
                                s.error = None;
                                s.rename_input
                                    .update(cx, |inp, cx| inp.set_value(dname.clone(), window, cx));
                                cx.notify();
                            });
                        }),
                );
            }

            let mut body = div()
                .id("deck-management-modal")
                .test_support()
                .v_flex()
                .gap(px(12.0))
                .child(list);

            if let Some(notice) = &st.notice {
                body = body.child(
                    StatusBanner::new("deck-manage-notice", BannerKind::Info, notice.clone())
                        .palette(p),
                );
            }
            if let Some(err) = &st.error {
                let on_ref = on_refresh.clone();
                let st_ref = st_body.clone();
                body = body.child(
                    StatusBanner::new("deck-manage-err", BannerKind::Error, err.clone())
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "deck-management", "refresh"),
                            move |_, window, cx| on_ref(st_ref.clone(), window, cx),
                        ),
                );
            }

            if let Some(deck) = selected {
                let did = deck.id;
                let ver = deck.version;
                let on_ren = on_rename.clone();
                let st_ren = st_body.clone();
                let on_del = on_delete.clone();
                let st_del = st_body.clone();
                let st_conf = st_body.clone();
                body = body
                    .child(
                        div()
                            .v_flex()
                            .gap(px(6.0))
                            .pt(px(8.0))
                            .border_t_1()
                            .border_color(p.border)
                            .child(div().text_label().text_color(p.text).child(I18n::t(
                                locale,
                                "deck-management",
                                "rename",
                            )))
                            .child(
                                div()
                                    .h_flex()
                                    .gap(px(8.0))
                                    .child(div().flex_1().child(
                                        Input::new(&st.rename_input).id("deck-rename-input"),
                                    ))
                                    .child(
                                        ActionButton::new("deck-rename-save")
                                            .label(I18n::t(locale, "deck-management", "save"))
                                            .primary()
                                            .disabled(st.busy)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                let name = st_ren
                                                    .read(cx)
                                                    .rename_input
                                                    .read(cx)
                                                    .value()
                                                    .trim()
                                                    .to_string();
                                                if !name.is_empty() {
                                                    on_ren(
                                                        st_ren.clone(),
                                                        did,
                                                        name,
                                                        ver,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(6.0))
                            .pt(px(8.0))
                            .border_t_1()
                            .border_color(p.border)
                            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                locale,
                                "deck-management",
                                "keep",
                            )))
                            .child(if st.confirm_delete {
                                ActionButton::new("deck-delete-confirm")
                                    .icon(ActionIconName::Delete)
                                    .label(I18n::format(
                                        locale,
                                        "deck-management",
                                        "confirm",
                                        &[
                                            ("name", Param::from(deck.name.as_str())),
                                            ("count", Param::from(deck.card_count)),
                                        ],
                                    ))
                                    .danger_solid()
                                    .disabled(st.busy)
                                    .palette(p)
                                    .on_click(move |_, window, cx| {
                                        on_del(st_del.clone(), did, ver, window, cx)
                                    })
                            } else {
                                ActionButton::new("deck-delete-arm")
                                    .icon(ActionIconName::Delete)
                                    .label(I18n::t(locale, "deck-management", "delete"))
                                    .danger()
                                    .disabled(st.busy)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        st_conf.update(cx, |s, cx| {
                                            s.confirm_delete = true;
                                            cx.notify();
                                        });
                                    })
                            }),
                    );
            }

            body.into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            ActionButton::new("deck-manage-close")
                .label(I18n::t(locale, "deck-management", "cancel"))
                .palette(p)
                .on_click(|_, window, cx| window.close_dialog(cx))
                .into_any_element()
        }),
        None,
    );
}

/// State entity for `DuplicateCardsDialog` merge workflow.
pub struct DuplicateCardsModalState {
    pub page: usize,
    pub data: Option<DuplicatePage>,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<String>,
    /// Selected `keep_id` per `duplicate_key`.
    pub keep_choice: BTreeMap<String, Uuid>,
}

pub fn open_duplicate_cards_dialog(
    state: Entity<DuplicateCardsModalState>,
    on_merge: UiCallback5<Entity<DuplicateCardsModalState>, Uuid, i64, Uuid, i64>,
    on_page: UiCallback2<Entity<DuplicateCardsModalState>, usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let st_body = state.clone();
    let config =
        ModalConfig::new(I18n::t(locale, "deck-library", "duplicates")).size(ModalSize::Wide);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = st_body.read(cx);
            let mut body = div()
                .id("duplicate-cards-modal")
                .test_support()
                .v_flex()
                .gap(px(10.0));

            if st.loading {
                return body
                    .child(
                        div()
                            .text_body()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "learning", "loading")),
                    )
                    .into_any_element();
            }
            if let Some(err) = &st.error {
                body = body.child(
                    StatusBanner::new("duplicates-err", BannerKind::Error, err.clone()).palette(p),
                );
            }
            let Some(data) = &st.data else {
                return body.into_any_element();
            };
            if data.items.is_empty() {
                return body
                    .child(div().text_body().text_color(p.muted).child(I18n::t(
                        locale,
                        "deck-library",
                        "empty",
                    )))
                    .into_any_element();
            }

            for (idx, dup) in data.items.iter().take(20).enumerate() {
                let mut row = div()
                    .v_flex()
                    .gap(px(6.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface_alt)
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_body()
                                    .text_color(p.text)
                                    .child(dup.card.prompt.clone()),
                            )
                            .child(div().text_meta().text_color(p.muted).child(
                                if dup.copies > 1 {
                                    I18n::format(
                                        locale,
                                        "deck-library",
                                        "exact",
                                        &[("count", Param::from(dup.copies))],
                                    )
                                } else {
                                    I18n::t(locale, "deck-library", "shared").to_string()
                                },
                            )),
                    );

                if let Some(key) = &dup.duplicate_key
                    && let Some(peers) = data.peers.get(key)
                    && peers.len() >= 2
                {
                    let keep_id = st.keep_choice.get(key).copied().unwrap_or(peers[0].id);
                    if let Some(keep_peer) = peers.iter().find(|x| x.id == keep_id)
                        && let Some(del_peer) = peers.iter().find(|x| x.id != keep_id)
                    {
                        let on_m = on_merge.clone();
                        let st_m = st_body.clone();
                        let (kid, kver, did, dver) = (
                            keep_peer.id,
                            keep_peer.version,
                            del_peer.id,
                            del_peer.version,
                        );
                        row = row.child(
                            ActionButton::new(SharedString::from(format!("dup-merge-{idx}")))
                                .icon(ActionIconName::Merge)
                                .label(I18n::t(locale, "deck-library", "delete"))
                                .danger()
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    on_m(st_m.clone(), kid, kver, did, dver, window, cx)
                                }),
                        );
                    }
                }
                body = body.child(row);
            }

            let last_page = data.total.unwrap_or(data.items.len() as i64).max(1) as usize;
            let total_pages = last_page.div_ceil(50).max(1);
            if total_pages > 1 {
                let on_p = on_page.clone();
                let st_p = st_body.clone();
                body = body.child(
                    NumberedSearchPagination::new("dups-pagination", st.page, total_pages)
                        .palette(p)
                        .on_select(move |pg, window, cx| on_p(st_p.clone(), pg, window, cx)),
                );
            }

            body.into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            ActionButton::new("dups-close")
                .label(I18n::t(locale, "recurring-mistakes", "close"))
                .palette(p)
                .on_click(|_, window, cx| window.close_dialog(cx))
                .into_any_element()
        }),
        None,
    );
}

// ── AnkiSyncOverview Modal (live push via subscribe_anki_sync_updates) ───────

/// State entity for `AnkiSyncOverview` modal.
pub struct AnkiSyncOverviewModalState {
    pub decks: Vec<LinkedAnkiDeck>,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub status_notice: Option<String>,
    pub only_open_errors: bool,
    pub expanded_history_deck: Option<Uuid>,
    pub history_rows: Vec<AnkiSyncHistoryEntry>,
    pub history_loading: bool,
    pub history_open_only: bool,
    pub retry_progress: Option<(usize, usize)>,
}

fn overview_status_label(locale: Locale, status: OverviewStatus) -> &'static str {
    let key = match status {
        OverviewStatus::Checking => "ankiChecking",
        OverviewStatus::Paused => "ankiPausedManually",
        OverviewStatus::Unreachable => "ankiPausedOffline",
        OverviewStatus::OtherProfile => "ankiPausedProfile",
        OverviewStatus::Waiting => "ankiAutoWaiting",
        OverviewStatus::Synced => "ankiUpToDate",
        OverviewStatus::NoMatches => "ankiAutoNoMatches",
        OverviewStatus::InvalidDeck => "ankiInvalidDeck",
        OverviewStatus::Conflict => "ankiSyncConflict",
        OverviewStatus::Timeout => "ankiSyncTimeout",
        OverviewStatus::AnkiError => "ankiConnectionError",
        OverviewStatus::InternalError => "ankiSyncInternalError",
        OverviewStatus::Failed => "ankiAutoFailed",
    };
    I18n::t(locale, "card-practice", key)
}

pub struct AnkiSyncOverviewCallbacks {
    pub on_sync_now: UiCallback<Uuid>,
    pub on_pause_resume: UiCallback2<Uuid, bool>, // true = pause
    pub on_toggle_history: UiCallback<Uuid>,
    pub on_retry_entry: UiCallback2<Uuid, Uuid>,
    pub on_retry_all_open: UiAction,
    pub on_refresh: UiAction,
}

pub fn open_anki_sync_overview_modal(
    state: Entity<AnkiSyncOverviewModalState>,
    cb: AnkiSyncOverviewCallbacks,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let st_body = state.clone();
    let config = ModalConfig::new(I18n::t(locale, "card-practice", "ankiOverview"))
        .size(ModalSize::ExtraWide);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = st_body.read(cx);

            let open_error_decks = st.decks.iter().filter(|d| d.open_errors > 0).count();
            let visible_decks: Vec<&LinkedAnkiDeck> = st
                .decks
                .iter()
                .filter(|d| !st.only_open_errors || d.open_errors > 0)
                .collect();

            let st_filter = st_body.clone();
            let on_retry_all = cb.on_retry_all_open.clone();
            let on_ref = cb.on_refresh.clone();

            let mut body = div()
                .id("anki-sync-overview-modal")
                .test_support()
                .v_flex()
                .gap(px(12.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("anki-sync-open-errors-filter")
                                .label(I18n::t(
                                    locale,
                                    "card-practice",
                                    "ankiOverviewOnlyDecksWithOpenErrors",
                                ))
                                .active(st.only_open_errors)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    st_filter.update(cx, |s, cx| {
                                        s.only_open_errors = !s.only_open_errors;
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            div()
                                .h_flex()
                                .gap(px(6.0))
                                .when(open_error_decks > 0, |this| {
                                    this.child(
                                        ActionButton::new("anki-sync-retry-all")
                                            .icon(ActionIconName::Refresh)
                                            .label(I18n::t(
                                                locale,
                                                "card-practice",
                                                "ankiOverviewRetryAll",
                                            ))
                                            .primary()
                                            .disabled(st.busy)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                on_retry_all(window, cx)
                                            }),
                                    )
                                })
                                .child(
                                    ActionButton::new("anki-sync-refresh")
                                        .icon(ActionIconName::Refresh)
                                        .variant(ActionButtonVariant::Ghost)
                                        .dense_nav()
                                        .palette(p)
                                        .on_click(move |_, window, cx| on_ref(window, cx)),
                                ),
                        ),
                );

            if let Some((done, total)) = st.retry_progress {
                body = body.child(div().text_meta().text_color(p.accent).child(I18n::format(
                    locale,
                    "card-practice",
                    "ankiOverviewRetryAllProgress",
                    &[
                        ("completed", Param::from(done as i64)),
                        ("total", Param::from(total as i64)),
                    ],
                )));
            }
            if let Some(notice) = &st.status_notice {
                body = body.child(
                    StatusBanner::new("anki-sync-notice", BannerKind::Success, notice.clone())
                        .palette(p),
                );
            }
            if let Some(err) = &st.error {
                body = body.child(
                    StatusBanner::new("anki-sync-err", BannerKind::Error, err.clone()).palette(p),
                );
            }

            if st.loading && st.decks.is_empty() {
                body = body.child(div().text_body().text_color(p.muted).child(I18n::t(
                    locale,
                    "card-practice",
                    "ankiOverviewLoading",
                )));
            } else if visible_decks.is_empty() {
                body = body.child(div().text_body().text_color(p.muted).child(I18n::t(
                    locale,
                    "card-practice",
                    if st.only_open_errors {
                        "ankiOverviewNoDecksWithOpenErrors"
                    } else {
                        "ankiOverviewEmpty"
                    },
                )));
            } else {
                for deck in visible_decks {
                    let did = deck.deck_id;
                    let paused = deck.paused;
                    let expanded = st.expanded_history_deck == Some(did);
                    let on_sync = cb.on_sync_now.clone();
                    let on_pause = cb.on_pause_resume.clone();
                    let on_hist = cb.on_toggle_history.clone();

                    let mut card = div()
                        .v_flex()
                        .gap(px(6.0))
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.surface_alt)
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .v_flex()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .text_body()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(p.text)
                                                .child(format!(
                                                    "{} · {}",
                                                    deck.deck_name, deck.profile
                                                )),
                                        )
                                        .child(
                                            div()
                                                .text_meta()
                                                .tabular_nums()
                                                .text_color(p.muted)
                                                .child(format!(
                                                    "{} · ↓{} ↑{} · {}: {}",
                                                    overview_status_label(locale, deck.status),
                                                    format_int(locale, deck.received_reviews),
                                                    format_int(locale, deck.sent_reviews),
                                                    I18n::t(
                                                        locale,
                                                        "card-practice",
                                                        "ankiOverviewOpenErrors"
                                                    ),
                                                    deck.open_errors
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .gap(px(6.0))
                                        .child(
                                            ActionButton::new(SharedString::from(format!(
                                                "anki-sync-now-{did}"
                                            )))
                                            .label(I18n::t(locale, "card-practice", "ankiSyncNow"))
                                            .disabled(st.busy)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                on_sync(did, window, cx)
                                            }),
                                        )
                                        .child(
                                            ActionButton::new(SharedString::from(format!(
                                                "anki-pause-{did}"
                                            )))
                                            .label(I18n::t(
                                                locale,
                                                "card-practice",
                                                if paused {
                                                    "ankiResumeSync"
                                                } else {
                                                    "ankiPauseSync"
                                                },
                                            ))
                                            .disabled(st.busy)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                on_pause(did, !paused, window, cx)
                                            }),
                                        )
                                        .child(
                                            ActionButton::new(SharedString::from(format!(
                                                "anki-hist-{did}"
                                            )))
                                            .label(I18n::t(locale, "card-practice", "ankiHistory"))
                                            .active(expanded)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                on_hist(did, window, cx)
                                            }),
                                        ),
                                ),
                        );

                    if expanded {
                        let st_hfilter = st_body.clone();
                        let mut hist_box = div()
                            .v_flex()
                            .gap(px(4.0))
                            .pt(px(6.0))
                            .border_t_1()
                            .border_color(p.border)
                            .child(
                                ActionButton::new("anki-hist-open-filter")
                                    .label(I18n::t(locale, "card-practice", "ankiHistoryOpenOnly"))
                                    .active(st.history_open_only)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        st_hfilter.update(cx, |s, cx| {
                                            s.history_open_only = !s.history_open_only;
                                            cx.notify();
                                        });
                                    }),
                            );
                        let rows: Vec<&AnkiSyncHistoryEntry> = st
                            .history_rows
                            .iter()
                            .filter(|h| {
                                !st.history_open_only
                                    || (h.outcome == SyncOutcome::Failed && h.resolved_at.is_none())
                            })
                            .take(15)
                            .collect();
                        if rows.is_empty() {
                            hist_box = hist_box.child(
                                div().text_meta().text_color(p.muted).child(I18n::t(
                                    locale,
                                    "card-practice",
                                    "ankiHistoryEmpty",
                                )),
                            );
                        } else {
                            for entry in rows {
                                let eid = entry.id;
                                let is_open_fail = entry.outcome == SyncOutcome::Failed
                                    && entry.resolved_at.is_none();
                                let on_retry = cb.on_retry_entry.clone();
                                hist_box = hist_box.child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .justify_between()
                                        .text_meta()
                                        .tabular_nums()
                                        .child(format!(
                                            "{} · {:?} · ↓{} ↑{} {}",
                                            format_datetime_short_local(locale, entry.finished_at),
                                            entry.outcome,
                                            entry.received_reviews,
                                            entry.sent_reviews,
                                            entry.error_detail.as_deref().unwrap_or("")
                                        ))
                                        .when(is_open_fail, |this| {
                                            this.child(
                                                ActionButton::new(SharedString::from(format!(
                                                    "anki-retry-entry-{eid}"
                                                )))
                                                .label(I18n::t(
                                                    locale,
                                                    "card-practice",
                                                    "ankiHistoryRetry",
                                                ))
                                                .disabled(st.busy)
                                                .palette(p)
                                                .on_click(move |_, window, cx| {
                                                    on_retry(did, eid, window, cx)
                                                }),
                                            )
                                        }),
                                );
                            }
                        }
                        card = card.child(hist_box);
                    }
                    body = body.child(card);
                }
            }

            body.into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            ActionButton::new("anki-sync-overview-close")
                .label(I18n::t(locale, "recurring-mistakes", "close"))
                .palette(p)
                .on_click(|_, window, cx| window.close_dialog(cx))
                .into_any_element()
        }),
        None,
    );
}
