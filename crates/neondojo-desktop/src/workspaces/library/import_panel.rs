//! Native macOS match & recursive folder import bar, collapsible Finder
//! [`MatchDropZone`], cooperative-cancel progress panel, and [`ImportWarning`] list.

use crate::events::WorkspaceEvent;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::drop_zone::MatchDropZone;
use crate::ui::icons::{ActionIconName, action_icon};
use crate::workspaces::library::LibraryWorkspace;
use crate::workspaces::library::state::{
    ImportFileStatus, ImportRunState, accepts_library_matches, database_display_name,
    format_import_warning,
};
use gpui_kit::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::model::ImportWarning;
use neondojo_i18n::{I18n, Locale, Param};

impl LibraryWorkspace {
    pub(super) fn render_import_header(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let running = self.import_run.as_ref().is_some_and(|r| r.running);
        let target_dbs: Vec<_> = self
            .databases
            .iter()
            .filter(|db| accepts_library_matches(db.id))
            .collect();
        let active_target_id = self.effective_import_database_id();
        let active_target_name = active_target_id
            .and_then(|id| self.databases.iter().find(|d| d.id == id))
            .map(|db| database_display_name(db, locale))
            .unwrap_or_else(|| "Unsorted".to_string());

        let mut dest_strip = div()
            .h_flex()
            .items_center()
            .gap(px(4.0))
            .flex_wrap()
            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "library",
                "importInto",
            )));

        if target_dbs.len() <= 4 {
            for db in target_dbs {
                let db_id = db.id;
                let selected = active_target_id == Some(db_id);
                let name = database_display_name(db, locale);
                let ent = entity.clone();
                dest_strip = dest_strip.child(
                    div()
                        .id(SharedString::from(format!("library-import-dest-{db_id}")))
                        .test_support()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .bg(if selected { p.selected } else { p.surface })
                        .text_meta()
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if selected { p.accent } else { p.text })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            ent.update(cx, |this, cx| {
                                this.import_database_id = Some(db_id);
                                cx.notify();
                            });
                        })
                        .child(name),
                );
            }
        } else {
            let ent = entity.clone();
            let ids: Vec<_> = target_dbs.iter().map(|d| d.id).collect();
            dest_strip = dest_strip.child(
                ActionButton::new("library-import-dest-cycle")
                    .icon(ActionIconName::Database)
                    .label(active_target_name)
                    .tooltip(I18n::t(locale, "library", "importDestination"))
                    .palette(p)
                    .disabled(running)
                    .on_click(move |_, _, cx| {
                        let ids = ids.clone();
                        ent.update(cx, |this, cx| {
                            let cur = this.effective_import_database_id();
                            let next = cur
                                .and_then(|id| ids.iter().position(|&x| x == id))
                                .map(|idx| ids[(idx + 1) % ids.len()])
                                .or_else(|| ids.first().copied());
                            this.import_database_id = next;
                            cx.notify();
                        });
                    }),
            );
        }

        div()
            .id("library-import-header")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_wrap()
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-import-files-btn")
                                    .icon(ActionIconName::Import)
                                    .label(I18n::t(locale, "app", "app.importPlain"))
                                    .primary()
                                    .palette(p)
                                    .disabled(running || self.facade.is_none())
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.prompt_import_files(cx);
                                        });
                                    })
                            })
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-import-folder-btn")
                                    .icon(ActionIconName::ImportFolder)
                                    .label(I18n::t(locale, "library", "importFolder"))
                                    .tooltip(I18n::t(locale, "library", "importFolderHelp"))
                                    .palette(p)
                                    .disabled(running || self.facade.is_none())
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.prompt_import_folder(cx);
                                        });
                                    })
                            })
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-toggle-dropzone-btn")
                                    .icon(if self.show_drop_zone {
                                        ActionIconName::Collapse
                                    } else {
                                        ActionIconName::Expand
                                    })
                                    .label(I18n::t(locale, "app", "app.dropZoneTitle"))
                                    .active(self.show_drop_zone)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.show_drop_zone = !this.show_drop_zone;
                                            cx.notify();
                                        });
                                    })
                            }),
                    )
                    .child(dest_strip),
            )
            .when(self.show_drop_zone, |this| {
                let ent_drop = entity.clone();
                let ent_browse = entity.clone();
                this.child(
                    MatchDropZone::new("library-match-dropzone")
                        .busy(running)
                        .palette(p)
                        .on_drop_batch(move |batch, _, cx| {
                            ent_drop.update(cx, |this, cx| {
                                this.start_import_batch(batch, cx);
                            });
                        })
                        .on_browse(move |_, _, cx| {
                            ent_browse.update(cx, |this, cx| {
                                this.prompt_import_files(cx);
                            });
                        }),
                )
            })
            .when_some(self.import_run.as_ref(), |this, run| {
                this.child(self.render_import_run_panel(run, p, locale, cx))
            })
    }

    fn render_import_run_panel(
        &self,
        run: &ImportRunState,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let heading = if run.running {
            I18n::format(
                locale,
                "library",
                "importProgress",
                &[
                    ("done", Param::from(run.done as i64)),
                    ("total", Param::from(run.total as i64)),
                ],
            )
        } else if run.stopped {
            I18n::format(
                locale,
                "library",
                "importStopped",
                &[
                    ("done", Param::from(run.done as i64)),
                    ("total", Param::from(run.total as i64)),
                ],
            )
        } else {
            I18n::format(
                locale,
                "library",
                "importFinished",
                &[("total", Param::from(run.total as i64))],
            )
        };

        let counts = I18n::format(
            locale,
            "library",
            "importCounts",
            &[
                ("imported", Param::from(run.imported as i64)),
                ("duplicates", Param::from(run.duplicates as i64)),
                ("failed", Param::from(run.failed as i64)),
            ],
        );

        let skipped_total = run.skipped_unsupported + run.ignored_hidden;
        let pct = if run.total > 0 {
            ((run.done as f32 / run.total as f32) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        let mut rows_list = div()
            .id("library-import-results-list")
            .v_flex()
            .gap(px(4.0))
            .max_h(px(220.0))
            .overflow_y_scroll();

        for (idx, row) in run.rows.iter().enumerate() {
            let (badge_text, badge_fg, badge_bg, open_match_id, warnings): (
                String,
                gpui_kit::Hsla,
                gpui_kit::Hsla,
                Option<uuid::Uuid>,
                &[ImportWarning],
            ) = match &row.status {
                ImportFileStatus::Imported {
                    match_id,
                    position_count,
                    warnings,
                } => (
                    format!(
                        "{} · {}",
                        I18n::t(locale, "library", "importedFile"),
                        I18n::format(
                            locale,
                            "app",
                            "app.importCount",
                            &[("count", Param::from(*position_count as i64))],
                        )
                    ),
                    p.success_text,
                    p.success_bg,
                    Some(*match_id),
                    warnings.as_slice(),
                ),
                ImportFileStatus::Duplicate { match_id, warnings } => (
                    I18n::t(locale, "library", "duplicate").to_string(),
                    p.warning_text,
                    p.warning_bg,
                    Some(*match_id),
                    warnings.as_slice(),
                ),
                ImportFileStatus::Failed(err) => {
                    (err.clone(), p.danger_text, p.danger_bg, None, &[])
                }
            };

            let mut item =
                div()
                    .id(SharedString::from(format!("library-import-row-{idx}")))
                    .v_flex()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_label()
                                    .text_color(p.text)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .child(row.display_name.clone()),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded(px(4.0))
                                            .bg(badge_bg)
                                            .text_meta()
                                            .text_color(badge_fg)
                                            .child(badge_text),
                                    )
                                    .when_some(open_match_id, |el, mid| {
                                        let ent = entity.clone();
                                        el.child(
                                            ActionButton::new(SharedString::from(format!(
                                                "library-import-open-{idx}"
                                            )))
                                            .icon(ActionIconName::Open)
                                            .label(I18n::t(locale, "workspace", "openMatch"))
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent.update(cx, |_, cx| {
                                                    cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                                        match_id: mid,
                                                        game_no: None,
                                                        position_id: None,
                                                        decision_no: None,
                                                    });
                                                });
                                            }),
                                        )
                                    }),
                            ),
                    );

            if !warnings.is_empty() {
                item = item.child(render_import_warning_list(warnings, p, locale));
            }
            rows_list = rows_list.child(item);
        }

        div()
            .id("library-import-run-panel")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface_alt)
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
                                    .child(format!("{} · {}", heading, run.destination_name)),
                            )
                            .child(div().text_meta().text_color(p.muted).child(counts)),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(run.running, |el| {
                                let ent = entity.clone();
                                el.child(
                                    ActionButton::new("library-import-stop-btn")
                                        .icon(ActionIconName::Cancel)
                                        .label(if run.stopping {
                                            I18n::t(locale, "library", "importStopping")
                                        } else {
                                            I18n::t(locale, "library", "importStop")
                                        })
                                        .variant(ActionButtonVariant::Danger)
                                        .disabled(run.stopping)
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            ent.update(cx, |this, cx| {
                                                if let Some(run) = &mut this.import_run {
                                                    run.request_stop();
                                                    cx.notify();
                                                }
                                            });
                                        }),
                                )
                            })
                            .when(!run.running, |el| {
                                let ent = entity.clone();
                                el.child(
                                    ActionButton::new("library-import-dismiss-btn")
                                        .icon(ActionIconName::Close)
                                        .tooltip(I18n::t(locale, "library", "close"))
                                        .variant(ActionButtonVariant::Ghost)
                                        .dense_nav()
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            ent.update(cx, |this, cx| {
                                                this.import_run = None;
                                                cx.notify();
                                            });
                                        }),
                                )
                            }),
                    ),
            )
            .when(run.total > 0, |el| {
                el.child(
                    div()
                        .w_full()
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(p.border)
                        .overflow_hidden()
                        .child(div().h_full().w(gpui_kit::relative(pct / 100.0)).bg(
                            if run.failed > 0 {
                                p.warning_text
                            } else {
                                p.accent
                            },
                        )),
                )
            })
            .when_some(run.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-import-error-banner", BannerKind::Error, err)
                        .palette(p),
                )
            })
            .when(skipped_total > 0, |el| {
                el.child(div().text_meta().text_color(p.muted).child(I18n::format(
                    locale,
                    "library",
                    "importSkipped",
                    &[("count", Param::from(skipped_total as i64))],
                )))
            })
            .when(!run.rows.is_empty(), |el| el.child(rows_list))
    }
}

/// Renders a list of [`ImportWarning`] entries (`ImportWarningList.tsx` parity).
pub(super) fn render_import_warning_list(
    warnings: &[ImportWarning],
    p: NeonPalette,
    locale: Locale,
) -> impl IntoElement {
    let mut list = div()
        .v_flex()
        .gap(px(4.0))
        .p(px(8.0))
        .rounded(px(6.0))
        .bg(p.warning_bg)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .child(action_icon(ActionIconName::Warning, 13.0, p.warning_text))
                .child(
                    div()
                        .text_meta()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.warning_text)
                        .child(I18n::format(
                            locale,
                            "library",
                            "importNotesCount",
                            &[("count", Param::from(warnings.len() as i64))],
                        )),
                ),
        );

    for warning in warnings {
        list = list.child(
            div()
                .text_meta()
                .text_color(p.warning_text)
                .child(format!("• {}", format_import_warning(warning, locale))),
        );
    }

    list
}
