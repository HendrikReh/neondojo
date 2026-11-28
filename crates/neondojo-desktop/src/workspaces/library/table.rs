//! Dense Match Library toolbar, column picker popover, batch action bar,
//! horizontally & vertically scrollable match table (15 toggleable / reorderable
//! / resizable columns + 3 dynamic Position-ID occurrence columns), and
//! [`NumberedSearchPagination`].

use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::icons::{ActionIconName, action_icon};
use crate::ui::pagination::NumberedSearchPagination;
use crate::workspaces::library::LibraryWorkspace;
use crate::workspaces::library::dialogs::BatchDialogKind;
use crate::workspaces::library::state::{
    ColumnLayoutState, LibraryDataColumn, MAX_SELECTION, OccurrenceColumn, PAGE_SIZE,
    SortDirection, can_show_match_pr, clone_database_match, database_display_name,
    format_library_analysis_status, format_naive_date, format_play_mode, format_pr,
    format_utc_date, has_pr_warning, library_row_key, resumable_session_id,
};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt,
    component::tooltip::Tooltip, div, prelude::FluentBuilder, px,
};
use neondojo::databases::DatabaseMatch;
use neondojo_i18n::{I18n, Locale, Param};

const SELECT_COL_WIDTH: f32 = 34.0;
const RESIZE_STEP_PX: i32 = 20;

impl LibraryWorkspace {
    pub(super) fn render_toolbar_and_batch_bar(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let has_occurrences = self.filters.has_position_filter();
        let active_db_label = self
            .selected_database_id
            .and_then(|id| self.databases.iter().find(|d| d.id == id))
            .map(|db| database_display_name(db, locale))
            .unwrap_or_else(|| I18n::t(locale, "library", "allDatabases").to_string());

        let count_text = match self.total_matches {
            Some(total) => format!("{active_db_label} · {total}"),
            None => format!("{active_db_label} · {}", self.matches.len()),
        };

        let hidden_sort_badge = self
            .columns
            .is_sort_column_hidden(&self.sort, has_occurrences)
            .then(|| {
                let col_name = ColumnLayoutState::sort_label(&self.sort, locale);
                let dir_name = self.direction.label(locale);
                I18n::format(
                    locale,
                    "library",
                    "sortedBy",
                    &[
                        ("column", Param::from(col_name.as_str())),
                        ("direction", Param::from(dir_name)),
                    ],
                )
            });

        let bulk_progress_badge = self
            .bulk_tracker
            .scope_progress(self.selected_database_id)
            .map(|(completed, total)| {
                I18n::format(
                    locale,
                    "library",
                    "bulkAnalysisProgress",
                    &[
                        ("completed", Param::from(completed as i64)),
                        ("total", Param::from(total as i64)),
                    ],
                )
            });

        let toolbar = div()
            .id("library-toolbar")
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(8.0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .tabular_nums()
                            .text_color(p.text)
                            .child(count_text),
                    )
                    .when_some(hidden_sort_badge, |el, badge| {
                        el.child(
                            div()
                                .id("library-hidden-sort-badge")
                                .test_support()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(5.0))
                                .bg(p.warning_bg)
                                .border_1()
                                .border_color(p.border)
                                .text_meta()
                                .text_color(p.warning_text)
                                .child(badge),
                        )
                    })
                    .when_some(bulk_progress_badge, |el, badge| {
                        let finished = !self.bulk_tracker.has_active_jobs();
                        let ent = entity.clone();
                        el.child(
                            div()
                                .id("library-bulk-progress-pill")
                                .test_support()
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(5.0))
                                .bg(if finished { p.success_bg } else { p.selected })
                                .border_1()
                                .border_color(p.border)
                                .text_meta()
                                .text_color(if finished { p.success_text } else { p.accent })
                                .child(badge)
                                .when(finished, |inner| {
                                    inner.child(
                                        div()
                                            .id("library-bulk-progress-dismiss")
                                            .cursor_pointer()
                                            .on_click(move |_, _, cx| {
                                                ent.update(cx, |this, cx| {
                                                    this.bulk_tracker.dismiss_if_finished();
                                                    this.persist_library_prefs(cx);
                                                    cx.notify();
                                                });
                                            })
                                            .child("×"),
                                    )
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .when(self.columns.has_custom_widths(), |el| {
                        let ent = entity.clone();
                        el.child(
                            ActionButton::new("library-reset-widths-btn")
                                .icon(ActionIconName::Reset)
                                .label(I18n::t(locale, "library", "resetColumnWidths"))
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent.update(cx, |this, cx| {
                                        this.columns.reset_widths();
                                        this.persist_library_prefs(cx);
                                        cx.notify();
                                    });
                                }),
                        )
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-columns-picker-btn")
                            .icon(ActionIconName::Settings)
                            .label(I18n::t(locale, "library", "columns"))
                            .active(self.show_column_picker)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| {
                                    this.show_column_picker = !this.show_column_picker;
                                    cx.notify();
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-refresh-btn")
                            .icon(ActionIconName::Refresh)
                            .tooltip(I18n::t(locale, "workspace", "retry"))
                            .dense_nav()
                            .palette(p)
                            .disabled(self.facade.is_none() || self.loading_matches)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| {
                                    this.reload_all(cx);
                                });
                            })
                    }),
            );

        div()
            .v_flex()
            .gap(px(8.0))
            .child(toolbar)
            .when(self.show_column_picker, |el| {
                el.child(self.render_column_picker_popover(p, locale, cx))
            })
            .child(self.render_batch_bar(p, locale, cx))
            .when_some(self.last_batch_banner.clone(), |el, (kind, msg)| {
                let ent = entity.clone();
                el.child(
                    StatusBanner::new("library-batch-result-banner", kind, msg)
                        .palette(p)
                        .on_dismiss(move |_, _, cx| {
                            ent.update(cx, |this, cx| {
                                this.last_batch_banner = None;
                                cx.notify();
                            });
                        }),
                )
            })
    }

    fn render_column_picker_popover(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let ordered = self.columns.order().to_vec();
        let visible_count = self.columns.ordered_visible().len();
        let last_idx = ordered.len().saturating_sub(1);

        let mut list = div().flex().flex_row().flex_wrap().gap(px(6.0));
        for (idx, col) in ordered.into_iter().enumerate() {
            let visible = self.columns.is_visible(col);
            let cannot_hide = visible && visible_count <= 1;
            let width = self.columns.width_of(col);
            let ent_toggle = entity.clone();
            let ent_left = entity.clone();
            let ent_right = entity.clone();
            let ent_shrink = entity.clone();
            let ent_grow = entity.clone();

            list = list.child(
                div()
                    .id(SharedString::from(format!("library-col-pick-{}", col.id())))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(if visible { p.accent } else { p.border })
                    .bg(if visible { p.selected } else { p.surface_alt })
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "library-col-toggle-{}",
                                col.id()
                            )))
                            .test_support()
                            .h_flex()
                            .items_center()
                            .gap(px(5.0))
                            .when(!cannot_hide, |el| {
                                el.cursor_pointer().on_click(move |_, _, cx| {
                                    ent_toggle.update(cx, |this, cx| {
                                        this.columns.toggle_visible(col, !visible);
                                        this.persist_library_prefs(cx);
                                        cx.notify();
                                    });
                                })
                            })
                            .child(
                                div()
                                    .text_meta()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if visible { p.accent } else { p.muted })
                                    .child(if visible { "✓" } else { "○" }),
                            )
                            .child(
                                div()
                                    .text_label()
                                    .text_color(if visible { p.text } else { p.muted })
                                    .child(col.picker_label(locale)),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("library-col-left-{}", col.id())))
                            .test_support()
                            .px(px(3.0))
                            .rounded(px(3.0))
                            .text_meta()
                            .text_color(if idx > 0 { p.muted } else { p.compact_disabled })
                            .when(idx > 0, |el| {
                                el.cursor_pointer()
                                    .hover(move |s| s.bg(p.surface))
                                    .on_click(move |_, _, cx| {
                                        ent_left.update(cx, |this, cx| {
                                            this.columns.move_column(col, -1);
                                            this.persist_library_prefs(cx);
                                            cx.notify();
                                        });
                                    })
                            })
                            .child("←"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "library-col-right-{}",
                                col.id()
                            )))
                            .test_support()
                            .px(px(3.0))
                            .rounded(px(3.0))
                            .text_meta()
                            .text_color(if idx < last_idx {
                                p.muted
                            } else {
                                p.compact_disabled
                            })
                            .when(idx < last_idx, |el| {
                                el.cursor_pointer()
                                    .hover(move |s| s.bg(p.surface))
                                    .on_click(move |_, _, cx| {
                                        ent_right.update(cx, |this, cx| {
                                            this.columns.move_column(col, 1);
                                            this.persist_library_prefs(cx);
                                            cx.notify();
                                        });
                                    })
                            })
                            .child("→"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "library-col-shrink-{}",
                                col.id()
                            )))
                            .px(px(3.0))
                            .text_meta()
                            .text_color(p.muted)
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                ent_shrink.update(cx, |this, cx| {
                                    this.columns.adjust_width(col, -RESIZE_STEP_PX);
                                    this.persist_library_prefs(cx);
                                    cx.notify();
                                });
                            })
                            .child("−"),
                    )
                    .child(
                        div()
                            .text_micro()
                            .tabular_nums()
                            .text_color(p.muted)
                            .child(format!("{width}")),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("library-col-grow-{}", col.id())))
                            .px(px(3.0))
                            .text_meta()
                            .text_color(p.muted)
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                ent_grow.update(cx, |this, cx| {
                                    this.columns.adjust_width(col, RESIZE_STEP_PX);
                                    this.persist_library_prefs(cx);
                                    cx.notify();
                                });
                            })
                            .child("+"),
                    ),
            );
        }

        div()
            .id("library-columns-popover")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(I18n::t(locale, "library", "chooseColumns")),
                            )
                            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                locale,
                                "library",
                                "columnSelectionHint",
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap(px(6.0))
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-cols-show-all-btn")
                                    .label(I18n::t(locale, "library", "showAllColumns"))
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.columns.show_all();
                                            this.persist_library_prefs(cx);
                                            cx.notify();
                                        });
                                    })
                            })
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-cols-default-btn")
                                    .label(I18n::t(locale, "library", "defaultColumns"))
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.columns.reset_defaults();
                                            this.persist_library_prefs(cx);
                                            cx.notify();
                                        });
                                    })
                            }),
                    ),
            )
            .child(list)
    }

    fn render_batch_bar(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let has_selection = !self.selection.is_empty();
        let single = self.selection.single().map(clone_database_match);
        let resume_session = single.as_ref().and_then(resumable_session_id);
        let (analyzable, unavailable_count) = self.selection.partition_analyzable();
        let can_mutate = self.selection.can_mutate_membership() && self.facade.is_some();
        let can_export = self.selection.can_export() && self.facade.is_some();
        let can_tags = has_selection && self.facade.is_some();

        let selected_label = I18n::format(
            locale,
            "library",
            "selectedMatches",
            &[("count", Param::from(self.selection.len() as i64))],
        );

        div()
            .id("library-batch-bar")
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(if has_selection { p.accent } else { p.border })
            .bg(if has_selection {
                p.selected
            } else {
                p.surface_alt
            })
            .child(
                div()
                    .v_flex()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .tabular_nums()
                            .text_color(p.text)
                            .child(selected_label),
                    )
                    .child(div().text_micro().text_color(p.muted).child(I18n::t(
                        locale,
                        "library",
                        "batchLimit",
                    ))),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child({
                        let single_open = single.as_ref().map(clone_database_match);
                        let disabled = single_open.is_none()
                            || resume_session.is_some()
                            || single_open.as_ref().is_some_and(|m| m.play_paused);
                        let ent = entity.clone();
                        ActionButton::new("library-batch-open-btn")
                            .icon(ActionIconName::Open)
                            .label(I18n::t(locale, "workspace", "openMatch"))
                            .disabled(disabled)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                if let Some(m) = &single_open {
                                    let m = clone_database_match(m);
                                    ent.update(cx, |this, cx| this.open_library_match(&m, cx));
                                }
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-resume-btn")
                            .icon(ActionIconName::Resume)
                            .label(I18n::t(locale, "library", "resumePlay"))
                            .disabled(resume_session.is_none())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                if let Some(session_id) = resume_session {
                                    ent.update(cx, |this, cx| {
                                        this.emit_resume_play(session_id, cx);
                                    });
                                }
                            })
                    })
                    .child({
                        let single_edit = single.as_ref().map(clone_database_match);
                        let ent = entity.clone();
                        ActionButton::new("library-batch-edit-btn")
                            .icon(ActionIconName::Edit)
                            .label(I18n::t(locale, "library", "editMatch"))
                            .disabled(single_edit.is_none() || self.facade.is_none())
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                if let Some(m) = &single_edit {
                                    let m = clone_database_match(m);
                                    ent.update(cx, |this, cx| {
                                        this.open_edit_match_modal(m, window, cx);
                                    });
                                }
                            })
                    })
                    .child({
                        let analyzable_clone: Vec<_> =
                            analyzable.iter().map(clone_database_match).collect();
                        let ent = entity.clone();
                        ActionButton::new("library-batch-analyze-btn")
                            .icon(ActionIconName::Analysis)
                            .label(format!(
                                "{}…",
                                I18n::t(locale, "library", "analyzeSelection")
                            ))
                            .disabled(analyzable_clone.is_empty() || self.facade.is_none())
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                let matches: Vec<_> =
                                    analyzable_clone.iter().map(clone_database_match).collect();
                                ent.update(cx, |this, cx| {
                                    this.open_bulk_analysis_modal(
                                        matches,
                                        unavailable_count,
                                        window,
                                        cx,
                                    );
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-copy-btn")
                            .icon(ActionIconName::Copy)
                            .label(I18n::t(locale, "library", "copy"))
                            .disabled(!can_mutate)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_batch_modal(BatchDialogKind::Copy, window, cx);
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-move-btn")
                            .icon(ActionIconName::Move)
                            .label(I18n::t(locale, "library", "move"))
                            .disabled(!can_mutate)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_batch_modal(BatchDialogKind::Move, window, cx);
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-tags-btn")
                            .icon(ActionIconName::Tags)
                            .label(I18n::t(locale, "library", "tags"))
                            .disabled(!can_tags)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_batch_modal(BatchDialogKind::Tags, window, cx);
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-export-btn")
                            .icon(ActionIconName::Export)
                            .label(I18n::t(locale, "library", "export"))
                            .disabled(!can_export)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_batch_modal(BatchDialogKind::Export, window, cx);
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-delete-btn")
                            .icon(ActionIconName::Delete)
                            .label(I18n::t(locale, "library", "delete"))
                            .variant(ActionButtonVariant::Danger)
                            .disabled(!can_mutate)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |this, cx| {
                                    this.open_batch_modal(BatchDialogKind::Delete, window, cx);
                                });
                            })
                    })
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-batch-clear-btn")
                            .icon(ActionIconName::Clear)
                            .tooltip(I18n::t(locale, "library", "clearSelection"))
                            .dense_nav()
                            .disabled(!has_selection)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| {
                                    this.selection.clear();
                                    cx.notify();
                                });
                            })
                    }),
            )
    }

    pub(super) fn render_match_table_section(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let has_occurrences = self.filters.has_position_filter();
        let visible_cols = self.columns.ordered_visible();
        let show_white = self.columns.is_visible(LibraryDataColumn::White);

        let all_on_page_selected =
            !self.matches.is_empty() && self.matches.iter().all(|m| self.selection.contains(m.id));
        let some_on_page_selected = self.matches.iter().any(|m| self.selection.contains(m.id));

        let total_pages = self
            .total_matches
            .map(|t| (t.max(0) as usize).div_ceil(PAGE_SIZE))
            .unwrap_or(1)
            .max(1);

        // Compute deterministic total table width for horizontal scrolling.
        let mut table_width: f32 = SELECT_COL_WIDTH;
        if has_occurrences {
            for &occ in OccurrenceColumn::ALL {
                table_width += self.columns.width_of_occurrence(occ) as f32;
            }
        }
        for &col in &visible_cols {
            table_width += self.columns.width_of(col) as f32;
        }

        // Header row
        let mut header_row = div()
            .id("library-table-header")
            .h_flex()
            .items_center()
            .w(px(table_width))
            .min_w_full()
            .bg(p.surface_alt)
            .border_b_1()
            .border_color(p.border_strong);

        if has_occurrences {
            for &occ in OccurrenceColumn::ALL {
                let w = self.columns.width_of_occurrence(occ) as f32;
                let sort_key = occ.sort_key();
                let is_sorted = self.sort == sort_key;
                let ent_sort = entity.clone();
                let ent_grow = entity.clone();

                header_row = header_row.child(
                    div()
                        .id(SharedString::from(format!("library-th-{}", occ.id())))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .w(px(w))
                        .flex_shrink_0()
                        .px(px(8.0))
                        .py(px(7.0))
                        .border_r_1()
                        .border_color(p.border)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.selected))
                        .on_click(move |_, _, cx| {
                            ent_sort.update(cx, |this, cx| {
                                this.toggle_sort(sort_key, cx);
                            });
                        })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(4.0))
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_table_heading()
                                        .text_color(if is_sorted { p.accent } else { p.muted })
                                        .truncate()
                                        .child(occ.label(locale)),
                                )
                                .when(is_sorted, |el| {
                                    el.child(action_icon(
                                        match self.direction {
                                            SortDirection::Asc => ActionIconName::SortUp,
                                            SortDirection::Desc => ActionIconName::SortDown,
                                        },
                                        12.0,
                                        p.accent,
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("library-resize-{}", occ.id())))
                                .w(px(6.0))
                                .h(px(14.0))
                                .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
                                .on_click(move |_, _, cx| {
                                    ent_grow.update(cx, |this, cx| {
                                        this.columns.adjust_occurrence_width(occ, RESIZE_STEP_PX);
                                        this.persist_library_prefs(cx);
                                        cx.notify();
                                    });
                                }),
                        ),
                );
            }
        }

        // Select-all header cell
        header_row = header_row.child({
            let ent = entity.clone();
            let next_checked = !all_on_page_selected;
            let disabled = self.loading_matches || self.matches.is_empty();
            div()
                .id("library-th-select")
                .h_flex()
                .items_center()
                .justify_center()
                .w(px(SELECT_COL_WIDTH))
                .flex_shrink_0()
                .py(px(7.0))
                .border_r_1()
                .border_color(p.border)
                .child(
                    div()
                        .id("library-select-all-checkbox")
                        .test_support()
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(if all_on_page_selected || some_on_page_selected {
                            p.accent
                        } else {
                            p.control_border
                        })
                        .bg(if all_on_page_selected || some_on_page_selected {
                            p.accent
                        } else {
                            p.surface
                        })
                        .text_meta()
                        .text_color(p.accent_text)
                        .when(!disabled, |el| {
                            el.cursor_pointer().on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| {
                                    let page_items: Vec<_> =
                                        this.matches.iter().map(clone_database_match).collect();
                                    this.selection.set_many(&page_items, next_checked);
                                    cx.notify();
                                });
                            })
                        })
                        .child(if all_on_page_selected {
                            "✓"
                        } else if some_on_page_selected {
                            "−"
                        } else {
                            ""
                        }),
                )
        });

        // Ordered visible data column headers
        for &col in &visible_cols {
            let w = self.columns.width_of(col) as f32;
            let sort_key = col.sort_key();
            let is_sorted = sort_key.is_some_and(|k| self.sort == k);
            let label = col.header_label(locale);
            let tip = col.picker_label(locale);
            let ent_sort = entity.clone();
            let ent_resize = entity.clone();

            let mut th = div()
                .id(SharedString::from(format!("library-th-{}", col.id())))
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .w(px(w))
                .flex_shrink_0()
                .px(px(8.0))
                .py(px(7.0))
                .border_r_1()
                .border_color(p.border)
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));

            if let Some(key) = sort_key {
                th = th
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.selected))
                    .on_click(move |_, _, cx| {
                        ent_sort.update(cx, |this, cx| this.toggle_sort(key, cx));
                    });
            }

            th = th
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_table_heading()
                                .text_color(if is_sorted { p.accent } else { p.muted })
                                .truncate()
                                .child(label),
                        )
                        .when(is_sorted, |el| {
                            el.child(action_icon(
                                match self.direction {
                                    SortDirection::Asc => ActionIconName::SortUp,
                                    SortDirection::Desc => ActionIconName::SortDown,
                                },
                                12.0,
                                p.accent,
                            ))
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("library-resize-{}", col.id())))
                        .w(px(6.0))
                        .h(px(14.0))
                        .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
                        .on_click(move |_, _, cx| {
                            ent_resize.update(cx, |this, cx| {
                                this.columns.adjust_width(col, RESIZE_STEP_PX);
                                this.persist_library_prefs(cx);
                                cx.notify();
                            });
                        }),
                );

            header_row = header_row.child(th);
        }

        // Body rows
        let mut body = div().v_flex().w(px(table_width)).min_w_full();
        for (row_idx, m) in self.matches.iter().enumerate() {
            let row_key = library_row_key(m);
            let checked = self.selection.contains(m.id);
            let check_disabled = !checked && self.selection.len() >= MAX_SELECTION;
            let row_bg = if checked {
                p.selected
            } else if row_idx % 2 == 1 {
                p.surface_alt
            } else {
                p.surface
            };

            let mut row = div()
                .id(SharedString::from(format!("library-row-{row_key}")))
                .test_support()
                .h_flex()
                .items_center()
                .w(px(table_width))
                .min_w_full()
                .min_h(px(32.0))
                .bg(row_bg)
                .border_b_1()
                .border_color(p.border)
                .hover(move |s| s.bg(p.selected));

            if has_occurrences {
                let db_name = m
                    .occurrence
                    .as_ref()
                    .map(|o| o.database_name.clone())
                    .or_else(|| {
                        self.databases
                            .iter()
                            .find(|d| d.id == m.database_id)
                            .map(|d| database_display_name(d, locale))
                    })
                    .unwrap_or_else(|| "—".to_string());
                let game_str = m
                    .occurrence
                    .as_ref()
                    .map(|o| o.game_no.to_string())
                    .unwrap_or_else(|| "—".to_string());
                let move_str = m
                    .occurrence
                    .as_ref()
                    .map(|o| o.move_no.to_string())
                    .unwrap_or_else(|| "—".to_string());

                let match_for_occ = clone_database_match(m);
                let ent_occ = entity.clone();

                row = row
                    .child(
                        div()
                            .w(px(
                                self.columns.width_of_occurrence(OccurrenceColumn::Database) as f32,
                            ))
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(5.0))
                            .text_table()
                            .text_color(p.text)
                            .truncate()
                            .child(db_name),
                    )
                    .child(
                        div()
                            .w(px(
                                self.columns.width_of_occurrence(OccurrenceColumn::Game) as f32
                            ))
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(5.0))
                            .text_table()
                            .text_color(p.muted)
                            .child(game_str),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("library-occ-open-{row_key}")))
                            .w(px(
                                self.columns.width_of_occurrence(OccurrenceColumn::Move) as f32
                            ))
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(5.0))
                            .text_table()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.accent)
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                let m = clone_database_match(&match_for_occ);
                                ent_occ.update(cx, |this, cx| {
                                    this.open_library_match(&m, cx);
                                });
                            })
                            .child(move_str),
                    );
            }

            // Selection checkbox cell
            row = row.child({
                let match_for_sel = clone_database_match(m);
                let ent_sel = entity.clone();
                div()
                    .h_flex()
                    .items_center()
                    .justify_center()
                    .w(px(SELECT_COL_WIDTH))
                    .flex_shrink_0()
                    .py(px(5.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("library-row-check-{row_key}")))
                            .test_support()
                            .h_flex()
                            .items_center()
                            .justify_center()
                            .w(px(18.0))
                            .h(px(18.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(if checked { p.accent } else { p.control_border })
                            .bg(if checked { p.accent } else { p.surface })
                            .text_meta()
                            .text_color(p.accent_text)
                            .when(!check_disabled, |el| {
                                el.cursor_pointer().on_click(move |_, _, cx| {
                                    let m = clone_database_match(&match_for_sel);
                                    ent_sel.update(cx, |this, cx| {
                                        this.selection.set_many(&[m], !checked);
                                        cx.notify();
                                    });
                                })
                            })
                            .child(if checked { "✓" } else { "" }),
                    )
            });

            // Ordered visible data cells
            for &col in &visible_cols {
                let w = self.columns.width_of(col) as f32;
                row = row.child(
                    self.render_data_cell(m, &row_key, col, w, show_white, p, locale, &entity),
                );
            }

            body = body.child(row);
        }

        let table_content: AnyElement = if let Some(err) = &self.load_error {
            let ent = entity.clone();
            StatusBanner::new("library-load-error-banner", BannerKind::Error, err.clone())
                .palette(p)
                .on_retry(I18n::t(locale, "workspace", "retry"), move |_, _, cx| {
                    ent.update(cx, |this, cx| this.reload_all(cx));
                })
                .into_any_element()
        } else if self.matches.is_empty() && !self.loading_matches {
            let has_filters = self.filters.has_active_filters();
            let in_single_db = self.selected_database_id.is_some();
            let ent_clear = entity.clone();
            let ent_all = entity.clone();
            let actions = div()
                .h_flex()
                .gap(px(8.0))
                .when(has_filters, |el| {
                    el.child(
                        ActionButton::new("library-empty-clear-filters-btn")
                            .icon(ActionIconName::Clear)
                            .label(I18n::t(locale, "library", "clearFilters"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_clear.update(cx, |this, cx| {
                                    this.clear_all_filters(Some(window), cx);
                                });
                            }),
                    )
                })
                .when(in_single_db, |el| {
                    el.child(
                        ActionButton::new("library-empty-all-dbs-btn")
                            .icon(ActionIconName::Deck)
                            .label(I18n::t(locale, "library", "searchAllDatabases"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_all.update(cx, |this, cx| this.select_database(None, cx));
                            }),
                    )
                });
            let desc = if self.facade.is_none() {
                "Connect to PostgreSQL to browse your match library.".to_string()
            } else {
                I18n::t(locale, "workspace", "databaseDescription").to_string()
            };
            EmptyState::new(
                "library-empty-state",
                I18n::t(locale, "library", "noMatches"),
                desc,
            )
            .icon(ActionIconName::Database)
            .action(actions)
            .palette(p)
            .into_any_element()
        } else {
            div()
                .id("library-table-scroll-container")
                .test_support()
                .v_flex()
                .flex_1()
                .min_h(px(0.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .overflow_scroll()
                .child(header_row)
                .child(body)
                .into_any_element()
        };

        let ent_page = entity.clone();
        div()
            .v_flex()
            .flex_1()
            .min_h(px(0.0))
            .gap(px(8.0))
            .when(has_occurrences, |el| {
                el.child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "library",
                    "occurrenceSelection",
                )))
            })
            .child(table_content)
            .when(total_pages > 1 || self.page > 1, |el| {
                el.child(
                    div().h_flex().justify_end().pt(px(2.0)).child(
                        NumberedSearchPagination::new(
                            "library-table-pagination",
                            self.page,
                            total_pages,
                        )
                        .disabled(self.loading_matches)
                        .palette(p)
                        .on_select(move |target_page, _, cx| {
                            ent_page.update(cx, |this, cx| {
                                this.go_to_page(target_page, cx);
                            });
                        }),
                    ),
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn render_data_cell(
        &self,
        m: &DatabaseMatch,
        row_key: &str,
        col: LibraryDataColumn,
        width: f32,
        show_white: bool,
        p: NeonPalette,
        locale: Locale,
        entity: &gpui_kit::Entity<Self>,
    ) -> AnyElement {
        let base = || {
            div()
                .w(px(width))
                .flex_shrink_0()
                .px(px(8.0))
                .py(px(5.0))
                .text_table()
                .text_color(p.text)
                .overflow_hidden()
        };

        match col {
            LibraryDataColumn::Id => base()
                .text_color(p.muted)
                .child(m.match_no.to_string())
                .into_any_element(),

            LibraryDataColumn::White => {
                self.render_open_player_cell(m, row_key, true, width, p, entity)
            }

            LibraryDataColumn::Black => {
                if show_white {
                    base().truncate().child(m.black.clone()).into_any_element()
                } else {
                    self.render_open_player_cell(m, row_key, false, width, p, entity)
                }
            }

            LibraryDataColumn::WhitePr => {
                let value = can_show_match_pr(m).then_some(m.white_pr).flatten();
                self.render_pr_cell(m, value, width, p, locale)
            }

            LibraryDataColumn::BlackPr => {
                let value = can_show_match_pr(m).then_some(m.black_pr).flatten();
                self.render_pr_cell(m, value, width, p, locale)
            }

            LibraryDataColumn::PlayMode => {
                let label = m
                    .play_mode
                    .map(|mode| format_play_mode(mode, m.play_assisted, locale))
                    .unwrap_or_else(|| "—".to_string());
                base()
                    .text_color(if m.play_mode.is_some() {
                        p.text
                    } else {
                        p.muted
                    })
                    .truncate()
                    .child(label)
                    .into_any_element()
            }

            LibraryDataColumn::MatchLength => {
                let text = if m.match_length > 0 {
                    m.match_length.to_string()
                } else {
                    "M".to_string()
                };
                base().child(text).into_any_element()
            }

            LibraryDataColumn::Score => {
                let text = match (m.white_score, m.black_score) {
                    (Some(w), Some(b)) => format!("{w}–{b}"),
                    _ => "n/a".to_string(),
                };
                base().child(text).into_any_element()
            }

            LibraryDataColumn::PlayedAt => base()
                .text_color(p.muted)
                .truncate()
                .child(format_naive_date(m.played_on))
                .into_any_element(),

            LibraryDataColumn::CreatedAt => base()
                .text_color(p.muted)
                .truncate()
                .child(format_utc_date(m.created_at))
                .into_any_element(),

            LibraryDataColumn::Tournament => base()
                .text_color(p.muted)
                .truncate()
                .child(m.tournament.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),

            LibraryDataColumn::PositionCount => base()
                .text_color(p.muted)
                .child(m.position_count.to_string())
                .into_any_element(),

            LibraryDataColumn::AnalysisStatus => {
                let status_str =
                    format_library_analysis_status(m.analysis_status.as_deref(), locale);
                let color = match m.analysis_status.as_deref() {
                    Some("completed") => p.success_text,
                    Some("running") | Some("queued") => p.accent,
                    Some("partial") => p.warning_text,
                    Some("failed") => p.danger_text,
                    _ => p.muted,
                };
                base()
                    .text_color(color)
                    .truncate()
                    .child(status_str)
                    .into_any_element()
            }

            LibraryDataColumn::Tags => {
                let mut container = div().h_flex().items_center().gap(px(4.0)).overflow_hidden();
                for tag in m.tags.iter().take(4) {
                    container = container.child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .bg(p.surface_alt)
                            .border_1()
                            .border_color(p.border)
                            .text_micro()
                            .text_color(p.text)
                            .child(tag.clone()),
                    );
                }
                if m.tags.len() > 4 {
                    container = container.child(
                        div()
                            .text_micro()
                            .text_color(p.muted)
                            .child(format!("+{}", m.tags.len() - 4)),
                    );
                }
                base().child(container).into_any_element()
            }

            LibraryDataColumn::Comment => base()
                .text_color(p.muted)
                .truncate()
                .child(m.comment.clone())
                .into_any_element(),
        }
    }

    fn render_open_player_cell(
        &self,
        m: &DatabaseMatch,
        row_key: &str,
        is_white: bool,
        width: f32,
        p: NeonPalette,
        entity: &gpui_kit::Entity<Self>,
    ) -> AnyElement {
        let player = if is_white { &m.white } else { &m.black };
        let show_title = !m.display_name.trim().is_empty() && m.display_name != m.filename;
        let label = if show_title {
            format!("{player} · {}", m.display_name)
        } else {
            player.clone()
        };
        let match_clone = clone_database_match(m);
        let ent = entity.clone();

        div()
            .id(SharedString::from(format!("library-row-open-{row_key}")))
            .test_support()
            .h_flex()
            .items_center()
            .gap(px(4.0))
            .w(px(width))
            .flex_shrink_0()
            .px(px(8.0))
            .py(px(5.0))
            .text_table()
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.accent)
            .cursor_pointer()
            .hover(move |s| s.text_color(p.accent_hover))
            .on_click(move |_, _, cx| {
                let m = clone_database_match(&match_clone);
                ent.update(cx, |this, cx| this.open_library_match(&m, cx));
            })
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
            .into_any_element()
    }

    fn render_pr_cell(
        &self,
        m: &DatabaseMatch,
        value: Option<f64>,
        width: f32,
        p: NeonPalette,
        locale: Locale,
    ) -> AnyElement {
        let formatted = format_pr(value, locale);
        let warn = has_pr_warning(m);
        let warning_tip = if m.import_warning_count > 0 {
            Some(I18n::format(
                locale,
                "library",
                "importNotesCount",
                &[("count", Param::from(m.import_warning_count as i64))],
            ))
        } else if m.analysis_status.as_deref() == Some("partial") {
            Some(I18n::t(locale, "library", "partial").to_string())
        } else {
            None
        };

        div()
            .h_flex()
            .items_center()
            .gap(px(4.0))
            .w(px(width))
            .flex_shrink_0()
            .px(px(8.0))
            .py(px(5.0))
            .text_table()
            .text_color(if value.is_some() { p.text } else { p.muted })
            .child(formatted)
            .when(warn, |el| {
                let icon = action_icon(ActionIconName::Warning, 12.0, p.warning_text);
                if let Some(tip) = warning_tip {
                    el.child(
                        div()
                            .id(SharedString::from(format!("library-pr-warn-{}", m.id)))
                            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                            .child(icon),
                    )
                } else {
                    el.child(icon)
                }
            })
            .into_any_element()
    }
}
