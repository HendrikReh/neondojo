//! Board-fact comparison and move explanation view (`MoveExplanation.tsx` &
//! `move-explanation.ts`).
//!
//! Calls `neondojo::move_explanation::explain` synchronously in-process (zero
//! HTTP round-trip) and highlights what speaks for the best play versus the
//! chosen play.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled,
    TestSupportExt, Window, base::StyledExt, div, px,
};
use neondojo::move_explanation::{MoveExplanation, MoveExplanationRequest, explain};
use neondojo_i18n::{
    I18n, Locale, Param, format_decimal, format_int, format_percent, localize_decision,
};

/// Which play a board difference speaks in favor of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferenceFavor {
    Best,
    Played,
}

/// Concrete structural or probability difference between two plays of the same roll.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveDifference {
    Win {
        favors: DifferenceFavor,
        best: f64,
        played: f64,
    },
    Gammon {
        favors: DifferenceFavor,
        best: f64,
        played: f64,
    },
    GammonLoss {
        favors: DifferenceFavor,
        best: f64,
        played: f64,
    },
    Hits {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
    },
    Shots {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
        best_cost: f64,
        played_cost: f64,
    },
    PointsBest {
        points: Vec<i32>,
    },
    PointsPlayed {
        points: Vec<i32>,
    },
    Prime {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
    },
    Deep {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
    },
    Stack {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
    },
    Blots {
        favors: DifferenceFavor,
        best: i32,
        played: i32,
    },
}

impl MoveDifference {
    pub fn favors(&self) -> DifferenceFavor {
        match self {
            Self::Win { favors, .. }
            | Self::Gammon { favors, .. }
            | Self::GammonLoss { favors, .. }
            | Self::Hits { favors, .. }
            | Self::Shots { favors, .. }
            | Self::Prime { favors, .. }
            | Self::Deep { favors, .. }
            | Self::Stack { favors, .. }
            | Self::Blots { favors, .. } => *favors,
            Self::PointsBest { .. } => DifferenceFavor::Best,
            Self::PointsPlayed { .. } => DifferenceFavor::Played,
        }
    }

    /// Formats this difference into a localized natural-language bullet.
    pub fn describe(&self, locale: Locale) -> String {
        match self {
            Self::Win { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "win",
                &[
                    ("best", Param::from(format_percent(locale, *best, 1))),
                    ("played", Param::from(format_percent(locale, *played, 1))),
                ],
            ),
            Self::Gammon { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "gammon",
                &[
                    ("best", Param::from(format_percent(locale, *best, 1))),
                    ("played", Param::from(format_percent(locale, *played, 1))),
                ],
            ),
            Self::GammonLoss { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "gammonLoss",
                &[
                    ("best", Param::from(format_percent(locale, *best, 1))),
                    ("played", Param::from(format_percent(locale, *played, 1))),
                ],
            ),
            Self::Hits { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "hits",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                ],
            ),
            Self::Shots {
                best,
                played,
                best_cost,
                played_cost,
                ..
            } => I18n::format(
                locale,
                "move-explanation",
                "shots",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                    (
                        "bestCost",
                        Param::from(format_decimal(locale, *best_cost, 1, 1)),
                    ),
                    (
                        "playedCost",
                        Param::from(format_decimal(locale, *played_cost, 1, 1)),
                    ),
                ],
            ),
            Self::PointsBest { points } => I18n::format(
                locale,
                "move-explanation",
                "pointsBest",
                &[(
                    "points",
                    Param::from(
                        points
                            .iter()
                            .map(|&pt| format_int(locale, i64::from(pt)))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )],
            ),
            Self::PointsPlayed { points } => I18n::format(
                locale,
                "move-explanation",
                "pointsPlayed",
                &[(
                    "points",
                    Param::from(
                        points
                            .iter()
                            .map(|&pt| format_int(locale, i64::from(pt)))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )],
            ),
            Self::Prime { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "prime",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                ],
            ),
            Self::Deep { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "deep",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                ],
            ),
            Self::Stack { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "stack",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                ],
            ),
            Self::Blots { best, played, .. } => I18n::format(
                locale,
                "move-explanation",
                "blots",
                &[
                    ("best", Param::from(format_int(locale, i64::from(*best)))),
                    (
                        "played",
                        Param::from(format_int(locale, i64::from(*played))),
                    ),
                ],
            ),
        }
    }
}

/// Exact Rust port of `moveDifferences()` from `web/src/shared/review/move-explanation.ts`.
pub fn compute_move_differences(
    explanation: &MoveExplanation,
    best_chances: Option<[f64; 6]>,
    played_chances: Option<[f64; 6]>,
    max_best: usize,
    max_played: usize,
) -> Vec<MoveDifference> {
    let best = &explanation.best;
    let played = &explanation.played;
    let mut found = Vec::new();
    let favor = |best_is_better: bool| {
        if best_is_better {
            DifferenceFavor::Best
        } else {
            DifferenceFavor::Played
        }
    };

    if let (Some(bc), Some(pc)) = (best_chances, played_chances)
        && (bc[0] - pc[0]).abs() >= 0.01
    {
        found.push(MoveDifference::Win {
            favors: favor(bc[0] > pc[0]),
            best: bc[0],
            played: pc[0],
        });
    }

    if best.opponent_on_bar != played.opponent_on_bar {
        found.push(MoveDifference::Hits {
            favors: favor(best.opponent_on_bar > played.opponent_on_bar),
            best: best.opponent_on_bar,
            played: played.opponent_on_bar,
        });
    }

    if best.hitting_rolls != played.hitting_rolls || best.hit_pips != played.hit_pips {
        let best_is_safer = if best.hit_pips != played.hit_pips {
            best.hit_pips < played.hit_pips
        } else {
            best.hitting_rolls < played.hitting_rolls
        };
        found.push(MoveDifference::Shots {
            favors: favor(best_is_safer),
            best: best.hitting_rolls,
            played: played.hitting_rolls,
            best_cost: f64::from(best.hit_pips) / 36.0,
            played_cost: f64::from(played.hit_pips) / 36.0,
        });
    }

    let best_only: Vec<i32> = best
        .points
        .iter()
        .copied()
        .filter(|pt| !played.points.contains(pt))
        .collect();
    if !best_only.is_empty() {
        found.push(MoveDifference::PointsBest { points: best_only });
    }

    let played_only: Vec<i32> = played
        .points
        .iter()
        .copied()
        .filter(|pt| !best.points.contains(pt))
        .collect();
    if !played_only.is_empty() {
        found.push(MoveDifference::PointsPlayed {
            points: played_only,
        });
    }

    if best.longest_prime != played.longest_prime {
        found.push(MoveDifference::Prime {
            favors: favor(best.longest_prime > played.longest_prime),
            best: best.longest_prime,
            played: played.longest_prime,
        });
    }

    if best.deep_checkers != played.deep_checkers {
        found.push(MoveDifference::Deep {
            favors: favor(best.deep_checkers < played.deep_checkers),
            best: best.deep_checkers,
            played: played.deep_checkers,
        });
    }

    if best.stacked_checkers != played.stacked_checkers {
        found.push(MoveDifference::Stack {
            favors: favor(best.stacked_checkers < played.stacked_checkers),
            best: best.stacked_checkers,
            played: played.stacked_checkers,
        });
    }

    if best.hitting_rolls == played.hitting_rolls
        && best.hit_pips == played.hit_pips
        && best.blots.len() != played.blots.len()
    {
        found.push(MoveDifference::Blots {
            favors: favor(best.blots.len() < played.blots.len()),
            best: best.blots.len() as i32,
            played: played.blots.len() as i32,
        });
    }

    if let (Some(bc), Some(pc)) = (best_chances, played_chances) {
        if (bc[1] - pc[1]).abs() >= 0.01 {
            found.push(MoveDifference::Gammon {
                favors: favor(bc[1] > pc[1]),
                best: bc[1],
                played: pc[1],
            });
        }
        if (bc[4] - pc[4]).abs() >= 0.01 {
            found.push(MoveDifference::GammonLoss {
                favors: favor(bc[4] < pc[4]),
                best: bc[4],
                played: pc[4],
            });
        }
    }

    let mut out = Vec::new();
    out.extend(
        found
            .iter()
            .filter(|d| d.favors() == DifferenceFavor::Best)
            .take(max_best)
            .cloned(),
    );
    out.extend(
        found
            .iter()
            .filter(|d| d.favors() == DifferenceFavor::Played)
            .take(max_played)
            .cloned(),
    );
    out
}

/// Synchronous in-process helper calling `neondojo::move_explanation::explain`.
pub fn explain_checker_plays(
    board: [i32; 26],
    player: i32,
    dice: [i32; 2],
    best: &str,
    played: &str,
) -> Result<MoveExplanation, &'static str> {
    explain(&MoveExplanationRequest {
        board,
        player,
        dice,
        best: best.to_string(),
        played: played.to_string(),
    })
}

/// Visual move explanation section comparing best play vs chosen play.
#[derive(IntoElement)]
pub struct MoveExplanationView {
    id: ElementId,
    best_notation: String,
    played_notation: String,
    differences: Vec<MoveDifference>,
    show_glossary: bool,
    palette: Option<NeonPalette>,
}

impl MoveExplanationView {
    pub fn from_explanation(
        id: impl Into<ElementId>,
        best_notation: impl Into<String>,
        played_notation: impl Into<String>,
        explanation: &MoveExplanation,
        best_chances: Option<[f64; 6]>,
        played_chances: Option<[f64; 6]>,
    ) -> Self {
        let differences = compute_move_differences(explanation, best_chances, played_chances, 3, 2);
        Self {
            id: id.into(),
            best_notation: best_notation.into(),
            played_notation: played_notation.into(),
            differences,
            show_glossary: false,
            palette: None,
        }
    }

    pub fn show_glossary(mut self, show: bool) -> Self {
        self.show_glossary = show;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }
}

impl RenderOnce for MoveExplanationView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        let for_best: Vec<String> = self
            .differences
            .iter()
            .filter(|d| d.favors() == DifferenceFavor::Best)
            .map(|d| d.describe(locale))
            .collect();
        let for_played: Vec<String> = self
            .differences
            .iter()
            .filter(|d| d.favors() == DifferenceFavor::Played)
            .map(|d| d.describe(locale))
            .collect();

        let header_line = I18n::format(
            locale,
            "move-explanation",
            "columns",
            &[
                (
                    "best",
                    Param::from(localize_decision(locale, &self.best_notation)),
                ),
                (
                    "played",
                    Param::from(localize_decision(locale, &self.played_notation)),
                ),
            ],
        );

        let mut card = div()
            .id(self.id)
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .v_flex()
                    .gap(px(2.0))
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "move-explanation",
                        "title",
                    )))
                    .child(div().text_meta().text_color(p.muted).child(header_line)),
            );

        if self.differences.is_empty() {
            card = card.child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "move-explanation",
                "same",
            )));
        } else {
            if for_best.is_empty() {
                card = card.child(div().text_body().text_color(p.muted).child(I18n::t(
                    locale,
                    "move-explanation",
                    "onlyEvaluation",
                )));
            } else {
                let mut best_block = div().v_flex().gap(px(4.0)).child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.success_text)
                        .child(I18n::t(locale, "move-explanation", "forBest")),
                );
                for line in for_best {
                    best_block = best_block.child(
                        div()
                            .text_body()
                            .text_color(p.text)
                            .child(format!("• {line}")),
                    );
                }
                card = card.child(best_block);
            }

            if !for_played.is_empty() {
                let mut played_block = div().v_flex().gap(px(4.0)).child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(I18n::t(locale, "move-explanation", "forPlayed")),
                );
                for line in for_played {
                    played_block = played_block.child(
                        div()
                            .text_body()
                            .text_color(p.text)
                            .child(format!("• {line}")),
                    );
                }
                card = card.child(played_block);
            }
        }

        card = card.child(div().text_meta().text_color(p.muted).child(I18n::t(
            locale,
            "move-explanation",
            "note",
        )));

        if self.show_glossary {
            let mut glossary = div()
                .v_flex()
                .gap(px(4.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(I18n::t(locale, "move-explanation", "glossary")),
                );
            for term in ["equity", "loss", "pr", "ply"] {
                glossary = glossary.child(
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(format!("• {}", I18n::t(locale, "move-explanation", term))),
                );
            }
            card = card.child(glossary);
        }

        card
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neondojo_rules::backgammon::initial_board;

    #[test]
    fn explains_opening_31_and_ranks_best_vs_played_differences() {
        let explanation =
            explain_checker_plays(initial_board(), 0, [3, 1], "8/5 6/5", "13/10 24/23")
                .expect("valid legal plays for opening 3-1");
        let diffs = compute_move_differences(&explanation, None, None, 3, 2);
        assert!(
            diffs
                .iter()
                .any(|d| matches!(d, MoveDifference::PointsBest { points } if points.contains(&5))),
            "expected best play 8/5 6/5 to highlight making the 5-point"
        );
        let described = diffs[0].describe(Locale::En);
        assert!(!described.is_empty());
    }
}
