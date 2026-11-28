//! Study → Recurring Mistakes sub-tab + interactive `RecurringMistakePractice` modal
//! + unanalyzed matches drawer.

use super::common::{
    ALL_GAME_PLANS, CheckerExerciseController, UiAction, UiCallback, UiCallback2, game_plan_label,
    mistake_kind_label, position_to_board_position, preview_for_notation,
};
use super::fsrs_review::render_practice_evaluation_card;
use crate::i18n::current_locale;
use crate::keybindings::CTX_STUDY_EXERCISE;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::decimal_input::{DecimalInput, DecimalInputConfig, decimal_separator_for_locale};
use crate::ui::filter_chips::{AppliedFilterChip, AppliedFilterChips};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use gpui_kit::{
    App, AppContext, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window, base::StyledExt,
    component::WindowExt, div, prelude::FluentBuilder, px,
};
use neondojo::databases::Database;
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::model::MatchSummary;
use neondojo::practice::evaluation::{CubeOfferAction, CubeResponseAction, PracticeAnswer};
use neondojo::recurring_mistakes::{
    MistakeAnswer, MistakeAttempt, MistakeKind, MistakePracticeQuestion, MistakeSource, Mistakes,
    MistakesQuery,
};
use neondojo_board::{BoardView, BoardViewMode};
use neondojo_i18n::{
    I18n, Locale, Param, format_decimal, format_int, format_percent, localize_decision,
    parse_localized_decimal,
};
use uuid::Uuid;

/// Interactive state for Study → Recurring Mistakes.
pub struct RecurringMistakesState {
    pub databases: Vec<Database>,
    pub database_id: Option<Uuid>,
    pub game_plan: Option<GamePlan>,
    pub min_loss_input: Entity<DecimalInput>,
    pub min_loss_canonical: String,
    pub data: Option<Mistakes>,
    pub loading: bool,
    pub error: Option<String>,
    pub unanalyzed: Vec<MatchSummary>,
    pub unanalyzed_open: bool,
    pub unanalyzed_loading: bool,
}

impl RecurringMistakesState {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let locale = current_locale(cx);
        let sep = decimal_separator_for_locale(locale.code());
        let cfg = DecimalInputConfig::new("mistakes-min-loss", 0.01)
            .min(0.0)
            .max(1000.0)
            .separator(sep)
            .width(96.0);
        let min_loss_input = cx.new(|cx| DecimalInput::new(cfg, "0.08", None, window, cx));
        Self {
            databases: Vec::new(),
            database_id: None,
            game_plan: None,
            min_loss_input,
            min_loss_canonical: "0.08".to_string(),
            data: None,
            loading: false,
            error: None,
            unanalyzed: Vec::new(),
            unanalyzed_open: false,
            unanalyzed_loading: false,
        }
    }

    pub fn parsed_min_loss(&self, cx: &App) -> Option<f64> {
        let raw = self.min_loss_input.read(cx).canonical_value();
        let val = parse_localized_decimal(Locale::En, raw)?;
        (val.is_finite() && (0.0..=1000.0).contains(&val)).then_some(val)
    }

    pub fn build_query(
        &self,
        player: &str,
        independent_only: bool,
        configured_player: Option<String>,
        cx: &App,
    ) -> MistakesQuery {
        MistakesQuery {
            player: (!player.trim().is_empty()).then(|| player.trim().to_string()),
            database_id: self.database_id,
            game_plan: self.game_plan,
            min_loss: self.parsed_min_loss(cx).or(Some(0.08)),
            independent_only: Some(independent_only),
            resolve: configured_player,
        }
    }
}

pub struct MistakesCallbacks {
    pub on_select_database: UiCallback<Option<Uuid>>,
    pub on_select_game_plan: UiCallback<Option<GamePlan>>,
    pub on_toggle_independent: UiAction,
    pub on_refresh: UiAction,
    pub on_practice_group: UiCallback2<GamePlan, MistakeKind>,
    pub on_toggle_unanalyzed: UiAction,
    pub on_open_match: UiCallback<Uuid>,
    pub on_analyze_matches: UiCallback<Vec<Uuid>>,
}

/// Renders Study → Recurring Mistakes (filters, unanalyzed matches drawer, group table).
pub fn render_recurring_mistakes(
    state: &RecurringMistakesState,
    player_picker: impl IntoElement,
    selected_player: &str,
    independent_only: bool,
    palette: NeonPalette,
    cb: MistakesCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let mut db_row = div()
        .h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child({
            let on_db = cb.on_select_database.clone();
            ActionButton::new("mistakes-db-all")
                .label(I18n::t(locale, "recurring-mistakes", "allDatabases"))
                .active(state.database_id.is_none())
                .palette(palette)
                .on_click(move |_, window, cx| on_db(None, window, cx))
        });
    for db in state.databases.iter().take(8) {
        let db_id = db.id;
        let on_db = cb.on_select_database.clone();
        db_row = db_row.child(
            ActionButton::new(SharedString::from(format!("mistakes-db-{db_id}")))
                .label(db.name.clone())
                .active(state.database_id == Some(db_id))
                .palette(palette)
                .on_click(move |_, window, cx| on_db(Some(db_id), window, cx)),
        );
    }

    let mut plan_row = div()
        .h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child({
            let on_gp = cb.on_select_game_plan.clone();
            ActionButton::new("mistakes-plan-any")
                .label(I18n::t(locale, "recurring-mistakes", "any"))
                .active(state.game_plan.is_none())
                .palette(palette)
                .on_click(move |_, window, cx| on_gp(None, window, cx))
        });
    for &plan in ALL_GAME_PLANS {
        let on_gp = cb.on_select_game_plan.clone();
        plan_row = plan_row.child(
            ActionButton::new(SharedString::from(format!("mistakes-plan-{plan:?}")))
                .label(game_plan_label(locale, plan))
                .active(state.game_plan == Some(plan))
                .palette(palette)
                .on_click(move |_, window, cx| on_gp(Some(plan), window, cx)),
        );
    }

    let on_indep = cb.on_toggle_independent.clone();
    let on_ref = cb.on_refresh.clone();
    let mut chips = Vec::new();
    if let Some(db_id) = state.database_id
        && let Some(db) = state.databases.iter().find(|d| d.id == db_id)
    {
        chips.push(
            AppliedFilterChip::new("db", I18n::t(locale, "recurring-mistakes", "database"))
                .value(db.name.clone()),
        );
    }
    if let Some(gp) = state.game_plan {
        chips.push(
            AppliedFilterChip::new("plan", I18n::t(locale, "game-plan", "title"))
                .value(game_plan_label(locale, gp)),
        );
    }
    if independent_only {
        chips.push(AppliedFilterChip::new(
            "independent",
            I18n::t(locale, "recurring-mistakes", "independentOnly"),
        ));
    }

    let on_rm_db = cb.on_select_database.clone();
    let on_rm_gp = cb.on_select_game_plan.clone();
    let on_rm_ind = cb.on_toggle_independent.clone();
    let on_clr_db = cb.on_select_database.clone();
    let on_clr_gp = cb.on_select_game_plan.clone();
    let on_clr_ind = cb.on_toggle_independent.clone();

    let filters_card = div()
        .id("study-mistakes-filters")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .v_flex()
                        .gap(px(2.0))
                        .child(div().text_panel().text_color(palette.text).child(I18n::t(
                            locale,
                            "recurring-mistakes",
                            "title",
                        )))
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "recurring-mistakes",
                            "coverage",
                        ))),
                )
                .child(
                    ActionButton::new("mistakes-refresh-btn")
                        .icon(ActionIconName::Refresh)
                        .label(I18n::t(locale, "recurring-mistakes", "refresh"))
                        .disabled(state.loading)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_ref(window, cx)),
                ),
        )
        .child(player_picker)
        .child(db_row)
        .child(plan_row)
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_label().text_color(palette.muted).child(I18n::t(
                            locale,
                            "recurring-mistakes",
                            "minLoss",
                        )))
                        .child(state.min_loss_input.clone()),
                )
                .child(
                    ActionButton::new("mistakes-independent-toggle")
                        .label(I18n::t(locale, "recurring-mistakes", "independentOnly"))
                        .active(independent_only)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_indep(window, cx)),
                ),
        )
        .when(!chips.is_empty(), |this| {
            this.child(
                AppliedFilterChips::new("mistakes-applied-chips", chips)
                    .palette(palette)
                    .on_remove(move |id, window, cx| match id {
                        "db" => on_rm_db(None, window, cx),
                        "plan" => on_rm_gp(None, window, cx),
                        "independent" => on_rm_ind(window, cx),
                        _ => {}
                    })
                    .on_clear_all(move |window, cx| {
                        on_clr_db(None, window, cx);
                        on_clr_gp(None, window, cx);
                        if independent_only {
                            on_clr_ind(window, cx);
                        }
                    }),
            )
        });

    let mut root = div()
        .id("study-mistakes-panel")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w_full()
        .child(filters_card);

    if let Some(err) = &state.error {
        let on_retry = cb.on_refresh.clone();
        root = root.child(
            StatusBanner::new("mistakes-err-banner", BannerKind::Error, err.clone())
                .palette(palette)
                .on_retry(
                    I18n::t(locale, "recurring-mistakes", "retry"),
                    move |_, window, cx| on_retry(window, cx),
                ),
        );
    }

    if state.loading && state.data.is_none() {
        return root.child(
            div()
                .p(px(20.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "recurring-mistakes", "loading")),
        );
    }

    if selected_player.trim().is_empty() {
        return root.child(
            EmptyState::new(
                "mistakes-choose-player",
                I18n::t(locale, "recurring-mistakes", "choose"),
                I18n::t(locale, "recurring-mistakes", "chooseHelp"),
            )
            .icon(ActionIconName::Filter)
            .palette(palette),
        );
    }

    let Some(data) = &state.data else {
        return root;
    };

    // Coverage & unanalyzed matches strip
    let on_un = cb.on_toggle_unanalyzed.clone();
    let coverage = format!(
        "{}: {} / {}",
        I18n::t(locale, "recurring-mistakes", "analyzed"),
        format_int(locale, data.analyzed_decisions as i64),
        format_int(locale, data.analyzed_matches as i64)
    );
    root = root.child(
        div()
            .h_flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .bg(palette.surface_alt)
            .child(
                div()
                    .id("mistakes-player-coverage")
                    .test_support()
                    .role(gpui_kit::Role::Label)
                    .aria_value(coverage.clone())
                    .text_meta()
                    .tabular_nums()
                    .text_color(palette.muted)
                    .child(coverage),
            )
            .when(data.unanalyzed_matches > 0, |this| {
                this.child(
                    ActionButton::new("mistakes-unanalyzed-toggle")
                        .icon(ActionIconName::Warning)
                        .label(I18n::format(
                            locale,
                            "study-overview",
                            "unanalyzed",
                            &[
                                ("missing", Param::from(data.unanalyzed_matches as i64)),
                                ("total", Param::from(data.player_matches as i64)),
                            ],
                        ))
                        .active(state.unanalyzed_open)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_un(window, cx)),
                )
            }),
    );

    // Unanalyzed Matches expandable list
    if state.unanalyzed_open {
        let ids: Vec<Uuid> = state.unanalyzed.iter().map(|m| m.id).collect();
        let on_analyze_all = cb.on_analyze_matches.clone();
        let mut un_box = div()
            .id("study-unanalyzed-matches-list")
            .test_support()
            .v_flex()
            .gap(px(6.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_panel().text_color(palette.text).child(format!(
                        "{} ({})",
                        I18n::t(locale, "recurring-mistakes", "matches"),
                        state.unanalyzed.len()
                    )))
                    .when(!ids.is_empty(), |this| {
                        this.child(
                            ActionButton::new("mistakes-analyze-all-unanalyzed")
                                .icon(ActionIconName::Analysis)
                                .label(I18n::t(locale, "app", "gameAnalysis.start"))
                                .primary()
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_analyze_all(ids.clone(), window, cx)
                                }),
                        )
                    }),
            );
        for (idx, m) in state.unanalyzed.iter().take(30).enumerate() {
            let mid = m.id;
            let on_open = cb.on_open_match.clone();
            un_box = un_box.child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .bg(palette.surface_alt)
                    .child(div().text_table().text_color(palette.text).child(format!(
                        "{} vs {} ({} pt)",
                        m.white, m.black, m.match_length
                    )))
                    .child(
                        ActionButton::new(SharedString::from(format!("unanalyzed-open-{idx}")))
                            .icon(ActionIconName::Open)
                            .label(I18n::t(locale, "workspace", "analysis"))
                            .variant(ActionButtonVariant::Ghost)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_open(mid, window, cx)),
                    ),
            );
        }
        root = root.child(un_box);
    }

    if data.groups.is_empty() {
        return root.child(
            EmptyState::new(
                "mistakes-empty",
                I18n::t(locale, "recurring-mistakes", "title"),
                I18n::t(locale, "recurring-mistakes", "empty"),
            )
            .icon(ActionIconName::Confirm)
            .palette(palette),
        );
    }

    // Mistake Groups Table
    let mut table = div()
        .id("study-mistakes-table")
        .test_support()
        .v_flex()
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .overflow_hidden()
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .bg(palette.surface_alt)
                .border_b_1()
                .border_color(palette.border)
                .text_table_heading()
                .text_color(palette.muted)
                .child(div().flex_1().child(I18n::t(locale, "game-plan", "title")))
                .child(
                    div()
                        .w(px(140.0))
                        .child(I18n::t(locale, "recurring-mistakes", "kind")),
                )
                .child(div().w(px(70.0)).text_right().child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "occurrences",
                )))
                .child(div().w(px(64.0)).text_right().child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "matches",
                )))
                .child(div().w(px(88.0)).text_right().child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "errorRate",
                )))
                .child(div().w(px(90.0)).text_right().child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "lossPer100",
                )))
                .child(div().w(px(110.0)).text_right().child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "trend",
                )))
                .child(div().w(px(92.0))),
        );

    for (idx, group) in data.groups.iter().enumerate() {
        let gp = group.game_plan;
        let mk = group.mistake_kind;
        let on_practice = cb.on_practice_group.clone();
        let (rate_txt, loss100_txt, trend_txt) = match &group.rates {
            Some(r) => (
                format_percent(locale, r.error_rate, 1),
                if r.small_sample {
                    I18n::t(locale, "recurring-mistakes", "smallSample").to_string()
                } else {
                    format_decimal(locale, r.loss_per_100, 2, 2)
                },
                r.trend
                    .as_ref()
                    .map(|t| {
                        format!(
                            "{} → {}",
                            format_percent(locale, t.earlier_error_rate, 0),
                            format_percent(locale, t.recent_error_rate, 0)
                        )
                    })
                    .unwrap_or_else(|| "—".into()),
            ),
            None => ("—".into(), "—".into(), "—".into()),
        };

        table = table.child(
            div()
                .id(SharedString::from(format!("mistakes-group-row-{idx}")))
                .test_support()
                .h_flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(palette.border)
                .text_table()
                .tabular_nums()
                .text_color(palette.text)
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::MEDIUM)
                        .child(game_plan_label(locale, gp)),
                )
                .child(div().w(px(140.0)).child(mistake_kind_label(locale, mk)))
                .child(
                    div()
                        .w(px(70.0))
                        .text_right()
                        .child(format_int(locale, group.count as i64)),
                )
                .child(
                    div()
                        .w(px(64.0))
                        .text_right()
                        .child(format_int(locale, group.match_count as i64)),
                )
                .child(div().w(px(88.0)).text_right().child(rate_txt))
                .child(div().w(px(90.0)).text_right().child(loss100_txt))
                .child(
                    div()
                        .w(px(110.0))
                        .text_right()
                        .text_color(palette.muted)
                        .child(trend_txt),
                )
                .child(
                    div().w(px(92.0)).h_flex().justify_end().child(
                        ActionButton::new(SharedString::from(format!("mistakes-practice-{idx}")))
                            .icon(ActionIconName::Play)
                            .label(I18n::t(locale, "recurring-mistakes", "practice"))
                            .palette(palette)
                            .on_click(move |_, window, cx| on_practice(gp, mk, window, cx)),
                    ),
                ),
        );
    }

    root.child(table)
}

// ── Interactive `RecurringMistakePractice` Modal ─────────────────────────────

pub struct MistakePracticeModalState {
    pub title: String,
    pub items: Vec<MistakePracticeQuestion>,
    pub index: usize,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub checker: Option<CheckerExerciseController>,
    pub selected_cube_offer: Option<CubeOfferAction>,
    pub selected_cube_response: Option<CubeResponseAction>,
    pub attempt: Option<MistakeAttempt>,
    pub revealed: Option<MistakeAnswer>,
    pub board_view: Entity<BoardView>,
}

impl MistakePracticeModalState {
    pub fn current(&self) -> Option<&MistakePracticeQuestion> {
        self.items.get(self.index)
    }

    pub fn load_current_question(&mut self, facade: Option<&BackendFacade>, cx: &mut App) {
        self.attempt = None;
        self.revealed = None;
        self.selected_cube_offer = None;
        self.selected_cube_response = None;
        self.error = None;
        if let Some(q) = self.items.get(self.index) {
            self.checker = (q.kind == "checker")
                .then(|| CheckerExerciseController::from_position(&q.position, facade));
        } else {
            self.checker = None;
        }
        self.sync_board(cx);
    }

    pub fn build_pending_answer(&self) -> Option<PracticeAnswer> {
        let q = self.current()?;
        match q.kind.as_str() {
            "checker" => {
                self.checker
                    .as_ref()
                    .filter(|c| c.is_complete())
                    .map(|c| PracticeAnswer::Checker {
                        steps: c.chosen_steps.clone(),
                    })
            }
            "cube_offer" => self
                .selected_cube_offer
                .map(|action| PracticeAnswer::CubeOffer { action }),
            "cube_response" => self
                .selected_cube_response
                .map(|action| PracticeAnswer::CubeResponse { action }),
            _ => None,
        }
    }

    pub fn sync_board(&self, cx: &mut App) {
        let Some(q) = self.current() else {
            return;
        };
        let mut bp = position_to_board_position(&q.position);
        let answered = self.attempt.is_some() || self.revealed.is_some();
        let interaction = if !answered && q.kind == "checker" {
            if let Some(ctrl) = &self.checker {
                bp.board = ctrl.current_board();
                Some(ctrl.interaction_state())
            } else {
                None
            }
        } else {
            None
        };
        let preview = if answered && q.kind == "checker" {
            let best = self
                .attempt
                .as_ref()
                .map(|a| a.evaluation.best.as_str())
                .or_else(|| self.revealed.as_ref().map(|r| r.best.as_str()));
            best.and_then(|n| preview_for_notation(&q.position, n))
        } else {
            None
        };
        let mode = if preview.is_some() {
            BoardViewMode::Arrows
        } else {
            BoardViewMode::Before
        };
        self.board_view.update(cx, |bv, cx| {
            bv.set_players(q.players[0].clone(), q.players[1].clone(), cx);
            bv.set_cube_offer_pending(q.kind == "cube_response", cx);
            bv.set_show_pip_counts(true, cx);
            bv.set_position(bp, cx);
            bv.set_interaction(interaction, cx);
            bv.set_preview(preview, mode, cx);
        });
    }
}

fn source_label(locale: Locale, source: Option<MistakeSource>) -> &'static str {
    let key = match source {
        Some(MistakeSource::Xg) => "xgSource",
        Some(MistakeSource::Bgblitz) => "bgblitzSource",
        Some(MistakeSource::Sgf) => "sgfSource",
        _ => "nativeSource",
    };
    I18n::t(locale, "recurring-mistakes", key)
}

pub fn open_mistake_practice_modal(
    state: Entity<MistakePracticeModalState>,
    on_check: UiCallback<Entity<MistakePracticeModalState>>,
    on_reveal: UiCallback<Entity<MistakePracticeModalState>>,
    on_next: UiCallback<Entity<MistakePracticeModalState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let title = state.read(cx).title.clone();
    let st_body = state.clone();
    let st_footer = state;
    let config = ModalConfig::new(I18n::t(locale, "recurring-mistakes", "practiceTitle"))
        .subtitle(title)
        .size(ModalSize::ExtraWide);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = st_body.read(cx);

            let mut body = div()
                .id("recurring-mistake-practice-modal")
                .test_support()
                .key_context(CTX_STUDY_EXERCISE)
                .v_flex()
                .gap(px(12.0));

            if st.loading {
                return body
                    .child(div().text_body().text_color(p.muted).child(I18n::t(
                        locale,
                        "recurring-mistakes",
                        "loadingPractice",
                    )))
                    .into_any_element();
            }
            if let Some(err) = &st.error {
                body = body.child(
                    StatusBanner::new("mistake-practice-err", BannerKind::Error, err.clone())
                        .palette(p),
                );
            }
            let Some(q) = st.current() else {
                let msg = if st.items.is_empty() {
                    I18n::t(locale, "recurring-mistakes", "noQuestions")
                } else {
                    I18n::t(locale, "recurring-mistakes", "complete")
                };
                return body
                    .child(div().text_body().text_color(p.muted).child(msg))
                    .into_any_element();
            };

            let answered = st.attempt.is_some() || st.revealed.is_some();
            let loc_line = I18n::format(
                locale,
                "recurring-mistakes",
                "location",
                &[
                    ("game", Param::from(q.reference.game_no)),
                    ("decision", Param::from(q.reference.decision_no)),
                ],
            );

            let mut controls = div().v_flex().gap(px(10.0)).w(px(340.0));
            controls = controls.child(div().text_meta().text_color(p.muted).child(format!(
                "{} / {} · {loc_line}",
                st.index + 1,
                st.items.len()
            )));

            if !answered {
                match q.kind.as_str() {
                    "checker" => {
                        let st_undo = st_body.clone();
                        let can_undo = st
                            .checker
                            .as_ref()
                            .is_some_and(|c| !c.chosen_steps.is_empty());
                        controls = controls.child(
                            ActionButton::new("mistake-practice-undo")
                                .icon(ActionIconName::Undo)
                                .label(I18n::t(locale, "app", "play.undo"))
                                .shortcut("Z")
                                .disabled(!can_undo || st.busy)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    st_undo.update(cx, |s, cx| {
                                        if let Some(c) = s.checker.as_mut() {
                                            c.undo_step();
                                        }
                                        s.sync_board(cx);
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    "cube_offer" => {
                        let st1 = st_body.clone();
                        let st2 = st_body.clone();
                        controls = controls.child(
                            div()
                                .h_flex()
                                .gap(px(8.0))
                                .child(
                                    ActionButton::new("mistake-cube-nodouble")
                                        .label(localize_decision(locale, "no double"))
                                        .active(matches!(
                                            st.selected_cube_offer,
                                            Some(CubeOfferAction::NoDouble)
                                        ))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            st1.update(cx, |s, cx| {
                                                s.selected_cube_offer =
                                                    Some(CubeOfferAction::NoDouble);
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    ActionButton::new("mistake-cube-double")
                                        .label(localize_decision(locale, "double"))
                                        .active(matches!(
                                            st.selected_cube_offer,
                                            Some(CubeOfferAction::Double)
                                        ))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            st2.update(cx, |s, cx| {
                                                s.selected_cube_offer =
                                                    Some(CubeOfferAction::Double);
                                                cx.notify();
                                            });
                                        }),
                                ),
                        );
                    }
                    "cube_response" => {
                        let st1 = st_body.clone();
                        let st2 = st_body.clone();
                        controls = controls.child(
                            div()
                                .h_flex()
                                .gap(px(8.0))
                                .child(
                                    ActionButton::new("mistake-cube-take")
                                        .label(localize_decision(locale, "take"))
                                        .active(matches!(
                                            st.selected_cube_response,
                                            Some(CubeResponseAction::Take)
                                        ))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            st1.update(cx, |s, cx| {
                                                s.selected_cube_response =
                                                    Some(CubeResponseAction::Take);
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    ActionButton::new("mistake-cube-drop")
                                        .label(localize_decision(locale, "drop"))
                                        .active(matches!(
                                            st.selected_cube_response,
                                            Some(CubeResponseAction::Drop)
                                        ))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            st2.update(cx, |s, cx| {
                                                s.selected_cube_response =
                                                    Some(CubeResponseAction::Drop);
                                                cx.notify();
                                            });
                                        }),
                                ),
                        );
                    }
                    _ => {}
                }

                let can_check = st.build_pending_answer().is_some() && !st.busy;
                let cb_chk = on_check.clone();
                let cb_rev = on_reveal.clone();
                let st_chk = st_body.clone();
                let st_rev = st_body.clone();
                controls = controls.child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("mistake-practice-check")
                                .icon(ActionIconName::Confirm)
                                .label(I18n::t(locale, "learning", "checkAnswer"))
                                .primary()
                                .disabled(!can_check)
                                .palette(p)
                                .on_click(move |_, window, cx| cb_chk(st_chk.clone(), window, cx)),
                        )
                        .child(
                            ActionButton::new("mistake-practice-reveal")
                                .icon(ActionIconName::Solution)
                                .label(I18n::t(locale, "recurring-mistakes", "reveal"))
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| cb_rev(st_rev.clone(), window, cx)),
                        ),
                );
            } else {
                if let Some(att) = &st.attempt {
                    controls = controls.child(render_practice_evaluation_card(
                        "mistake-practice-eval",
                        &att.evaluation,
                        p,
                        cx,
                    ));
                }
                let orig = st
                    .attempt
                    .as_ref()
                    .map(|a| &a.original)
                    .or(st.revealed.as_ref());
                if let Some(ans) = orig {
                    controls = controls.child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(p.surface_alt)
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "recurring-mistakes",
                                "originalAnalysis",
                            )))
                            .child(div().text_body().text_color(p.text).child(format!(
                                "{}: {} · {}: {}",
                                I18n::t(locale, "recurring-mistakes", "best"),
                                localize_decision(locale, &ans.best),
                                I18n::t(locale, "recurring-mistakes", "played"),
                                localize_decision(locale, &ans.played)
                            )))
                            .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                                format!(
                                    "Δ {} EMG · {} ({})",
                                    format_decimal(locale, ans.equity_loss, 3, 3),
                                    source_label(locale, ans.source),
                                    ans.depth.as_deref().unwrap_or_else(|| I18n::t(
                                        locale,
                                        "recurring-mistakes",
                                        "unknownDepth"
                                    ))
                                ),
                            )),
                    );
                }
            }

            body.child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(14.0))
                    .child(div().flex_1().h(px(360.0)).child(st.board_view.clone()))
                    .child(controls),
            )
            .into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = st_footer.read(cx);
            let answered = st.attempt.is_some() || st.revealed.is_some();
            let has_more = st.index + 1 < st.items.len();
            let st_next = st_footer.clone();
            let cb_next = on_next.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .when(answered, |this| {
                    this.child(
                        ActionButton::new("mistake-practice-next")
                            .icon(ActionIconName::Next)
                            .label(I18n::t(
                                locale,
                                "recurring-mistakes",
                                if has_more { "next" } else { "finish" },
                            ))
                            .primary()
                            .palette(p)
                            .on_click(move |_, window, cx| cb_next(st_next.clone(), window, cx)),
                    )
                })
                .child(
                    ActionButton::new("mistake-practice-close")
                        .label(I18n::t(locale, "recurring-mistakes", "close"))
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .into_any_element()
        }),
        None,
    );
}
