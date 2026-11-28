//! Reusable move history row (`MoveHistoryContent.tsx`).
//!
//! Displays ply number, localized compact notation, player color dot, player
//! name, dice roll, right-aligned 3-decimal equity loss (`tabular_nums()`), and
//! severity badge with detailed provenance tooltip.

use crate::i18n::current_locale;
use crate::review::severity::SeverityStyle;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt, component::tooltip::Tooltip, div, prelude::FluentBuilder, px, rgb,
};
use neondojo::model::MoveSeverity;
use neondojo_i18n::{
    CheckerPositionRef, I18n, format_decimal, format_equity, localize_decision_in_position,
};
use std::rc::Rc;

type RowClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Provenance of a move assessment badge.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AssessmentSource {
    #[default]
    Engine,
    Imported,
}

/// Evaluation summary rendered on the right edge of a [`MoveHistoryRow`].
#[derive(Debug, Clone)]
pub struct MoveAssessmentBadge {
    pub equity_loss: f64,
    pub severity: MoveSeverity,
    pub source: AssessmentSource,
    pub engine: SharedString,
    pub depth: Option<SharedString>,
}

/// Dense interactive move list row shared by Analysis, Play history, and Study.
#[derive(IntoElement)]
pub struct MoveHistoryRow {
    id: ElementId,
    ply_label: SharedString,
    /// `0` = White, `1` = Black.
    player: u8,
    player_name: SharedString,
    dice: Vec<u8>,
    notation: String,
    board: Option<[i8; 26]>,
    assessment: Option<MoveAssessmentBadge>,
    selected: bool,
    palette: Option<NeonPalette>,
    on_click: Option<RowClickHandler>,
}

impl MoveHistoryRow {
    pub fn new(
        id: impl Into<ElementId>,
        ply_label: impl Into<SharedString>,
        player: u8,
        player_name: impl Into<SharedString>,
        notation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ply_label: ply_label.into(),
            player,
            player_name: player_name.into(),
            dice: Vec::new(),
            notation: notation.into(),
            board: None,
            assessment: None,
            selected: false,
            palette: None,
            on_click: None,
        }
    }

    pub fn dice(mut self, dice: Vec<u8>) -> Self {
        self.dice = dice;
        self
    }

    pub fn board_i32(mut self, board: &[i32; 26]) -> Self {
        let mut compact = [0i8; 26];
        for (dst, &src) in compact.iter_mut().zip(board.iter()) {
            *dst = src.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
        }
        self.board = Some(compact);
        self
    }

    pub fn assessment(mut self, assessment: Option<MoveAssessmentBadge>) -> Self {
        self.assessment = assessment;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for MoveHistoryRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        let pos_ref = if let (Some(board), [d1, d2]) = (self.board.as_ref(), self.dice.as_slice()) {
            Some(CheckerPositionRef {
                board,
                player: self.player,
                dice: [*d1, *d2],
            })
        } else {
            None
        };
        let notation = localize_decision_in_position(locale, &self.notation, pos_ref);
        let dice_str = if self.dice.len() >= 2 {
            Some(format!("{}–{}", self.dice[0], self.dice[1]))
        } else {
            None
        };

        let severity_style = self
            .assessment
            .as_ref()
            .map(|a| SeverityStyle::resolve(a.severity, &p));

        let tooltip_text = match (&self.assessment, &severity_style) {
            (Some(a), Some(style)) => {
                let src_key = match a.source {
                    AssessmentSource::Imported => "imported",
                    AssessmentSource::Engine => "engine",
                };
                let src_label = I18n::t(locale, "analysis-source", src_key);
                let depth = a
                    .depth
                    .as_ref()
                    .map(|d| format!(" · {d}"))
                    .unwrap_or_default();
                format!(
                    "{} · Δ {} · {} · {}{}",
                    style.label(locale),
                    format_equity(locale, a.equity_loss),
                    src_label,
                    a.engine,
                    depth
                )
            }
            _ => I18n::t(locale, "app", "matchAnalysis.unavailable").to_string(),
        };

        let dot_fill = if self.player == 0 {
            rgb(0xf6f6f2)
        } else {
            rgb(0x23272e)
        };
        let dot_border = if self.player == 0 {
            p.control_border
        } else {
            p.border_strong
        };

        let row_bg = if self.selected { p.selected } else { p.surface };
        let hover_bg = if self.selected {
            p.selected
        } else {
            p.surface_alt
        };

        div()
            .id(self.id)
            .test_support()
            .h_flex()
            .items_center()
            .gap(px(10.0))
            .w_full()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if self.selected { p.accent } else { p.border })
            .bg(row_bg)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .when_some(self.on_click, |this, cb| {
                this.on_click(move |event, window, cx| cb(event, window, cx))
            })
            .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
            .child(
                div()
                    .w(px(30.0))
                    .flex_shrink_0()
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.muted)
                    .child(self.ply_label),
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .truncate()
                            .child(notation),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_meta()
                            .text_color(p.muted)
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(dot_fill)
                                    .border_1()
                                    .border_color(dot_border)
                                    .flex_shrink_0(),
                            )
                            .child(div().truncate().child(self.player_name))
                            .when_some(dice_str, |this, dice| {
                                this.child("·")
                                    .child(div().tabular_nums().text_color(p.text).child(dice))
                            }),
                    ),
            )
            .child(match (self.assessment, severity_style) {
                (Some(a), Some(style)) => {
                    let loss_str = format_decimal(locale, a.equity_loss, 3, 3);
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .flex_shrink_0()
                        .child(
                            div()
                                .text_table()
                                .tabular_nums()
                                .text_color(if style.is_error() { style.fg } else { p.muted })
                                .child(loss_str),
                        )
                        .when(style.is_error(), |this| {
                            this.child(
                                div()
                                    .text_micro()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(style.bg)
                                    .text_color(style.fg)
                                    .child(style.label(locale)),
                            )
                        })
                        .into_any_element()
                }
                _ => div()
                    .text_meta()
                    .text_color(p.muted)
                    .flex_shrink_0()
                    .child("—")
                    .into_any_element(),
            })
    }
}
