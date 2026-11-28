//! Shared cube decision context, outcomes table, 6-chance table, and
//! Kazaross-XG2 take-point view (`CubeDecisionDetails.tsx` & `cube-context-api.ts`).

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, Styled, TestSupportExt, Window, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::cube_context::{CubeContext, TakePointBasis, cube_context};
use neondojo_board::decode_match_context;
use neondojo_i18n::{I18n, Param, format_equity, format_percent, localize_decision};
use std::rc::Rc;

type CalculateChancesHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Unit of cube outcome values.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CubeEquityUnit {
    #[default]
    NormalizedCubeful,
    MatchWinningChance,
}

/// The three cube actions / outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeOutcome {
    NoDouble,
    DoubleTake,
    DoublePass,
}

/// Saved or live cube evaluation values belonging to the deciding player.
#[derive(Debug, Clone, PartialEq)]
pub struct CubeEvaluationData {
    pub no_double: f64,
    pub double_take: f64,
    pub double_pass: f64,
    pub unit: CubeEquityUnit,
    /// Optional 6 cubeless probabilities: `[win, gammon, bg, opp_win, opp_gammon, opp_bg]`.
    pub probabilities: Option<[f64; 6]>,
}

impl CubeEvaluationData {
    pub fn value(&self, outcome: CubeOutcome) -> f64 {
        match outcome {
            CubeOutcome::NoDouble => self.no_double,
            CubeOutcome::DoubleTake => self.double_take,
            CubeOutcome::DoublePass => self.double_pass,
        }
    }
}

/// True when the opponent would pass a double, yet playing on is worth more
/// than cashing. Matches `tooGoodToDouble` in `cube-context-api.ts`.
pub fn too_good_to_double(evaluation: &CubeEvaluationData, response: bool) -> bool {
    let sign = if response { -1.0 } else { 1.0 };
    let no_double = sign * evaluation.no_double;
    let take = sign * evaluation.double_take;
    let pass = sign * evaluation.double_pass;
    pass <= take && no_double > pass
}

/// Optimal outcome when both sides act correctly, from either player's perspective.
/// Matches `properCubeOutcome` in `cube-context-api.ts`.
pub fn proper_cube_outcome(evaluation: &CubeEvaluationData, response: bool) -> CubeOutcome {
    let sign = if response { -1.0 } else { 1.0 };
    let no_double = sign * evaluation.no_double;
    let take = sign * evaluation.double_take;
    let pass = sign * evaluation.double_pass;
    let takes = take <= pass;
    if take.min(pass) <= no_double {
        CubeOutcome::NoDouble
    } else if takes {
        CubeOutcome::DoubleTake
    } else {
        CubeOutcome::DoublePass
    }
}

/// Formats the concise cube & match score line (`CubeContextLine`).
pub fn format_cube_context_line(
    locale: neondojo_i18n::Locale,
    match_id: &str,
    players: [&str; 2],
    offered_by: Option<usize>,
    with_format: bool,
) -> String {
    let ctx = decode_match_context(match_id);
    let mut parts: Vec<String> = Vec::new();
    if ctx.length > 0 {
        if with_format {
            parts.push(I18n::format(
                locale,
                "app",
                "app.matchLength",
                &[("count", Param::from(ctx.length))],
            ));
        }
        let away = [
            (ctx.length - ctx.scores[0]).max(0),
            (ctx.length - ctx.scores[1]).max(0),
        ];
        if ctx.crawford {
            parts.push(I18n::t(locale, "cube-context", "crawford").to_string());
        } else if away.contains(&1) {
            parts.push(I18n::t(locale, "cube-context", "postCrawford").to_string());
        }
        let away_str = format!(
            "{}, {}",
            I18n::format(
                locale,
                "cube-context",
                "away",
                &[
                    ("player", Param::from(players[0])),
                    ("count", Param::from(away[0])),
                ],
            ),
            I18n::format(
                locale,
                "cube-context",
                "away",
                &[
                    ("player", Param::from(players[1])),
                    ("count", Param::from(away[1])),
                ],
            )
        );
        parts.push(away_str);
    } else if with_format {
        let jacoby_key = if ctx.jacoby { "jacobyOn" } else { "jacobyOff" };
        parts.push(format!(
            "{} · {}",
            I18n::t(locale, "app", "common.moneyGame"),
            I18n::t(locale, "cube-context", jacoby_key)
        ));
    }

    let cube_text = if let Some(doubler) = offered_by.filter(|&d| d <= 1) {
        I18n::format(
            locale,
            "cube-context",
            "cubeOffered",
            &[
                ("player", Param::from(players[doubler])),
                ("value", Param::from(ctx.cube * 2)),
            ],
        )
    } else if let Some(owner) = ctx.owner {
        I18n::format(
            locale,
            "cube-context",
            "cubeOwned",
            &[
                ("value", Param::from(ctx.cube)),
                ("player", Param::from(players[usize::from(owner).min(1)])),
            ],
        )
    } else {
        I18n::format(
            locale,
            "cube-context",
            "cubeCentered",
            &[("value", Param::from(ctx.cube))],
        )
    };
    parts.push(cube_text);
    parts.join(" · ")
}

/// Complete cube decision details panel: outcomes table (3-row offer or 2-row
/// response), `Too good to double` callout, 6-chance cubeless table, and
/// synchronous Kazaross-XG2 take-point summary.
#[derive(IntoElement)]
pub struct CubeDecisionDetailsView {
    id: ElementId,
    match_id: Option<String>,
    /// Absolute color of deciding player (`0` White, `1` Black).
    player: usize,
    /// `true` for Take / Pass response; `false` for Double / No-double offer.
    response: bool,
    players: [SharedString; 2],
    evaluation: Option<CubeEvaluationData>,
    calculating_chances: bool,
    palette: Option<NeonPalette>,
    on_calculate_chances: Option<CalculateChancesHandler>,
}

impl CubeDecisionDetailsView {
    pub fn new(
        id: impl Into<ElementId>,
        player: usize,
        response: bool,
        players: [SharedString; 2],
    ) -> Self {
        Self {
            id: id.into(),
            match_id: None,
            player: player.min(1),
            response,
            players,
            evaluation: None,
            calculating_chances: false,
            palette: None,
            on_calculate_chances: None,
        }
    }

    pub fn match_id(mut self, match_id: impl Into<String>) -> Self {
        self.match_id = Some(match_id.into());
        self
    }

    pub fn evaluation(mut self, evaluation: Option<CubeEvaluationData>) -> Self {
        self.evaluation = evaluation;
        self
    }

    pub fn calculating_chances(mut self, calculating: bool) -> Self {
        self.calculating_chances = calculating;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_calculate_chances(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_calculate_chances = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for CubeDecisionDetailsView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        let doubler = if self.response {
            1 - self.player
        } else {
            self.player
        };
        let resolved_ctx: Option<CubeContext> = self
            .match_id
            .as_deref()
            .and_then(|mid| cube_context(mid, doubler as i32).ok());

        let mut root = div()
            .id(self.id)
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .w_full();

        if let Some(mid) = self.match_id.as_deref() {
            let line = format_cube_context_line(
                locale,
                mid,
                [self.players[0].as_ref(), self.players[1].as_ref()],
                self.response.then_some(doubler),
                true,
            );
            root = root.child(div().text_meta().text_color(p.muted).child(line));
        }

        if let Some(eval) = &self.evaluation {
            let mwc = eval.unit == CubeEquityUnit::MatchWinningChance;
            let fmt_val = |v: f64| {
                if mwc {
                    format_percent(locale, v, 2)
                } else {
                    format_equity(locale, v)
                }
            };

            let rows: &[CubeOutcome] = if self.response {
                &[CubeOutcome::DoubleTake, CubeOutcome::DoublePass]
            } else {
                &[
                    CubeOutcome::NoDouble,
                    CubeOutcome::DoubleTake,
                    CubeOutcome::DoublePass,
                ]
            };

            let best = if !self.response {
                proper_cube_outcome(eval, false)
            } else if eval.double_take >= eval.double_pass {
                CubeOutcome::DoubleTake
            } else {
                CubeOutcome::DoublePass
            };

            let label_for = |outcome: CubeOutcome| -> String {
                match (self.response, outcome) {
                    (_, CubeOutcome::NoDouble) => {
                        I18n::t(locale, "analysis-source", "noDouble").to_string()
                    }
                    (true, CubeOutcome::DoubleTake) => localize_decision(locale, "take"),
                    (true, CubeOutcome::DoublePass) => localize_decision(locale, "drop"),
                    (false, CubeOutcome::DoubleTake) => {
                        I18n::t(locale, "analysis-source", "doubleTake").to_string()
                    }
                    (false, CubeOutcome::DoublePass) => {
                        I18n::t(locale, "analysis-source", "doublePass").to_string()
                    }
                }
            };

            let caption = format!(
                "{} · {} · {}",
                I18n::t(locale, "cube-context", "outcomes"),
                I18n::t(
                    locale,
                    "cube-context",
                    if mwc { "unitMwc" } else { "unitCubeful" }
                ),
                I18n::format(
                    locale,
                    "cube-context",
                    "perspective",
                    &[("player", Param::from(self.players[self.player].as_ref()))],
                )
            );

            let mut table = div()
                .v_flex()
                .gap(px(4.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(div().text_meta().text_color(p.muted).child(caption));

            for &outcome in rows {
                let is_best = outcome == best;
                table = table.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded(px(5.0))
                        .bg(if is_best { p.selected } else { p.surface })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_table()
                                .font_weight(if is_best {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(p.text)
                                .child(label_for(outcome))
                                .when(is_best, |this| {
                                    this.child(div().text_meta().text_color(p.accent).child(
                                        format!("· {}", I18n::t(locale, "cube-context", "best")),
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .text_table()
                                .tabular_nums()
                                .font_weight(if is_best {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(p.text)
                                .child(fmt_val(eval.value(outcome))),
                        ),
                );
            }
            root = root.child(table);

            if !self.response && too_good_to_double(eval, false) {
                root = root.child(
                    div()
                        .id("cube-too-good-callout")
                        .test_support()
                        .px(px(10.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .bg(p.warning_bg)
                        .text_label()
                        .text_color(p.warning_text)
                        .child(I18n::t(locale, "cube-context", "tooGood")),
                );
            }

            // 6-chance cubeless table or calculate trigger
            if let Some(probs) = eval.probabilities {
                let mut chances_box = div()
                    .id("cube-chances-table")
                    .test_support()
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .text_table_heading()
                            .text_color(p.muted)
                            .pb(px(4.0))
                            .border_b_1()
                            .border_color(p.border)
                            .child(
                                div()
                                    .flex_1()
                                    .child(I18n::t(locale, "cube-context", "chances")),
                            )
                            .child(div().w(px(64.0)).text_right().child(I18n::t(
                                locale,
                                "cube-context",
                                "win",
                            )))
                            .child(div().w(px(72.0)).text_right().child(I18n::t(
                                locale,
                                "cube-context",
                                "gammon",
                            )))
                            .child(div().w(px(82.0)).text_right().child(I18n::t(
                                locale,
                                "cube-context",
                                "backgammon",
                            ))),
                    );

                for (row_idx, side) in [self.player, 1 - self.player].into_iter().enumerate() {
                    let base = row_idx * 3;
                    chances_box = chances_box.child(
                        div()
                            .h_flex()
                            .items_center()
                            .py(px(3.0))
                            .text_table()
                            .tabular_nums()
                            .text_color(p.text)
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(self.players[side].clone()),
                            )
                            .child(div().w(px(64.0)).text_right().child(format_percent(
                                locale,
                                probs[base],
                                1,
                            )))
                            .child(div().w(px(72.0)).text_right().child(format_percent(
                                locale,
                                probs[base + 1],
                                1,
                            )))
                            .child(div().w(px(82.0)).text_right().child(format_percent(
                                locale,
                                probs[base + 2],
                                1,
                            ))),
                    );
                }
                root = root.child(chances_box);
            } else if let Some(handler) = self.on_calculate_chances {
                root = root.child(if self.calculating_chances {
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(I18n::t(locale, "cube-context", "calculatingChances"))
                        .into_any_element()
                } else {
                    ActionButton::new("cube-calculate-chances")
                        .icon(ActionIconName::Analysis)
                        .label(I18n::t(locale, "cube-context", "calculateChances"))
                        .palette(p)
                        .on_click(move |event, window, cx| handler(event, window, cx))
                        .into_any_element()
                });
            } else {
                root = root.child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "cube-context",
                    "chancesUnavailable",
                )));
            }
        }

        // Take-point line from Kazaross-XG2 match equity table (or 25% money play)
        if let Some(ctx) = resolved_ctx {
            let mut take_line = match &ctx.take_point {
                Some(tp) => {
                    let key = match tp.basis {
                        TakePointBasis::Money => "takePointMoney",
                        TakePointBasis::MatchEquityTable => "takePointMatch",
                    };
                    I18n::format(
                        locale,
                        "cube-context",
                        key,
                        &[
                            ("value", Param::from(format_percent(locale, tp.value, 1))),
                            ("table", Param::from(tp.table.as_deref().unwrap_or(""))),
                        ],
                    )
                }
                None => I18n::t(locale, "cube-context", "takePointUnavailable").to_string(),
            };

            if ctx.take_point.is_some()
                && let Some(probs) = self.evaluation.as_ref().and_then(|e| e.probabilities)
            {
                let taker_wins = if self.response { probs[0] } else { probs[3] };
                take_line.push(' ');
                take_line.push_str(&I18n::format(
                    locale,
                    "cube-context",
                    "takerChance",
                    &[
                        ("player", Param::from(self.players[1 - doubler].as_ref())),
                        ("value", Param::from(format_percent(locale, taker_wins, 1))),
                    ],
                ));
            }

            root = root.child(
                div()
                    .id("cube-take-point")
                    .test_support()
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.muted)
                    .child(take_line),
            );
        }

        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_too_good_and_proper_cube_outcome_for_offer_and_response() {
        let too_good = CubeEvaluationData {
            no_double: 1.15,
            double_take: 1.25,
            double_pass: 1.0,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: None,
        };
        assert!(too_good_to_double(&too_good, false));
        assert_eq!(proper_cube_outcome(&too_good, false), CubeOutcome::NoDouble);

        let clear_double_take = CubeEvaluationData {
            no_double: 0.45,
            double_take: 0.62,
            double_pass: 1.0,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: None,
        };
        assert!(!too_good_to_double(&clear_double_take, false));
        assert_eq!(
            proper_cube_outcome(&clear_double_take, false),
            CubeOutcome::DoubleTake
        );

        let clear_double_pass = CubeEvaluationData {
            no_double: 0.85,
            double_take: 1.18,
            double_pass: 1.0,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: None,
        };
        assert!(!too_good_to_double(&clear_double_pass, false));
        assert_eq!(
            proper_cube_outcome(&clear_double_pass, false),
            CubeOutcome::DoublePass
        );

        // Responder's values are negated relative to the doubler.
        let responder_take = CubeEvaluationData {
            no_double: -0.45,
            double_take: -0.62,
            double_pass: -1.0,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: None,
        };
        assert_eq!(
            proper_cube_outcome(&responder_take, true),
            CubeOutcome::DoubleTake
        );
    }
}
