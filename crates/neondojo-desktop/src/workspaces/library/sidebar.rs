//! Database sidebar (`DatabaseSidebar.tsx`) showing `All databases`, all user
//! databases, and the 4 protected system databases (`Unsorted`, `Bot Matches`,
//! `Paused Matches`, `Engine Duels`) with match counts and CRUD / Empty rules.

use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::{ActionIconName, action_icon};
use crate::workspaces::library::LibraryWorkspace;
use crate::workspaces::library::state::{
    can_empty_database, clone_database, database_display_name, database_tooltip,
};
use gpui_kit::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt,
    component::tooltip::Tooltip, div, prelude::FluentBuilder, px,
};
use neondojo::play::PAUSED_MATCHES_DATABASE_ID;
use neondojo_i18n::{I18n, Locale};

impl LibraryWorkspace {
    pub(super) fn render_database_sidebar(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let total_matches: i64 = self.databases.iter().map(|d| d.match_count).sum();
        let active_db = self
            .selected_database_id
            .and_then(|id| self.databases.iter().find(|d| d.id == id))
            .map(clone_database);

        let mut collections = div()
            .id("library-database-list")
            .v_flex()
            .gap(px(2.0))
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child({
                let is_all = self.selected_database_id.is_none();
                let ent = entity.clone();
                div()
                    .id("library-db-all")
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .bg(if is_all { p.selected } else { p.surface_alt })
                    .text_color(if is_all { p.accent } else { p.text })
                    .hover(move |s| s.bg(p.selected))
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| this.select_database(None, cx));
                    })
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .child(action_icon(
                                ActionIconName::Deck,
                                15.0,
                                if is_all { p.accent } else { p.muted },
                            ))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(if is_all {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .truncate()
                                    .child(I18n::t(locale, "library", "allDatabases")),
                            ),
                    )
                    .child(
                        div()
                            .text_meta()
                            .tabular_nums()
                            .text_color(if is_all { p.accent } else { p.muted })
                            .child(total_matches.to_string()),
                    )
            });

        for db in &self.databases {
            let db_id = db.id;
            let selected = self.selected_database_id == Some(db_id);
            let display_name = database_display_name(db, locale);
            let tooltip_text = database_tooltip(db, locale);
            let icon = if db.is_system {
                ActionIconName::SystemDatabase
            } else {
                ActionIconName::Database
            };
            let ent = entity.clone();

            let row = div()
                .id(SharedString::from(format!("library-db-{db_id}")))
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .bg(if selected { p.selected } else { p.surface_alt })
                .text_color(if selected { p.accent } else { p.text })
                .hover(move |s| s.bg(p.selected))
                .on_click(move |_, _, cx| {
                    ent.update(cx, |this, cx| this.select_database(Some(db_id), cx));
                })
                .when_some(tooltip_text, |el, tip| {
                    el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                })
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .flex_1()
                        .min_w(px(0.0))
                        .child(action_icon(
                            icon,
                            15.0,
                            if selected { p.accent } else { p.muted },
                        ))
                        .child(
                            div()
                                .text_label()
                                .font_weight(if selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .truncate()
                                .child(display_name),
                        ),
                )
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(if selected { p.accent } else { p.muted })
                        .child(db.match_count.to_string()),
                );

            collections = collections.child(row);
        }

        div()
            .id("library-database-sidebar")
            .test_support()
            .v_flex()
            .w(px(236.0))
            .flex_shrink_0()
            .h_full()
            .min_h(px(0.0))
            .bg(p.surface_alt)
            .border_r_1()
            .border_color(p.border)
            .p(px(12.0))
            .gap(px(10.0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "library",
                        "databases",
                    )))
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-new-database-btn")
                            .icon(ActionIconName::Add)
                            .tooltip(I18n::t(locale, "library", "newDatabase"))
                            .dense_nav()
                            .palette(p)
                            .disabled(self.facade.is_none())
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_database_form_modal(None, window, cx);
                                });
                            })
                    }),
            )
            .child(collections)
            .when_some(active_db, |this, db| {
                let edit_db = clone_database(&db);
                let remove_db = clone_database(&db);
                let ent_edit = entity.clone();
                let ent_remove = entity.clone();
                let show_remove = db.id != PAUSED_MATCHES_DATABASE_ID;
                let remove_disabled =
                    self.facade.is_none() || (db.is_system && !can_empty_database(&db));
                let remove_label = if db.is_system {
                    I18n::t(locale, "library", "clearDatabase")
                } else {
                    I18n::t(locale, "library", "deleteDatabase")
                };
                let desc = db.description.trim().to_string();

                this.child(
                    div()
                        .id("library-active-db-actions")
                        .test_support()
                        .v_flex()
                        .gap(px(8.0))
                        .pt(px(10.0))
                        .border_t_1()
                        .border_color(p.border)
                        .when(!desc.is_empty(), |el| {
                            el.child(div().text_meta().text_color(p.muted).child(desc))
                        })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    ActionButton::new("library-edit-db-btn")
                                        .icon(ActionIconName::Edit)
                                        .label(I18n::t(locale, "library", "editDatabase"))
                                        .palette(p)
                                        .disabled(self.facade.is_none())
                                        .on_click(move |_, window, cx| {
                                            let db_clone = clone_database(&edit_db);
                                            ent_edit.update(cx, |this, cx| {
                                                this.open_database_form_modal(
                                                    Some(db_clone),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                                .when(show_remove, |el| {
                                    el.child(
                                        ActionButton::new("library-remove-db-btn")
                                            .icon(ActionIconName::Delete)
                                            .tooltip(remove_label)
                                            .variant(ActionButtonVariant::Danger)
                                            .dense_nav()
                                            .palette(p)
                                            .disabled(remove_disabled)
                                            .on_click(move |_, window, cx| {
                                                let db_clone = clone_database(&remove_db);
                                                ent_remove.update(cx, |this, cx| {
                                                    this.open_confirm_database_modal(
                                                        db_clone, window, cx,
                                                    );
                                                });
                                            }),
                                    )
                                }),
                        ),
                )
            })
    }
}
