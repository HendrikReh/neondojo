//! The same searchable player control serves the global overview and database-scoped mistakes.

use super::StudyWorkspace;
use crate::theme::{NeonPalette, NeonTypographyExt};
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Window,
    base::StyledExt,
    component::select::{SearchableVec, Select, SelectEvent, SelectState},
    div, px,
};
use neondojo_i18n::{I18n, Locale};

pub(super) struct StudyPlayerPicker {
    select: Entity<SelectState<SearchableVec<String>>>,
    players: Vec<String>,
    items_changed: bool,
}

impl StudyPlayerPicker {
    pub(super) fn new(window: &mut Window, cx: &mut Context<StudyWorkspace>) -> Self {
        let select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(Vec::new()), None, window, cx).searchable(true)
        });
        cx.subscribe(&select, |workspace, _, event, cx| {
            let SelectEvent::Confirm(player) = event;
            workspace.set_selected_player(player.clone().unwrap_or_default(), cx);
        })
        .detach();
        Self {
            select,
            players: vec![],
            items_changed: false,
        }
    }

    pub(super) fn set_players(&mut self, players: Vec<String>) {
        if self.players != players {
            self.players = players;
            self.items_changed = true;
        }
    }

    pub(super) fn available_player<'a>(&self, selected: &'a str) -> &'a str {
        if self.players.iter().any(|name| name == selected) {
            selected
        } else {
            ""
        }
    }

    pub(super) fn render(
        &mut self,
        id: &'static str,
        selected: &str,
        locale: Locale,
        palette: NeonPalette,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement + use<> {
        if self.items_changed {
            self.select.update(cx, |select, cx| {
                select.set_items(SearchableVec::new(self.players.clone()), window, cx);
            });
            self.items_changed = false;
        }
        if self
            .select
            .read(cx)
            .selected_value()
            .map(String::as_str)
            .unwrap_or("")
            != selected
        {
            self.select.update(cx, |select, cx| {
                select.set_selected_value(&selected.to_owned(), window, cx);
            });
        }
        let label = I18n::t(locale, "recurring-mistakes", "player");
        div()
            .v_flex()
            .gap(px(4.))
            .min_w_0()
            .child(div().text_label().text_color(palette.muted).child(label))
            .child(
                div().w_full().max_w(px(320.)).h(px(32.)).child(
                    Select::new(&self.select)
                        .id(id)
                        .accessibility_label(label)
                        .placeholder(I18n::t(locale, "recurring-mistakes", "choose"))
                        .search_placeholder(I18n::t(locale, "player-directory", "search"))
                        .cleanable(true)
                        .disabled(self.players.is_empty()),
                ),
            )
    }
}
