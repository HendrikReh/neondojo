//! Modal dialogs for Match Library CRUD, metadata editing, batch operations
//! (Copy, Move, Tags, Delete, native Export), and Bulk Match Analysis.

use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{
    ModalBusyLock, ModalConfig, ModalSize, open_library_dialog, open_modal_dialog,
};
use crate::workspaces::library::state::{
    accepts_library_matches, clone_database, clone_database_match, database_display_name,
    parse_tag_csv,
};
use crate::workspaces::library::{LibraryWorkspace, run_facade_async};
use chrono::NaiveDate;
use gpui_kit::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::api::{MatchAnalysisEngine, MatchAnalysisRequest};
use neondojo::databases::{
    Database, DatabaseMatch, DatabaseWrite, ExportFormat, LibraryAction, MetadataWrite,
};
use neondojo::facade::BackendFacade;
use neondojo::settings::EngineInfo;
use neondojo_i18n::{I18n, Param};
use neondojo_rules::options::engine_depths;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BatchDialogKind {
    Copy,
    Move,
    Tags,
    Export,
    Delete,
}

// ── 1. Create / Edit Database Modal ──────────────────────────────────────────

struct DatabaseFormView {
    workspace: Entity<LibraryWorkspace>,
    facade: BackendFacade,
    editing: Option<Database>,
    name_input: Entity<InputState>,
    desc_input: Entity<InputState>,
    busy: ModalBusyLock,
    error: Option<SharedString>,
}

impl DatabaseFormView {
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let name = if let Some(db) = &self.editing
            && db.is_system
        {
            db.name.clone()
        } else {
            self.name_input.read(cx).value().trim().to_string()
        };
        if name.is_empty() {
            self.error = Some("Database name is required.".into());
            cx.notify();
            return;
        }
        let description = self.desc_input.read(cx).value().trim().to_string();
        let facade = self.facade.clone();
        let editing = self.editing.as_ref().map(clone_database);
        let workspace = self.workspace.clone();
        let busy = self.busy.clone();
        busy.set_busy(true);
        self.error = None;
        cx.notify();

        let locale = current_locale(cx);
        let task = TokioBridge::spawn_write(cx, async move {
            run_facade_async(move || async move {
                match editing {
                    Some(db) => {
                        facade
                            .rename_database(
                                db.id,
                                DatabaseWrite {
                                    name,
                                    description,
                                    version: Some(db.version),
                                },
                            )
                            .await
                    }
                    None => {
                        facade
                            .create_database(DatabaseWrite {
                                name,
                                description,
                                version: None,
                            })
                            .await
                    }
                }
            })
            .await
        });

        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok(Ok(db)) => {
                        let new_id = db.id;
                        workspace.update(cx, |ws, cx| {
                            ws.selected_database_id = Some(new_id);
                            ws.on_library_mutation_completed(cx);
                        });
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.close_dialog(cx);
                        });
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for DatabaseFormView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let is_system = self.editing.as_ref().is_some_and(|d| d.is_system);
        let busy = self.busy.is_busy();
        let ent = cx.entity();

        div()
            .id("library-database-form-modal")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-db-form-error", BannerKind::Error, err).palette(p),
                )
            })
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "library", "name")),
                    )
                    .child(if is_system {
                        let display = self
                            .editing
                            .as_ref()
                            .map(|d| database_display_name(d, locale))
                            .unwrap_or_default();
                        div()
                            .px(px(10.0))
                            .py(px(7.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.surface_alt)
                            .text_body()
                            .text_color(p.muted)
                            .child(display)
                    } else {
                        div().child(Input::new(&self.name_input).id("library-db-name-input"))
                    }),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "library",
                        "description",
                    )))
                    .child(Input::new(&self.desc_input).id("library-db-desc-input")),
            )
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        ActionButton::new("library-db-form-cancel")
                            .label(I18n::t(locale, "library", "cancel"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child({
                        let ent = ent.clone();
                        ActionButton::new("library-db-form-save")
                            .icon(ActionIconName::Save)
                            .label(I18n::t(locale, "library", "save"))
                            .primary()
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |v, cx| v.submit(window, cx));
                            })
                    }),
            )
    }
}

// ── 2. Delete / Empty Database Confirmation Modal ────────────────────────────

struct ConfirmDatabaseView {
    workspace: Entity<LibraryWorkspace>,
    facade: BackendFacade,
    database: Database,
    busy: ModalBusyLock,
    error: Option<SharedString>,
}

impl ConfirmDatabaseView {
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let facade = self.facade.clone();
        let db = clone_database(&self.database);
        let workspace = self.workspace.clone();
        let locale = current_locale(cx);
        self.busy.set_busy(true);
        self.error = None;
        cx.notify();

        let task = TokioBridge::spawn_write(cx, async move {
            run_facade_async(move || async move {
                if db.is_system {
                    facade.empty_database(db.id, db.version).await
                } else {
                    facade.delete_database(db.id, db.version, true).await
                }
            })
            .await
        });

        let window_handle = window.window_handle();
        let deleted_user_db = !self.database.is_system;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok(Ok(())) => {
                        workspace.update(cx, |ws, cx| {
                            if deleted_user_db {
                                ws.selected_database_id = None;
                            }
                            ws.selection.clear();
                            ws.on_library_mutation_completed(cx);
                        });
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.close_dialog(cx);
                        });
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for ConfirmDatabaseView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let busy = self.busy.is_busy();
        let warning_key = if self.database.is_system {
            "clearDatabaseWarning"
        } else {
            "databaseWarning"
        };
        let warning_text = I18n::format(
            locale,
            "library",
            warning_key,
            &[("count", Param::from(self.database.match_count))],
        );
        let action_label = if self.database.is_system {
            I18n::t(locale, "library", "clearDatabase")
        } else {
            I18n::t(locale, "library", "deleteDatabase")
        };
        let ent = cx.entity();

        div()
            .id("library-confirm-db-modal")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-confirm-db-error", BannerKind::Error, err)
                        .palette(p),
                )
            })
            .child(div().text_body().text_color(p.text).child(format!(
                "{}: {}",
                database_display_name(&self.database, locale),
                warning_text
            )))
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        ActionButton::new("library-confirm-db-cancel")
                            .label(I18n::t(locale, "library", "cancel"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("library-confirm-db-confirm")
                            .icon(ActionIconName::Delete)
                            .label(action_label)
                            .variant(ActionButtonVariant::DangerSolid)
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |v, cx| v.confirm(window, cx));
                            }),
                    ),
            )
    }
}

// ── 3. Edit Match Metadata Modal ─────────────────────────────────────────────

struct EditMatchMetadataView {
    workspace: Entity<LibraryWorkspace>,
    facade: BackendFacade,
    target: DatabaseMatch,
    display_name: Entity<InputState>,
    played_on: Entity<InputState>,
    tournament: Entity<InputState>,
    round: Entity<InputState>,
    place: Entity<InputState>,
    white_score: Entity<InputState>,
    black_score: Entity<InputState>,
    result_state: String,
    busy: ModalBusyLock,
    error: Option<SharedString>,
}

impl EditMatchMetadataView {
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let played_raw = self.played_on.read(cx).value().trim().to_string();
        let played_on = if played_raw.is_empty() {
            None
        } else {
            match NaiveDate::parse_from_str(&played_raw, "%Y-%m-%d") {
                Ok(d) => Some(d),
                Err(_) => {
                    self.error = Some("Date must use YYYY-MM-DD format.".into());
                    cx.notify();
                    return;
                }
            }
        };
        let parse_opt_score = |inp: &Entity<InputState>, cx: &App| -> Result<Option<i32>, String> {
            let s = inp.read(cx).value().trim().to_string();
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<i32>()
                    .map(Some)
                    .map_err(|_| "Scores must be whole numbers.".to_string())
            }
        };
        let white_score = match parse_opt_score(&self.white_score, cx) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(SharedString::from(e));
                cx.notify();
                return;
            }
        };
        let black_score = match parse_opt_score(&self.black_score, cx) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(SharedString::from(e));
                cx.notify();
                return;
            }
        };

        let non_empty = |inp: &Entity<InputState>, cx: &App| {
            let s = inp.read(cx).value().trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        };

        let payload = MetadataWrite {
            version: self.target.version,
            display_name: self.display_name.read(cx).value().trim().to_string(),
            played_on,
            tournament: non_empty(&self.tournament, cx),
            round: non_empty(&self.round, cx),
            place: non_empty(&self.place, cx),
            result_state: self.result_state.clone(),
            white_score,
            black_score,
        };

        let facade = self.facade.clone();
        let match_id = self.target.id;
        let workspace = self.workspace.clone();
        let locale = current_locale(cx);
        self.busy.set_busy(true);
        self.error = None;
        cx.notify();

        let task = TokioBridge::spawn_write(cx, async move {
            run_facade_async(move || async move {
                facade.update_match_metadata(match_id, payload).await
            })
            .await
        });

        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok(Ok(_updated)) => {
                        workspace.update(cx, |ws, cx| ws.on_library_mutation_completed(cx));
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.close_dialog(cx);
                        });
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for EditMatchMetadataView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let busy = self.busy.is_busy();
        let ent = cx.entity();

        let field = |label: &str, id: &'static str, input: &Entity<InputState>| {
            div()
                .v_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(label.to_string()),
                )
                .child(Input::new(input).id(id))
        };

        let states = [
            ("unknown", I18n::t(locale, "library", "unknown")),
            ("ongoing", I18n::t(locale, "library", "ongoing")),
            ("white_won", I18n::t(locale, "library", "whiteWon")),
            ("black_won", I18n::t(locale, "library", "blackWon")),
            ("draw", I18n::t(locale, "library", "draw")),
        ];
        let mut state_row = div().h_flex().flex_wrap().gap(px(4.0));
        for (val, label) in states {
            let ent = ent.clone();
            let val_s = val.to_string();
            state_row = state_row.child(
                ActionButton::new(SharedString::from(format!("library-edit-result-{val}")))
                    .label(label)
                    .active(self.result_state == val)
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        let v = val_s.clone();
                        ent.update(cx, |view, cx| {
                            view.result_state = v;
                            cx.notify();
                        });
                    }),
            );
        }

        div()
            .id("library-edit-match-modal")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-edit-match-error", BannerKind::Error, err)
                        .palette(p),
                )
            })
            .child(field(
                I18n::t(locale, "library", "matchName"),
                "library-meta-name",
                &self.display_name,
            ))
            .child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "played"),
                        "library-meta-date",
                        &self.played_on,
                    )))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "tournament"),
                        "library-meta-tournament",
                        &self.tournament,
                    ))),
            )
            .child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "round"),
                        "library-meta-round",
                        &self.round,
                    )))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "place"),
                        "library-meta-place",
                        &self.place,
                    ))),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "library", "result")),
                    )
                    .child(state_row),
            )
            .child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "white"),
                        "library-meta-white-score",
                        &self.white_score,
                    )))
                    .child(div().flex_1().child(field(
                        I18n::t(locale, "library", "black"),
                        "library-meta-black-score",
                        &self.black_score,
                    ))),
            )
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        ActionButton::new("library-edit-match-cancel")
                            .label(I18n::t(locale, "library", "cancel"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("library-edit-match-save")
                            .icon(ActionIconName::Save)
                            .label(I18n::t(locale, "library", "save"))
                            .primary()
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |v, cx| v.submit(window, cx));
                            }),
                    ),
            )
    }
}

// ── 4. Batch Action Modal (Copy / Move / Tags / Export / Delete) ─────────────

struct BatchActionModalView {
    workspace: Entity<LibraryWorkspace>,
    facade: BackendFacade,
    kind: BatchDialogKind,
    match_ids: Vec<Uuid>,
    source_database_id: Option<Uuid>,
    target_databases: Vec<Database>,
    selected_target_db: Option<Uuid>,
    add_tags_input: Entity<InputState>,
    remove_tags_input: Entity<InputState>,
    export_format: ExportFormat,
    busy: ModalBusyLock,
    error: Option<SharedString>,
}

impl BatchActionModalView {
    fn execute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let locale = current_locale(cx);

        if self.kind == BatchDialogKind::Export {
            self.execute_export(window, cx);
            return;
        }

        let action_name = match self.kind {
            BatchDialogKind::Copy => "copy",
            BatchDialogKind::Move => "move",
            BatchDialogKind::Tags => "tags",
            BatchDialogKind::Delete => "delete",
            BatchDialogKind::Export => return,
        };

        if matches!(self.kind, BatchDialogKind::Copy | BatchDialogKind::Move)
            && self.selected_target_db.is_none()
        {
            self.error = Some("Select a target database.".into());
            cx.notify();
            return;
        }

        let add_tags = if self.kind == BatchDialogKind::Tags {
            parse_tag_csv(&self.add_tags_input.read(cx).value())
        } else {
            Vec::new()
        };
        let remove_tags = if self.kind == BatchDialogKind::Tags {
            parse_tag_csv(&self.remove_tags_input.read(cx).value())
        } else {
            Vec::new()
        };
        if self.kind == BatchDialogKind::Tags && add_tags.is_empty() && remove_tags.is_empty() {
            self.error = Some("Enter at least one tag to add or remove.".into());
            cx.notify();
            return;
        }

        let action = LibraryAction {
            action: action_name.to_string(),
            match_ids: self.match_ids.clone(),
            database_id: self.source_database_id,
            target_database_id: self.selected_target_db,
            add_tags,
            remove_tags,
        };
        let facade = self.facade.clone();
        let workspace = self.workspace.clone();
        self.busy.set_busy(true);
        self.error = None;
        cx.notify();

        let task = TokioBridge::spawn_write(cx, async move {
            run_facade_async(move || async move { facade.apply_library_action(action).await }).await
        });

        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok(Ok(response)) => {
                        workspace.update(cx, |ws, cx| {
                            ws.apply_batch_action_response(&response, cx);
                        });
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.close_dialog(cx);
                        });
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn execute_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let default_dir = dirs::download_dir()
            .or_else(dirs::desktop_dir)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = if self.match_ids.len() > 1 {
            "neondojo-matches.zip"
        } else {
            match self.export_format {
                ExportFormat::Original => "match.sgf",
                ExportFormat::Sgf => "match.sgf",
                ExportFormat::Xg => "match.xg",
            }
        };

        let rx = cx.prompt_for_new_path(&default_dir, Some(suggested));
        let facade = self.facade.clone();
        let match_ids = self.match_ids.clone();
        let format = self.export_format;
        let workspace = self.workspace.clone();
        let window_handle = window.window_handle();

        self.busy.set_busy(true);
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let picked = rx.await;
            let dest = match picked {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    let _ = this.update(cx, |view, cx| {
                        view.busy.set_busy(false);
                        cx.notify();
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = this.update(cx, |view, cx| {
                        view.busy.set_busy(false);
                        view.error = Some(SharedString::from(err.to_string()));
                        cx.notify();
                    });
                    return;
                }
                Err(_) => {
                    let _ = this.update(cx, |view, cx| {
                        view.busy.set_busy(false);
                        cx.notify();
                    });
                    return;
                }
            };

            let write_task = cx.update(|cx| {
                TokioBridge::spawn_write(cx, async move {
                    run_facade_async(move || async move {
                        facade
                            .export_matches_to_path(match_ids, format, &dest)
                            .await
                    })
                    .await
                })
            });
            let export_result = write_task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match export_result {
                    Ok(Ok(written_path)) => {
                        workspace.update(cx, |ws, cx| {
                            ws.last_batch_banner = Some((
                                BannerKind::Success,
                                SharedString::from(format!(
                                    "{}: {}",
                                    I18n::t(locale, "library", "export"),
                                    written_path.display()
                                )),
                            ));
                            cx.notify();
                        });
                        let _ = window_handle.update(cx, |_, window, cx| {
                            window.close_dialog(cx);
                        });
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for BatchActionModalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let busy = self.busy.is_busy();
        let ent = cx.entity();
        let count = self.match_ids.len();

        let body: gpui_kit::AnyElement = match self.kind {
            BatchDialogKind::Copy | BatchDialogKind::Move => {
                let mut db_row = div().h_flex().flex_wrap().gap(px(6.0));
                for db in &self.target_databases {
                    let db_id = db.id;
                    let ent_pick = ent.clone();
                    db_row = db_row.child(
                        ActionButton::new(SharedString::from(format!(
                            "library-batch-target-{db_id}"
                        )))
                        .icon(if db.is_system {
                            ActionIconName::SystemDatabase
                        } else {
                            ActionIconName::Database
                        })
                        .label(database_display_name(db, locale))
                        .active(self.selected_target_db == Some(db_id))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_pick.update(cx, |view, cx| {
                                view.selected_target_db = Some(db_id);
                                cx.notify();
                            });
                        }),
                    );
                }
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "library", "target")),
                    )
                    .child(db_row)
                    .when(self.kind == BatchDialogKind::Copy, |el| {
                        el.child(
                            div()
                                .text_meta()
                                .text_color(p.muted)
                                .child(I18n::t(locale, "library", "copyHelp")),
                        )
                    })
                    .into_any_element()
            }
            BatchDialogKind::Tags => div()
                .v_flex()
                .gap(px(10.0))
                .child(
                    div()
                        .v_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_label()
                                .text_color(p.muted)
                                .child(I18n::t(locale, "library", "addTags")),
                        )
                        .child(Input::new(&self.add_tags_input).id("library-batch-add-tags")),
                )
                .child(
                    div()
                        .v_flex()
                        .gap(px(4.0))
                        .child(div().text_label().text_color(p.muted).child(I18n::t(
                            locale,
                            "library",
                            "removeTags",
                        )))
                        .child(Input::new(&self.remove_tags_input).id("library-batch-remove-tags")),
                )
                .into_any_element(),
            BatchDialogKind::Export => {
                let formats = [
                    (
                        ExportFormat::Original,
                        "original",
                        I18n::t(locale, "library", "original"),
                    ),
                    (ExportFormat::Sgf, "sgf", "SGF"),
                    (ExportFormat::Xg, "xg", "XG"),
                ];
                let mut fmt_row = div().h_flex().gap(px(6.0));
                for (fmt, slug, label) in formats {
                    let ent_fmt = ent.clone();
                    fmt_row = fmt_row.child(
                        ActionButton::new(SharedString::from(format!("library-export-fmt-{slug}")))
                            .label(label)
                            .active(self.export_format == fmt)
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_fmt.update(cx, |view, cx| {
                                    view.export_format = fmt;
                                    cx.notify();
                                });
                            }),
                    );
                }
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "library",
                        "exportFormat",
                    )))
                    .child(fmt_row)
                    .child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "library",
                        "exportHelp",
                    )))
                    .into_any_element()
            }
            BatchDialogKind::Delete => div()
                .text_body()
                .text_color(p.text)
                .child(I18n::format(
                    locale,
                    "library",
                    "deleteWarning",
                    &[("count", Param::from(count as i64))],
                ))
                .into_any_element(),
        };

        let confirm_label = match self.kind {
            BatchDialogKind::Copy => I18n::t(locale, "library", "copy"),
            BatchDialogKind::Move => I18n::t(locale, "library", "move"),
            BatchDialogKind::Tags => I18n::t(locale, "library", "save"),
            BatchDialogKind::Export => I18n::t(locale, "library", "export"),
            BatchDialogKind::Delete => I18n::t(locale, "library", "delete"),
        };

        let confirm_btn = ActionButton::new("library-batch-modal-confirm")
            .label(confirm_label)
            .variant(if self.kind == BatchDialogKind::Delete {
                ActionButtonVariant::DangerSolid
            } else {
                ActionButtonVariant::Primary
            })
            .disabled(busy)
            .palette(p)
            .on_click(move |_, window, cx| {
                ent.update(cx, |v, cx| v.execute(window, cx));
            });

        div()
            .id("library-batch-action-modal")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .when_some(self.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-batch-modal-error", BannerKind::Error, err)
                        .palette(p),
                )
            })
            .child(body)
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        ActionButton::new("library-batch-modal-cancel")
                            .label(I18n::t(locale, "library", "cancel"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(confirm_btn),
            )
    }
}

// ── 5. Bulk Match Analysis Modal ─────────────────────────────────────────────

struct BulkAnalysisModalView {
    workspace: Entity<LibraryWorkspace>,
    facade: BackendFacade,
    matches: Vec<DatabaseMatch>,
    unavailable_count: usize,
    engines: Vec<EngineInfo>,
    selected_engine: MatchAnalysisEngine,
    selected_plies: i32,
    accepted_count: usize,
    failures: Vec<(DatabaseMatch, String)>,
    busy: ModalBusyLock,
    loading_engines: bool,
    error: Option<SharedString>,
}

impl BulkAnalysisModalView {
    fn new(
        workspace: Entity<LibraryWorkspace>,
        facade: BackendFacade,
        matches: Vec<DatabaseMatch>,
        unavailable_count: usize,
        busy: ModalBusyLock,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            workspace,
            facade,
            matches,
            unavailable_count,
            engines: Vec::new(),
            selected_engine: MatchAnalysisEngine::Gnubg,
            selected_plies: 3,
            accepted_count: 0,
            failures: Vec::new(),
            busy,
            loading_engines: true,
            error: None,
        };
        view.load_engine_defaults(cx);
        view
    }

    fn load_engine_defaults(&mut self, cx: &mut Context<Self>) {
        let facade = self.facade.clone();
        let task = TokioBridge::spawn_read(cx, async move {
            run_facade_async(move || async move {
                let engines = facade.list_engines().await?;
                let config = facade.get_configuration().await.ok();
                Ok::<_, neondojo::domain_error::DomainError>((engines, config))
            })
            .await
        });

        let locale = current_locale(cx);
        cx.spawn(async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |view, cx| {
                view.loading_engines = false;
                match res {
                    Ok(Ok((engines, cfg))) => {
                        if let Some(cfg) = cfg {
                            let default_eng = match cfg.active_profile.configuration.analysis.engine
                            {
                                neondojo::configuration::ConfigurationEngine::OpenSage => {
                                    MatchAnalysisEngine::Bgsage
                                }
                                neondojo::configuration::ConfigurationEngine::Gnubg => {
                                    MatchAnalysisEngine::Gnubg
                                }
                            };
                            view.selected_engine = default_eng;
                            view.selected_plies = cfg.active_profile.configuration.analysis.plies;
                        }
                        view.engines = engines;
                        view.clamp_depth_for_engine();
                        cx.notify();
                    }
                    Ok(Err(err)) => {
                        view.error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(err) => {
                        view.error = Some(SharedString::from(err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn selected_engine_id(&self) -> &'static str {
        match self.selected_engine {
            MatchAnalysisEngine::Gnubg => "gnubg",
            MatchAnalysisEngine::Bgsage => "bgsage",
        }
    }

    fn available_depths(&self) -> Vec<i32> {
        let id = self.selected_engine_id();
        if let Some(info) = self.engines.iter().find(|e| e.id == id)
            && !info.plies.is_empty()
        {
            return info.plies.clone();
        }
        engine_depths(id, "analysis").to_vec()
    }

    fn selected_engine_available(&self) -> bool {
        let id = self.selected_engine_id();
        self.engines
            .iter()
            .find(|e| e.id == id)
            .is_none_or(|e| e.available)
    }

    fn clamp_depth_for_engine(&mut self) {
        let depths = self.available_depths();
        if !depths.contains(&self.selected_plies)
            && let Some(&fallback) = depths.first()
        {
            self.selected_plies = fallback;
        }
    }

    fn submit_targets(
        &mut self,
        targets: Vec<DatabaseMatch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_busy() || targets.is_empty() {
            return;
        }
        if !self.selected_engine_available() {
            self.error = Some("Selected analysis engine is unavailable.".into());
            cx.notify();
            return;
        }
        let facade = self.facade.clone();
        let engine = self.selected_engine;
        let plies = self.selected_plies;
        let workspace = self.workspace.clone();
        let locale = current_locale(cx);

        self.busy.set_busy(true);
        self.error = None;
        self.failures.clear();
        cx.notify();

        let task = TokioBridge::spawn_write(cx, async move {
            run_facade_async(move || async move {
                let mut jobs = Vec::new();
                let mut failed = Vec::new();
                for m in targets {
                    let req = MatchAnalysisRequest {
                        game_no: None,
                        engine_id: Some(engine),
                        plies: Some(plies),
                    };
                    match facade.create_match_analysis(m.id, req).await {
                        Ok(job) => jobs.push((job, m.database_id)),
                        Err(err) => failed.push((m, format_domain_error(locale, &err))),
                    }
                }
                (jobs, failed)
            })
            .await
        });

        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok((jobs, failed)) => {
                        if !jobs.is_empty() {
                            workspace.update(cx, |ws, cx| {
                                ws.track_bulk_analysis_jobs(&jobs, view.accepted_count > 0, cx);
                            });
                            view.accepted_count += jobs.len();
                        }
                        view.failures = failed;
                        if view.failures.is_empty() {
                            let _ = window_handle.update(cx, |_, window, cx| {
                                window.close_dialog(cx);
                            });
                        } else {
                            cx.notify();
                        }
                    }
                    Err(join_err) => {
                        view.error = Some(SharedString::from(join_err.to_string()));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for BulkAnalysisModalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let busy = self.busy.is_busy();
        let ent = cx.entity();
        let depths = self.available_depths();
        let engine_available = self.selected_engine_available();

        let engine_options = [
            (MatchAnalysisEngine::Gnubg, "gnubg", "GNUbg"),
            (MatchAnalysisEngine::Bgsage, "bgsage", "NeonGammon"),
        ];
        let mut engine_row = div().h_flex().gap(px(6.0));
        for (eng, id, fallback_name) in engine_options {
            let info = self.engines.iter().find(|e| e.id == id);
            let available = info.is_none_or(|e| e.available);
            let label = info
                .map(|e| {
                    if e.available {
                        e.name.clone()
                    } else {
                        format!(
                            "{} ({})",
                            e.name,
                            I18n::t(locale, "app", "versions.unavailable")
                        )
                    }
                })
                .unwrap_or_else(|| fallback_name.to_string());
            let ent_eng = ent.clone();
            engine_row = engine_row.child(
                ActionButton::new(SharedString::from(format!("library-bulk-engine-{id}")))
                    .label(label)
                    .active(self.selected_engine == eng)
                    .disabled(busy || !available)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_eng.update(cx, |v, cx| {
                            v.selected_engine = eng;
                            v.clamp_depth_for_engine();
                            cx.notify();
                        });
                    }),
            );
        }

        let mut depth_row = div().h_flex().gap(px(6.0));
        for depth in depths {
            let ent_d = ent.clone();
            depth_row = depth_row.child(
                ActionButton::new(SharedString::from(format!("library-bulk-depth-{depth}")))
                    .label(format!("{depth}-ply"))
                    .active(self.selected_plies == depth)
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_d.update(cx, |v, cx| {
                            v.selected_plies = depth;
                            cx.notify();
                        });
                    }),
            );
        }

        let matches_heading = I18n::format(
            locale,
            "library",
            "matchesToAnalyze",
            &[("count", Param::from(self.matches.len() as i64))],
        );

        let mut match_preview = div()
            .id("library-bulk-match-list")
            .v_flex()
            .gap(px(3.0))
            .max_h(px(140.0))
            .overflow_y_scroll();
        for m in &self.matches {
            match_preview = match_preview.child(
                div()
                    .text_meta()
                    .text_color(p.text)
                    .child(format!("#{} · {} — {}", m.match_no, m.white, m.black)),
            );
        }

        div()
            .id("library-bulk-analysis-modal")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .when_some(self.error.clone(), |el, err| {
                el.child(StatusBanner::new("library-bulk-error", BannerKind::Error, err).palette(p))
            })
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(div().text_label().text_color(p.muted).child("Engine"))
                    .child(engine_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "app",
                        "matchAnalysis.depth",
                    )))
                    .child(depth_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(matches_heading),
                    )
                    .child(match_preview)
                    .when(self.unavailable_count > 0, |el| {
                        el.child(div().text_meta().text_color(p.muted).child(I18n::format(
                            locale,
                            "library",
                            "analysisUnavailable",
                            &[("count", Param::from(self.unavailable_count as i64))],
                        )))
                    }),
            )
            .when(self.accepted_count > 0, |el| {
                el.child(
                    StatusBanner::new(
                        "library-bulk-accepted",
                        BannerKind::Success,
                        I18n::format(
                            locale,
                            "library",
                            "analysisAccepted",
                            &[("count", Param::from(self.accepted_count as i64))],
                        ),
                    )
                    .palette(p),
                )
            })
            .when(!self.failures.is_empty(), |el| {
                let mut fail_box = div()
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(p.danger_bg)
                    .border_1()
                    .border_color(p.danger_border);
                for (m, err) in &self.failures {
                    fail_box = fail_box.child(div().text_meta().text_color(p.danger_text).child(
                        format!("#{} ({} — {}): {}", m.match_no, m.white, m.black, err),
                    ));
                }
                el.child(fail_box)
            })
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        ActionButton::new("library-bulk-cancel")
                            .label(I18n::t(locale, "library", "close"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .when(!self.failures.is_empty(), |el| {
                        let ent_retry = ent.clone();
                        let failed_matches: Vec<DatabaseMatch> = self
                            .failures
                            .iter()
                            .map(|(m, _)| clone_database_match(m))
                            .collect();
                        el.child(
                            ActionButton::new("library-bulk-retry-failed")
                                .icon(ActionIconName::Refresh)
                                .label(I18n::t(locale, "library", "retryAnalysis"))
                                .primary()
                                .disabled(busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    let targets: Vec<_> =
                                        failed_matches.iter().map(clone_database_match).collect();
                                    ent_retry.update(cx, |v, cx| {
                                        v.submit_targets(targets, window, cx);
                                    });
                                }),
                        )
                    })
                    .when(self.failures.is_empty(), |el| {
                        let ent_start = ent.clone();
                        let all_matches: Vec<DatabaseMatch> =
                            self.matches.iter().map(clone_database_match).collect();
                        el.child(
                            ActionButton::new("library-bulk-start")
                                .icon(ActionIconName::Analysis)
                                .label(if busy {
                                    I18n::t(locale, "app", "matchAnalysis.submitting")
                                } else {
                                    I18n::t(locale, "library", "analyzeSelection")
                                })
                                .primary()
                                .disabled(
                                    busy || self.loading_engines
                                        || !engine_available
                                        || all_matches.is_empty(),
                                )
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    let targets: Vec<_> =
                                        all_matches.iter().map(clone_database_match).collect();
                                    ent_start.update(cx, |v, cx| {
                                        v.submit_targets(targets, window, cx);
                                    });
                                }),
                        )
                    }),
            )
    }
}

// ── Launchers on LibraryWorkspace ────────────────────────────────────────────

impl LibraryWorkspace {
    pub(super) fn open_database_form_modal(
        &mut self,
        editing: Option<Database>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let locale = current_locale(cx);
        let title = if editing.is_some() {
            I18n::t(locale, "library", "editDatabase")
        } else {
            I18n::t(locale, "library", "newDatabase")
        };
        let busy = ModalBusyLock::new(false);
        let initial_name = editing.as_ref().map(|d| d.name.clone()).unwrap_or_default();
        let initial_desc = editing
            .as_ref()
            .map(|d| d.description.clone())
            .unwrap_or_default();
        let workspace = cx.entity();

        let view = cx.new(|cx| {
            let name_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_name));
            let desc_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_desc));
            DatabaseFormView {
                workspace,
                facade,
                editing,
                name_input,
                desc_input,
                busy: busy.clone(),
                error: None,
            }
        });

        open_library_dialog(
            window,
            cx,
            title,
            busy,
            move |_, _| view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    pub(super) fn open_confirm_database_modal(
        &mut self,
        database: Database,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let locale = current_locale(cx);
        let title = if database.is_system {
            I18n::t(locale, "library", "clearDatabase")
        } else {
            I18n::t(locale, "library", "deleteDatabase")
        };
        let busy = ModalBusyLock::new(false);
        let workspace = cx.entity();

        let view = cx.new(|_| ConfirmDatabaseView {
            workspace,
            facade,
            database,
            busy: busy.clone(),
            error: None,
        });

        open_library_dialog(
            window,
            cx,
            title,
            busy,
            move |_, _| view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    pub(super) fn open_edit_match_modal(
        &mut self,
        target: DatabaseMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let workspace = cx.entity();

        let view = cx.new(|cx| {
            let display_name =
                cx.new(|cx| InputState::new(window, cx).default_value(target.display_name.clone()));
            let played_on = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("YYYY-MM-DD")
                    .default_value(
                        target
                            .played_on
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                    )
            });
            let tournament = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(target.tournament.clone().unwrap_or_default())
            });
            let round = cx.new(|cx| {
                InputState::new(window, cx).default_value(target.round.clone().unwrap_or_default())
            });
            let place = cx.new(|cx| {
                InputState::new(window, cx).default_value(target.place.clone().unwrap_or_default())
            });
            let white_score = cx.new(|cx| {
                InputState::new(window, cx).default_value(
                    target
                        .white_score
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                )
            });
            let black_score = cx.new(|cx| {
                InputState::new(window, cx).default_value(
                    target
                        .black_score
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                )
            });
            let result_state = target.result_state.clone();
            EditMatchMetadataView {
                workspace,
                facade,
                target,
                display_name,
                played_on,
                tournament,
                round,
                place,
                white_score,
                black_score,
                result_state,
                busy: busy.clone(),
                error: None,
            }
        });

        let config = ModalConfig::new(I18n::t(locale, "library", "editMatch"))
            .size(ModalSize::Wide)
            .busy_lock(busy);
        open_modal_dialog(
            window,
            cx,
            config,
            move |_, _| view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
            None,
        );
    }

    pub(super) fn open_batch_modal(
        &mut self,
        kind: BatchDialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        if self.selection.is_empty() {
            return;
        }
        let locale = current_locale(cx);
        let title = match kind {
            BatchDialogKind::Copy => I18n::t(locale, "library", "copy"),
            BatchDialogKind::Move => I18n::t(locale, "library", "move"),
            BatchDialogKind::Tags => I18n::t(locale, "library", "tags"),
            BatchDialogKind::Export => I18n::t(locale, "library", "export"),
            BatchDialogKind::Delete => I18n::t(locale, "library", "delete"),
        };

        let target_databases: Vec<Database> = self
            .databases
            .iter()
            .filter(|d| accepts_library_matches(d.id))
            .map(clone_database)
            .collect();
        let default_target = target_databases
            .iter()
            .find(|d| Some(d.id) != self.selected_database_id)
            .or_else(|| target_databases.first())
            .map(|d| d.id);

        let busy = ModalBusyLock::new(false);
        let workspace = cx.entity();
        let match_ids = self.selection.ids();
        let source_database_id = self.selected_database_id;

        let view = cx.new(|cx| {
            let add_tags_input = cx.new(|cx| InputState::new(window, cx));
            let remove_tags_input = cx.new(|cx| InputState::new(window, cx));
            BatchActionModalView {
                workspace,
                facade,
                kind,
                match_ids,
                source_database_id,
                target_databases,
                selected_target_db: default_target,
                add_tags_input,
                remove_tags_input,
                export_format: ExportFormat::Original,
                busy: busy.clone(),
                error: None,
            }
        });

        open_library_dialog(
            window,
            cx,
            title,
            busy,
            move |_, _| view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    pub(super) fn open_bulk_analysis_modal(
        &mut self,
        matches: Vec<DatabaseMatch>,
        unavailable_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let workspace = cx.entity();

        let view = cx.new(|cx| {
            BulkAnalysisModalView::new(
                workspace,
                facade,
                matches,
                unavailable_count,
                busy.clone(),
                cx,
            )
        });

        let config = ModalConfig::new(I18n::t(locale, "library", "analyzeSelection"))
            .size(ModalSize::Standard)
            .busy_lock(busy);
        open_modal_dialog(
            window,
            cx,
            config,
            move |_, _| view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
            None,
        );
    }
}
