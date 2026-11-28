//! Study → Overview sub-tab: next-action recommender, today's due/new breakdown,
//! editable `new_cards_per_day` (`0..=1000`), top-3 recurring mistake groups,
//! and player / `independent_only` filter persisted in `LocalUiState::study`.

use super::common::{
    MAX_NEW_CARDS_PER_DAY, UiAction, UiCallback, UiCallback2, clamp_new_cards_per_day,
    format_due_count, game_plan_label, mistake_kind_label, parse_and_clamp_new_cards_per_day,
};
use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, AppContext, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::game_plan::GamePlan;
use neondojo::learning::{LearningSummary, StudyDeck};
use neondojo::recurring_mistakes::{MistakeGroup, MistakeKind, MistakeRates, Mistakes};
use neondojo_i18n::{I18n, Param, format_datetime_medium_local, format_decimal, format_int};
use std::cmp::Ordering;
use uuid::Uuid;

/// Card queue availability input to [`recommend_study_action`].
#[derive(Debug, Clone)]
pub enum OverviewCardsInput<'a> {
    Loading,
    Error,
    Ready { deck: Option<&'a StudyDeck> },
}

/// Recurring mistakes availability input to [`recommend_study_action`].
#[derive(Clone)]
pub enum OverviewMistakesInput<'a> {
    Loading,
    Error,
    ChoosePlayer,
    Ready { group: Option<&'a MistakeGroup> },
}

/// Recommended next study step matching `web/src/workspaces/study/study-overview.ts`.
#[derive(Debug, Clone, PartialEq)]
pub enum RecommendedStudyAction {
    Deck {
        deck_id: Uuid,
        deck_name: String,
    },
    Mistake {
        game_plan: GamePlan,
        mistake_kind: MistakeKind,
        match_count: usize,
        decision_count: usize,
        loss_per_100: f64,
    },
    ChoosePlayer,
    OpenCards,
    Loading,
    Unavailable,
}

/// Matches configured player name case-insensitively against `players` when unique.
pub fn default_study_player(configured: &str, players: &[String]) -> String {
    let needle = configured.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return String::new();
    }
    let matches: Vec<&String> = players
        .iter()
        .filter(|p| p.trim().to_ascii_lowercase() == needle)
        .collect();
    if matches.len() == 1 {
        matches[0].clone()
    } else {
        String::new()
    }
}

fn ranked_loss(group: &MistakeGroup) -> f64 {
    match &group.rates {
        Some(rates) if !rates.small_sample => rates.loss_per_100,
        _ => -1.0,
    }
}

/// Ranks recurring mistake groups spanning at least 2 matches by `loss_per_100`
/// (excluding small samples), then `total_loss`, `match_count`, `count`, and key.
pub fn rank_mistake_groups(groups: &[MistakeGroup]) -> Vec<&MistakeGroup> {
    let mut eligible: Vec<&MistakeGroup> = groups.iter().filter(|g| g.match_count >= 2).collect();
    eligible.sort_by(|a, b| {
        ranked_loss(b)
            .total_cmp(&ranked_loss(a))
            .then_with(|| b.total_loss.total_cmp(&a.total_loss))
            .then_with(|| b.match_count.cmp(&a.match_count))
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| {
                let ak = format!("{:?}:{:?}", a.game_plan, a.mistake_kind);
                let bk = format!("{:?}:{:?}", b.game_plan, b.mistake_kind);
                ak.cmp(&bk)
            })
            .then(Ordering::Equal)
    });
    eligible
}

/// Pure next-action recommender matching `recommendStudyAction()` in `study-overview.ts`.
pub fn recommend_study_action(
    cards: OverviewCardsInput<'_>,
    mistakes: OverviewMistakesInput<'_>,
) -> RecommendedStudyAction {
    if let OverviewCardsInput::Ready { deck: Some(deck) } = cards {
        return RecommendedStudyAction::Deck {
            deck_id: deck.id,
            deck_name: deck.name.clone(),
        };
    }
    if matches!(cards, OverviewCardsInput::Loading)
        || matches!(mistakes, OverviewMistakesInput::Loading)
    {
        return RecommendedStudyAction::Loading;
    }
    if let OverviewMistakesInput::Ready { group: Some(group) } = mistakes
        && group.match_count >= 2
        && let Some(MistakeRates {
            small_sample: false,
            loss_per_100,
            decision_count,
            ..
        }) = group.rates
    {
        return RecommendedStudyAction::Mistake {
            game_plan: group.game_plan,
            mistake_kind: group.mistake_kind,
            match_count: group.match_count,
            decision_count,
            loss_per_100,
        };
    }
    if matches!(mistakes, OverviewMistakesInput::ChoosePlayer) {
        return RecommendedStudyAction::ChoosePlayer;
    }
    if matches!(cards, OverviewCardsInput::Error)
        || matches!(mistakes, OverviewMistakesInput::Error)
    {
        return RecommendedStudyAction::Unavailable;
    }
    RecommendedStudyAction::OpenCards
}

/// Interactive state for Study → Overview.
pub struct StudyOverviewState {
    pub summary: Option<LearningSummary>,
    pub summary_loading: bool,
    pub summary_error: Option<String>,
    pub new_cards_per_day: i32,
    pub new_cards_input: Entity<InputState>,
    pub saving_preferences: bool,
    pub preferences_notice: Option<String>,
    pub mistakes: Option<Mistakes>,
    pub mistakes_loading: bool,
    pub mistakes_error: Option<String>,
}

impl StudyOverviewState {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let default_limit = 20;
        let new_cards_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("20")
                .default_value(default_limit.to_string())
        });
        Self {
            summary: None,
            summary_loading: false,
            summary_error: None,
            new_cards_per_day: default_limit,
            new_cards_input,
            saving_preferences: false,
            preferences_notice: None,
            mistakes: None,
            mistakes_loading: false,
            mistakes_error: None,
        }
    }

    pub fn sync_new_cards_input(&mut self, value: i32, window: &mut Window, cx: &mut App) {
        let clamped = clamp_new_cards_per_day(value);
        self.new_cards_per_day = clamped;
        self.new_cards_input.update(cx, |state, cx| {
            state.set_value(clamped.to_string(), window, cx);
        });
    }

    pub fn draft_new_cards_per_day(&self, cx: &App) -> Option<i32> {
        parse_and_clamp_new_cards_per_day(self.new_cards_input.read(cx).value().as_ref())
    }
}

/// Callbacks wired from `StudyWorkspace` into the Overview renderer.
pub struct OverviewCallbacks {
    pub on_study_deck: UiCallback<Uuid>,
    pub on_open_flashcards: UiAction,
    pub on_open_mistakes: UiAction,
    pub on_practice_mistake: UiCallback2<GamePlan, MistakeKind>,
    pub on_save_new_cards_per_day: UiCallback<i32>,
    pub on_toggle_independent: UiAction,
    pub on_refresh: UiAction,
}

/// Renders Study → Overview.
#[allow(clippy::too_many_arguments)]
pub fn render_study_overview(
    state: &StudyOverviewState,
    player_picker: impl IntoElement,
    selected_player: &str,
    independent_only: bool,
    palette: NeonPalette,
    callbacks: OverviewCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);

    let ranked = state
        .mistakes
        .as_ref()
        .map(|m| rank_mistake_groups(&m.groups))
        .unwrap_or_default();
    let top_three: Vec<&MistakeGroup> = ranked.iter().copied().take(3).collect();

    let cards_input = if state.summary_loading && state.summary.is_none() {
        OverviewCardsInput::Loading
    } else if state.summary_error.is_some() && state.summary.is_none() {
        OverviewCardsInput::Error
    } else {
        OverviewCardsInput::Ready {
            deck: state.summary.as_ref().and_then(|s| s.next_deck.as_ref()),
        }
    };

    let mistakes_input = if state.mistakes_loading && state.mistakes.is_none() {
        OverviewMistakesInput::Loading
    } else if state.mistakes_error.is_some() && state.mistakes.is_none() {
        OverviewMistakesInput::Error
    } else if selected_player.trim().is_empty() {
        OverviewMistakesInput::ChoosePlayer
    } else {
        OverviewMistakesInput::Ready {
            group: top_three.first().copied(),
        }
    };

    let recommendation = recommend_study_action(cards_input, mistakes_input);

    // ── Section 1: Next Action Recommender ──────────────────────────────────
    let mut next_card = div()
        .id("study-overview-next-action")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(16.0))
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
                    "study-overview",
                    "next",
                )))
                .child({
                    let on_refresh = callbacks.on_refresh.clone();
                    ActionButton::new("study-overview-refresh")
                        .icon(ActionIconName::Refresh)
                        .tooltip(I18n::t(locale, "recurring-mistakes", "refresh"))
                        .variant(ActionButtonVariant::Ghost)
                        .dense_nav()
                        .palette(palette)
                        .on_click(move |_, window, cx| on_refresh(window, cx))
                }),
        );

    match recommendation {
        RecommendedStudyAction::Deck { deck_id, deck_name } => {
            let on_study = callbacks.on_study_deck.clone();
            next_card = next_card
                .child(
                    div()
                        .text_body()
                        .text_color(palette.muted)
                        .child(I18n::format(
                            locale,
                            "study-overview",
                            "deck",
                            &[("name", Param::from(deck_name.as_str()))],
                        )),
                )
                .child(
                    div().h_flex().gap(px(8.0)).child(
                        ActionButton::new("study-overview-study-due")
                            .icon(ActionIconName::Cards)
                            .label(I18n::t(locale, "study-overview", "studyDue"))
                            .primary()
                            .palette(palette)
                            .on_click(move |_, window, cx| on_study(deck_id, window, cx)),
                    ),
                );
        }
        RecommendedStudyAction::Mistake {
            game_plan,
            mistake_kind,
            match_count,
            decision_count,
            loss_per_100,
        } => {
            let on_practice = callbacks.on_practice_mistake.clone();
            let group_title = format!(
                "{} · {}",
                game_plan_label(locale, game_plan),
                mistake_kind_label(locale, mistake_kind)
            );
            next_card = next_card
                .child(
                    div()
                        .text_subject()
                        .text_color(palette.text)
                        .child(group_title),
                )
                .child(div().text_meta().text_color(palette.muted).child(format!(
                    "{} · {}",
                    I18n::format(
                        locale,
                        "study-overview",
                        "groupMatches",
                        &[("count", Param::from(match_count as i64))],
                    ),
                    I18n::format(
                        locale,
                        "study-overview",
                        "groupLoss",
                        &[
                            (
                                "loss",
                                Param::from(format_decimal(locale, loss_per_100, 2, 2)),
                            ),
                            ("count", Param::from(decision_count as i64)),
                        ],
                    ),
                )))
                .child(
                    div().h_flex().gap(px(8.0)).child(
                        ActionButton::new("study-overview-practice-recommended")
                            .icon(ActionIconName::Play)
                            .label(I18n::t(locale, "study-overview", "practiceGroup"))
                            .primary()
                            .palette(palette)
                            .on_click(move |_, window, cx| {
                                on_practice(game_plan, mistake_kind, window, cx)
                            }),
                    ),
                );
        }
        RecommendedStudyAction::ChoosePlayer => {
            next_card =
                next_card.child(div().text_body().text_color(palette.muted).child(I18n::t(
                    locale,
                    "recurring-mistakes",
                    "chooseHelp",
                )));
        }
        RecommendedStudyAction::Loading => {
            next_card =
                next_card.child(div().text_body().text_color(palette.muted).child(I18n::t(
                    locale,
                    "study-overview",
                    "loading",
                )));
        }
        RecommendedStudyAction::Unavailable => {
            next_card = next_card.child(
                div()
                    .text_body()
                    .text_color(palette.warning_text)
                    .child(I18n::t(locale, "study-overview", "unavailable")),
            );
        }
        RecommendedStudyAction::OpenCards => {
            let on_open_cards = callbacks.on_open_flashcards.clone();
            next_card = next_card
                .child(div().text_body().text_color(palette.muted).child(I18n::t(
                    locale,
                    "study-overview",
                    "emptyCards",
                )))
                .child(
                    div().h_flex().gap(px(8.0)).child(
                        ActionButton::new("study-overview-open-cards")
                            .icon(ActionIconName::Cards)
                            .label(I18n::t(locale, "study-overview", "openCards"))
                            .palette(palette)
                            .on_click(move |_, window, cx| on_open_cards(window, cx)),
                    ),
                );
        }
    }

    // ── Section 2: Today's Due/New Breakdown & Editable new_cards_per_day ───
    let mut cards_card = div()
        .id("study-overview-due-card")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .flex_1()
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "study-overview",
                    "dueCards",
                )))
                .child({
                    let on_open_cards = callbacks.on_open_flashcards.clone();
                    ActionButton::new("study-overview-cards-link")
                        .label(I18n::t(locale, "study-overview", "openCards"))
                        .variant(ActionButtonVariant::Ghost)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_open_cards(window, cx))
                }),
        );

    if let Some(err) = &state.summary_error {
        cards_card = cards_card.child(
            StatusBanner::new("study-overview-summary-err", BannerKind::Error, err.clone())
                .palette(palette),
        );
    } else if let Some(summary) = &state.summary {
        let waiting = (summary.new_count - summary.new_available_today).max(0);
        cards_card = cards_card
            .child(
                div()
                    .h_flex()
                    .items_baseline()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_page()
                            .tabular_nums()
                            .text_color(palette.text)
                            .child(format_int(locale, summary.today_count)),
                    )
                    .child(
                        div()
                            .text_body()
                            .tabular_nums()
                            .text_color(palette.muted)
                            .child(I18n::format(
                                locale,
                                "study-overview",
                                "todayBreakdown",
                                &[
                                    (
                                        "due",
                                        Param::from(format_int(locale, summary.review_due_count)),
                                    ),
                                    (
                                        "fresh",
                                        Param::from(format_int(
                                            locale,
                                            summary.new_available_today,
                                        )),
                                    ),
                                ],
                            )),
                    ),
            )
            .when(waiting > 0, |this| {
                this.child(
                    div()
                        .text_meta()
                        .text_color(palette.muted)
                        .child(I18n::format(
                            locale,
                            "study-overview",
                            "newWaiting",
                            &[("count", Param::from(waiting))],
                        )),
                )
            })
            .when(summary.today_count == 0, |this| {
                let note = match summary.next_due_at {
                    Some(dt) => I18n::format(
                        locale,
                        "study-overview",
                        "nextDue",
                        &[(
                            "date",
                            Param::from(format_datetime_medium_local(locale, dt)),
                        )],
                    ),
                    None if summary.card_count == 0 => {
                        I18n::t(locale, "study-overview", "emptyCards").to_string()
                    }
                    None => I18n::t(locale, "study-overview", "noDue").to_string(),
                };
                this.child(div().text_meta().text_color(palette.muted).child(note))
            });
    } else {
        cards_card = cards_card.child(
            div()
                .text_body()
                .text_color(palette.muted)
                .child(format_due_count(locale, 0)),
        );
    }

    let draft_limit = state.draft_new_cards_per_day(cx);
    let save_enabled =
        !state.saving_preferences && draft_limit.is_some_and(|v| v != state.new_cards_per_day);
    let save_cb = callbacks.on_save_new_cards_per_day.clone();
    let input_for_save = state.new_cards_input.clone();

    cards_card = cards_card.child(
        div()
            .v_flex()
            .gap(px(6.0))
            .pt(px(10.0))
            .border_t_1()
            .border_color(palette.border)
            .child(div().text_label().text_color(palette.text).child(format!(
                "{} (0–{MAX_NEW_CARDS_PER_DAY})",
                I18n::t(locale, "study-overview", "newPerDay")
            )))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div().w(px(110.0)).tabular_nums().child(
                            Input::new(&state.new_cards_input).id("study-new-cards-per-day"),
                        ),
                    )
                    .child(
                        ActionButton::new("study-save-new-cards-per-day")
                            .icon(ActionIconName::Save)
                            .label(I18n::t(locale, "deck-management", "save"))
                            .disabled(!save_enabled)
                            .palette(palette)
                            .on_click(move |_, window, cx| {
                                let raw = input_for_save.read(cx).value();
                                if let Some(val) = parse_and_clamp_new_cards_per_day(raw.as_ref()) {
                                    save_cb(val, window, cx);
                                }
                            }),
                    ),
            )
            .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                locale,
                "study-overview",
                "newPerDayHint",
            )))
            .when_some(state.preferences_notice.clone(), |this, notice| {
                this.child(
                    div()
                        .text_meta()
                        .text_color(palette.success_text)
                        .child(notice),
                )
            }),
    );

    // ── Section 3: Top-3 Recurring Mistake Groups + Player & Independent Filter
    let on_toggle_indep = callbacks.on_toggle_independent.clone();
    let indep_toggle = div()
        .id("study-overview-independent-toggle")
        .test_support()
        .h_flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .on_click(move |_, window, cx| on_toggle_indep(window, cx))
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(if independent_only {
                    palette.accent
                } else {
                    palette.control_border
                })
                .bg(if independent_only {
                    palette.accent
                } else {
                    palette.surface
                })
                .h_flex()
                .items_center()
                .justify_center()
                .text_micro()
                .text_color(palette.accent_text)
                .child(if independent_only { "✓" } else { "" }),
        )
        .child(div().text_label().text_color(palette.text).child(I18n::t(
            locale,
            "recurring-mistakes",
            "independentOnly",
        )));

    let mut mistakes_card = div()
        .id("study-overview-mistakes-card")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .flex_1()
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_panel()
                        .text_color(palette.text)
                        .child(I18n::t(locale, "study", "mistakes")),
                )
                .child({
                    let on_open_mistakes = callbacks.on_open_mistakes.clone();
                    ActionButton::new("study-overview-open-mistakes")
                        .label(I18n::t(locale, "study-overview", "openMistakes"))
                        .variant(ActionButtonVariant::Ghost)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_open_mistakes(window, cx))
                }),
        )
        .child(player_picker)
        .child(indep_toggle);

    if let Some(err) = &state.mistakes_error {
        mistakes_card = mistakes_card.child(
            StatusBanner::new(
                "study-overview-mistakes-err",
                BannerKind::Error,
                err.clone(),
            )
            .palette(palette),
        );
    } else if selected_player.trim().is_empty() {
        mistakes_card =
            mistakes_card.child(div().text_body().text_color(palette.muted).child(I18n::t(
                locale,
                "recurring-mistakes",
                "chooseHelp",
            )));
    } else if let Some(data) = &state.mistakes {
        let coverage = I18n::format(
            locale,
            "study-overview",
            "coverage",
            &[
                ("decisions", Param::from(data.analyzed_decisions as i64)),
                ("matches", Param::from(data.analyzed_matches as i64)),
            ],
        );
        mistakes_card = mistakes_card.child(
            div()
                .id("study-overview-player-coverage")
                .test_support()
                .role(gpui_kit::Role::Label)
                .aria_value(coverage.clone())
                .text_meta()
                .text_color(palette.muted)
                .child(coverage),
        );
        if data.unanalyzed_matches > 0 {
            mistakes_card =
                mistakes_card.child(div().text_meta().text_color(palette.warning_text).child(
                    I18n::format(
                        locale,
                        "study-overview",
                        "unanalyzed",
                        &[
                            ("missing", Param::from(data.unanalyzed_matches as i64)),
                            ("total", Param::from(data.player_matches as i64)),
                        ],
                    ),
                ));
        }

        if top_three.is_empty() {
            mistakes_card =
                mistakes_card.child(div().text_body().text_color(palette.muted).child(I18n::t(
                    locale,
                    "study-overview",
                    "noRecurring",
                )));
        } else {
            for (idx, group) in top_three.iter().enumerate() {
                let gp = group.game_plan;
                let mk = group.mistake_kind;
                let on_practice = callbacks.on_practice_mistake.clone();
                let loss_info = match &group.rates {
                    Some(rates) if !rates.small_sample => I18n::format(
                        locale,
                        "study-overview",
                        "groupLoss",
                        &[
                            (
                                "loss",
                                Param::from(format_decimal(locale, rates.loss_per_100, 2, 2)),
                            ),
                            ("count", Param::from(rates.decision_count as i64)),
                        ],
                    ),
                    _ => format!(
                        "{} · {}",
                        I18n::t(locale, "recurring-mistakes", "smallSample"),
                        format_decimal(locale, group.total_loss, 3, 3)
                    ),
                };

                mistakes_card = mistakes_card.child(
                    div()
                        .id(SharedString::from(format!(
                            "study-overview-top-group-{idx}"
                        )))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.0))
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.border)
                        .bg(palette.surface_alt)
                        .child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_body()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(palette.text)
                                        .child(format!(
                                            "{} · {}",
                                            game_plan_label(locale, gp),
                                            mistake_kind_label(locale, mk)
                                        )),
                                )
                                .child(
                                    div()
                                        .text_meta()
                                        .tabular_nums()
                                        .text_color(palette.muted)
                                        .child(format!(
                                            "{} · {loss_info}",
                                            I18n::format(
                                                locale,
                                                "study-overview",
                                                "groupMatches",
                                                &[("count", Param::from(group.match_count as i64))],
                                            ),
                                        )),
                                ),
                        )
                        .child(
                            ActionButton::new(SharedString::from(format!(
                                "study-overview-practice-top-{idx}"
                            )))
                            .icon(ActionIconName::Play)
                            .label(I18n::t(locale, "recurring-mistakes", "practice"))
                            .palette(palette)
                            .on_click(move |_, window, cx| on_practice(gp, mk, window, cx)),
                        ),
                );
            }
        }
    }

    div()
        .id("study-overview-panel")
        .test_support()
        .v_flex()
        .gap(px(16.0))
        .w_full()
        .child(next_card)
        .child(
            div()
                .h_flex()
                .items_start()
                .gap(px(16.0))
                .w_full()
                .child(cards_card)
                .child(mistakes_card),
        )
}
