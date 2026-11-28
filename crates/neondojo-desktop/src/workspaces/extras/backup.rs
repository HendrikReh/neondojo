//! Extras → Database Backup (`DatabaseBackupWorkspace.tsx`).
//!
//! Features:
//! - Native `cx.prompt_for_new_path` → `facade.export_backup_to_path`
//! - Native `cx.prompt_for_paths` → `facade.check_backup_path`
//! - `+`/`-`/`0` target-counts diff modal comparing backup vs `facade.backup_target_counts`
//! - `facade.restore_backup_path` handling 409 busy & 503 drain cleanly
//! - Emits [`DatabaseBackupEvent::Restored`] on successful restore so
//!   `ExtrasWorkspace` emits `WorkspaceEvent::TriggerAppReset { reason: "backup_restore" }`

use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::workspaces::extras::{spawn_facade_read, spawn_facade_write};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, Styled, Task, TestSupportExt, Window,
    base::StyledExt, component::WindowExt, div, prelude::FluentBuilder, px,
};
use neondojo::database_backup::{
    BackupCheck, BackupCompatibility, BackupContents, BackupTargetCounts,
};
use neondojo::domain_error::{DomainError, ErrorKind};
use neondojo_i18n::{I18n, Locale, format_datetime_medium_local, format_int, format_signed_diff};
use std::path::PathBuf;

/// Events emitted by [`DatabaseBackupWorkspace`] to [`super::ExtrasWorkspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackupEvent {
    Restored,
}

/// Direction of a backup-vs-target row count difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountDiffDirection {
    Increase,
    Decrease,
    Unchanged,
}

/// Single comparison row between backup archive and live target database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCountDiffRow {
    pub label_key: &'static str,
    pub backup: u64,
    pub target: i64,
    pub diff: i64,
    pub direction: CountDiffDirection,
}

/// Computes the 3-row `+` / `-` / `0` diff table (`matches`, `savedPositions`,
/// `learningCards`) comparing backup contents against current live counts.
pub fn compute_backup_diff(
    contents: &BackupContents,
    target: &BackupTargetCounts,
) -> [BackupCountDiffRow; 3] {
    let row = |label_key: &'static str, backup: u64, target_val: i64| {
        let diff = (backup as i64).saturating_sub(target_val);
        let direction = if diff > 0 {
            CountDiffDirection::Increase
        } else if diff < 0 {
            CountDiffDirection::Decrease
        } else {
            CountDiffDirection::Unchanged
        };
        BackupCountDiffRow {
            label_key,
            backup,
            target: target_val,
            diff,
            direction,
        }
    };
    [
        row("matches", contents.matches, target.matches),
        row(
            "savedPositions",
            contents.saved_positions,
            target.saved_positions,
        ),
        row(
            "learningCards",
            contents.learning_cards,
            target.learning_cards,
        ),
    ]
}

/// Parses UTC timestamp from a standard export filename
/// `neondojo-YYYY-MM-DD-HHMMSS.neondojo-backup`, rejecting impossible dates.
pub fn parse_backup_filename_timestamp(filename: &str) -> Option<DateTime<Utc>> {
    let rest = filename
        .strip_prefix("neondojo-")?
        .strip_suffix(".neondojo-backup")?;
    let (date_part, time_part) = rest.rsplit_once('-')?;
    if date_part.len() != 10 || time_part.len() != 6 {
        return None;
    }
    let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()?;
    let h: u32 = time_part[0..2].parse().ok()?;
    let m: u32 = time_part[2..4].parse().ok()?;
    let s: u32 = time_part[4..6].parse().ok()?;
    let time = NaiveTime::from_hms_opt(h, m, s)?;
    Some(DateTime::from_naive_utc_and_offset(
        NaiveDateTime::new(date, time),
        Utc,
    ))
}

/// Semantic category for restore errors so 409 busy and 503 drain surface clear guidance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreFailureKind {
    /// HTTP 409: another backup/restore or long-running background task (benchmark,
    /// engine duel, AI assessment, Anki sync) is active.
    BusyConflict(String),
    /// HTTP 503: restore drain timed out or another restore holds exclusive database lock.
    DrainUnavailable(String),
    /// Any other validation/schema/IO error.
    Other(String),
}

pub fn classify_restore_error(locale: Locale, err: &DomainError) -> RestoreFailureKind {
    let formatted = format_domain_error(locale, err);
    match err.kind {
        ErrorKind::Conflict => RestoreFailureKind::BusyConflict(formatted),
        ErrorKind::ServiceUnavailable => RestoreFailureKind::DrainUnavailable(formatted),
        _ => RestoreFailureKind::Other(formatted),
    }
}

#[derive(Debug, Clone)]
enum ExportPhase {
    Idle,
    Pending,
    Done(PathBuf),
    Error(SharedString),
}

#[derive(Debug, Clone)]
enum CheckPhase {
    Pending,
    Compatible(BackupContents),
    Incompatible(Option<BackupContents>),
    Error(SharedString),
}

#[derive(Debug, Clone)]
enum TargetCountsState {
    Loading,
    Ready(BackupTargetCounts),
    Error(SharedString),
}

/// Modal dialog entity reviewing backup contents vs live target database counts
/// and executing the destructive replace restore.
struct RestoreReviewDialog {
    path: PathBuf,
    filename: String,
    contents: BackupContents,
    target_counts: TargetCountsState,
    restoring: bool,
    restored: bool,
    restore_error: Option<RestoreFailureKind>,
    busy: ModalBusyLock,
    parent: Entity<DatabaseBackupWorkspace>,
    _counts_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl RestoreReviewDialog {
    fn new(
        path: PathBuf,
        contents: BackupContents,
        busy: ModalBusyLock,
        parent: Entity<DatabaseBackupWorkspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("backup.neondojo-backup")
            .to_string();
        let mut dlg = Self {
            path,
            filename,
            contents,
            target_counts: TargetCountsState::Loading,
            restoring: false,
            restored: false,
            restore_error: None,
            busy,
            parent,
            _counts_task: None,
            _action_task: None,
        };
        dlg.load_target_counts(cx);
        dlg
    }

    fn load_target_counts(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.backup_target_counts().await
        }) else {
            return;
        };
        self.target_counts = TargetCountsState::Loading;
        cx.notify();

        self._counts_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                match res {
                    Ok(counts) => dlg.target_counts = TargetCountsState::Ready(counts),
                    Err(err) => {
                        dlg.target_counts =
                            TargetCountsState::Error(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn can_replace(&self) -> bool {
        !self.restoring
            && !self.restored
            && matches!(self.target_counts, TargetCountsState::Ready(_))
    }

    fn execute_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_replace() {
            return;
        }
        let path = self.path.clone();
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.restore_backup_path(&path, true).await
        }) else {
            return;
        };
        self.restoring = true;
        self.restore_error = None;
        self.busy.set_busy(true);
        cx.notify();

        let parent = self.parent.clone();
        self._action_task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.restoring = false;
                match res {
                    Ok(Ok(())) => {
                        dlg.restored = true;
                        dlg.busy.set_busy(false);
                        parent.update(cx, |ws, cx| ws.notify_restored(cx));
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        dlg.busy.set_busy(false);
                        dlg.restore_error = Some(classify_restore_error(locale, &err));
                        cx.notify();
                    }
                    Err(err) => {
                        dlg.busy.set_busy(false);
                        dlg.restore_error = Some(RestoreFailureKind::Other(err.to_string()));
                        cx.notify();
                    }
                }
            });
        }));
    }
}

impl Render for RestoreReviewDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();

        let created_text = format_backup_created_at(locale, &self.contents, &self.filename);

        let diff_table = match &self.target_counts {
            TargetCountsState::Loading => div()
                .text_body()
                .text_color(p.muted)
                .child(I18n::t(locale, "database-backup", "targetLoading"))
                .into_any_element(),
            TargetCountsState::Error(msg) => {
                let ent = entity.clone();
                StatusBanner::new(
                    "backup-target-error",
                    BannerKind::Error,
                    format!(
                        "{} {msg}",
                        I18n::t(locale, "database-backup", "targetError")
                    ),
                )
                .palette(p)
                .on_retry(
                    I18n::t(locale, "database-backup", "targetRetry"),
                    move |_, _, cx| {
                        ent.update(cx, |dlg, cx| dlg.load_target_counts(cx));
                    },
                )
                .into_any_element()
            }
            TargetCountsState::Ready(target) => {
                let rows = compute_backup_diff(&self.contents, target);
                let mut table = div()
                    .v_flex()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface_alt)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .border_b_1()
                            .border_color(p.border)
                            .text_table_heading()
                            .text_color(p.muted)
                            .child(div().flex_1())
                            .child(div().w(px(90.0)).child(I18n::t(
                                locale,
                                "database-backup",
                                "backupColumn",
                            )))
                            .child(div().w(px(120.0)).child(I18n::t(
                                locale,
                                "database-backup",
                                "targetColumn",
                            )))
                            .child(div().w(px(90.0)).child(I18n::t(
                                locale,
                                "database-backup",
                                "differenceColumn",
                            ))),
                    );
                for row in rows {
                    let diff_color = match row.direction {
                        CountDiffDirection::Increase => p.success_text,
                        CountDiffDirection::Decrease => p.danger_text,
                        CountDiffDirection::Unchanged => p.muted,
                    };
                    table = table.child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .border_b_1()
                            .border_color(p.border)
                            .text_table()
                            .text_color(p.text)
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(I18n::t(locale, "database-backup", row.label_key)),
                            )
                            .child(
                                div()
                                    .w(px(90.0))
                                    .child(format_int(locale, row.backup as i64)),
                            )
                            .child(div().w(px(120.0)).child(format_int(locale, row.target)))
                            .child(
                                div()
                                    .w(px(90.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(diff_color)
                                    .child(format_signed_diff(locale, row.diff)),
                            ),
                    );
                }
                table.into_any_element()
            }
        };

        div()
            .v_flex()
            .gap(px(10.0))
            .when(self.restoring, |this| {
                this.child(
                    StatusBanner::new(
                        "backup-restoring-status",
                        BannerKind::Info,
                        I18n::t(locale, "database-backup", "restoring"),
                    )
                    .palette(p),
                )
            })
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .text_body()
                    .child(div().text_color(p.muted).child(I18n::t(
                        locale,
                        "database-backup",
                        "file",
                    )))
                    .child(
                        div()
                            .mono_font()
                            .text_color(p.text)
                            .child(self.filename.clone()),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .text_body()
                    .child(div().text_color(p.muted).child(I18n::t(
                        locale,
                        "database-backup",
                        "createdAt",
                    )))
                    .child(div().text_color(p.text).child(created_text)),
            )
            .child(diff_table)
            .child(
                StatusBanner::new(
                    "backup-confirm-warning",
                    BannerKind::Warning,
                    I18n::t(locale, "database-backup", "warning"),
                )
                .palette(p),
            )
            .child(div().text_meta().text_color(p.muted).child(format!(
                "{} {}",
                I18n::t(locale, "database-backup", "compatible"),
                I18n::t(locale, "database-backup", "validation")
            )))
            .when_some(self.restore_error.clone(), |this, err| {
                let (kind, msg) = match err {
                    RestoreFailureKind::BusyConflict(m) => (BannerKind::Warning, m),
                    RestoreFailureKind::DrainUnavailable(m) => (BannerKind::Warning, m),
                    RestoreFailureKind::Other(m) => (BannerKind::Error, m),
                };
                this.child(StatusBanner::new("backup-restore-error", kind, msg).palette(p))
            })
    }
}

fn format_backup_created_at(locale: Locale, contents: &BackupContents, filename: &str) -> String {
    if let Some(dt) = contents.created_at {
        return format_datetime_medium_local(locale, dt);
    }
    if let Some(dt) = parse_backup_filename_timestamp(filename) {
        return format!(
            "{} ({})",
            format_datetime_medium_local(locale, dt),
            I18n::t(locale, "database-backup", "dateFromFilename")
        );
    }
    I18n::t(locale, "database-backup", "missingDate").to_string()
}

/// Complete GPUI Database Backup sub-workspace.
pub struct DatabaseBackupWorkspace {
    visible: bool,
    export_phase: ExportPhase,
    selected_file: Option<PathBuf>,
    check_phase: Option<CheckPhase>,
    restored_notice: bool,
    _task: Option<Task<()>>,
}

impl DatabaseBackupWorkspace {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            visible: false,
            export_phase: ExportPhase::Idle,
            selected_file: None,
            check_phase: None,
            restored_notice: false,
            _task: None,
        }
    }

    pub fn set_visible(&mut self, visible: bool, _cx: &mut Context<Self>) {
        self.visible = visible;
    }

    pub(crate) fn notify_restored(&mut self, cx: &mut Context<Self>) {
        self.restored_notice = true;
        self.selected_file = None;
        self.check_phase = None;
        cx.emit(DatabaseBackupEvent::Restored);
        cx.notify();
    }

    fn export_backup(&mut self, cx: &mut Context<Self>) {
        if matches!(self.export_phase, ExportPhase::Pending) {
            return;
        }
        let locale = current_locale(cx);
        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!(
            "neondojo-{}.neondojo-backup",
            Utc::now().format("%Y-%m-%d-%H%M%S")
        );
        let prompt = cx.prompt_for_new_path(&dir, Some(&suggested));

        self._task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest_path))) = prompt.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.export_phase = ExportPhase::Pending;
                cx.notify();
            });
            let dest_for_task = dest_path.clone();
            let write_task = cx.update(|cx| {
                spawn_facade_write(cx, move |facade| async move {
                    facade.export_backup_to_path(&dest_for_task).await
                })
            });
            let Some(write_task) = write_task else {
                return;
            };
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(Ok(saved_path)) => ws.export_phase = ExportPhase::Done(saved_path),
                    Ok(Err(err)) => {
                        ws.export_phase =
                            ExportPhase::Error(format_domain_error(locale, &err).into());
                    }
                    Err(err) => {
                        ws.export_phase = ExportPhase::Error(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn choose_and_check_backup(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(I18n::t(locale, "database-backup", "chooseFile").into()),
        });

        self._task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.check_path(path, cx);
            });
        }));
    }

    fn check_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        self.selected_file = Some(path.clone());
        self.check_phase = Some(CheckPhase::Pending);
        self.restored_notice = false;
        cx.notify();

        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.check_backup_path(&path).await
        }) else {
            return;
        };

        self._task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(BackupCheck {
                        status: BackupCompatibility::Compatible,
                        contents: Some(contents),
                    }) => {
                        ws.check_phase = Some(CheckPhase::Compatible(contents));
                    }
                    Ok(BackupCheck {
                        status: BackupCompatibility::IncompatibleSchema,
                        contents,
                    }) => {
                        ws.check_phase = Some(CheckPhase::Incompatible(contents));
                    }
                    Ok(_) => {
                        ws.check_phase = Some(CheckPhase::Error(
                            I18n::t(locale, "app", "error.invalidResponse").into(),
                        ));
                    }
                    Err(err) => {
                        ws.check_phase =
                            Some(CheckPhase::Error(format_domain_error(locale, &err).into()));
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_review_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(path), Some(CheckPhase::Compatible(contents))) =
            (self.selected_file.clone(), self.check_phase.clone())
        else {
            return;
        };
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity().clone();
        let dlg = cx.new(|cx| RestoreReviewDialog::new(path, contents, busy.clone(), parent, cx));
        let body_dlg = dlg.clone();
        let footer_dlg = dlg;

        open_modal_dialog(
            window,
            cx,
            ModalConfig::new(I18n::t(locale, "database-backup", "importTitle"))
                .size(ModalSize::Wide)
                .busy_lock(busy),
            move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
            Some(move |_: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let loc = current_locale(cx);
                let state = footer_dlg.read(cx);
                let can_replace = state.can_replace();
                let restoring = state.restoring;
                let replace_dlg = footer_dlg.clone();
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("backup-modal-cancel")
                            .icon(ActionIconName::Cancel)
                            .label(I18n::t(loc, "database-backup", "cancel"))
                            .disabled(restoring)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("backup-modal-replace")
                            .icon(ActionIconName::Import)
                            .label(I18n::t(loc, "database-backup", "replace"))
                            .danger_solid()
                            .disabled(!can_replace)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                replace_dlg.update(cx, |d, cx| d.execute_restore(window, cx));
                            }),
                    )
                    .into_any_element()
            }),
            None,
        );
    }
}

impl EventEmitter<DatabaseBackupEvent> for DatabaseBackupWorkspace {}

impl Render for DatabaseBackupWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();

        let exporting = matches!(self.export_phase, ExportPhase::Pending);
        let checking = matches!(self.check_phase, Some(CheckPhase::Pending));
        let compatible = matches!(self.check_phase, Some(CheckPhase::Compatible(_)));
        let filename = self
            .selected_file
            .as_ref()
            .and_then(|f| f.file_name().and_then(|n| n.to_str()))
            .map(str::to_string);
        let contents = match &self.check_phase {
            Some(CheckPhase::Compatible(c)) => Some(c),
            Some(CheckPhase::Incompatible(c)) => c.as_ref(),
            _ => None,
        };

        let export_ent = entity.clone();
        let choose_ent = entity.clone();
        let retry_ent = entity.clone();
        let review_ent = entity.clone();

        div()
            .id("database-backup-workspace")
            .test_support()
            .v_flex()
            .gap(px(16.0))
            .max_w(px(820.0))
            .when(self.restored_notice, |this| {
                this.child(
                    StatusBanner::new(
                        "backup-restored-banner",
                        BannerKind::Success,
                        I18n::t(locale, "database-backup", "done"),
                    )
                    .palette(p),
                )
            })
            // Export panel
            .child(
                div()
                    .v_flex()
                    .gap(px(10.0))
                    .p(px(16.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .v_flex()
                                    .gap(px(4.0))
                                    .flex_1()
                                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                                        locale,
                                        "database-backup",
                                        "exportTitle",
                                    )))
                                    .child(div().text_body().text_color(p.muted).child(I18n::t(
                                        locale,
                                        "database-backup",
                                        "exportHelp",
                                    ))),
                            )
                            .child(
                                ActionButton::new("backup-export-btn")
                                    .icon(ActionIconName::Export)
                                    .label(I18n::t(locale, "database-backup", "exportTitle"))
                                    .disabled(exporting || checking)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        export_ent.update(cx, |ws, cx| ws.export_backup(cx));
                                    }),
                            ),
                    )
                    .child(match &self.export_phase {
                        ExportPhase::Idle => div().into_any_element(),
                        ExportPhase::Pending => div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "database-backup", "exporting"))
                            .into_any_element(),
                        ExportPhase::Done(path) => StatusBanner::new(
                            "backup-export-done",
                            BannerKind::Success,
                            format!(
                                "{} ({})",
                                I18n::t(locale, "database-backup", "exported"),
                                path.display()
                            ),
                        )
                        .palette(p)
                        .into_any_element(),
                        ExportPhase::Error(msg) => {
                            StatusBanner::new("backup-export-error", BannerKind::Error, msg.clone())
                                .palette(p)
                                .into_any_element()
                        }
                    }),
            )
            // Restore panel
            .child(
                div()
                    .v_flex()
                    .gap(px(10.0))
                    .p(px(16.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "database-backup",
                        "importTitle",
                    )))
                    .child(div().text_body().text_color(p.muted).child(I18n::t(
                        locale,
                        "database-backup",
                        "importHelp",
                    )))
                    .child(
                        StatusBanner::new(
                            "backup-warning",
                            BannerKind::Warning,
                            I18n::t(locale, "database-backup", "warning"),
                        )
                        .palette(p),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                ActionButton::new("backup-choose-file-btn")
                                    .icon(ActionIconName::Open)
                                    .label(I18n::t(locale, "database-backup", "chooseFile"))
                                    .disabled(exporting || checking)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        choose_ent
                                            .update(cx, |ws, cx| ws.choose_and_check_backup(cx));
                                    }),
                            )
                            .child(
                                div()
                                    .text_body()
                                    .text_color(if filename.is_some() { p.text } else { p.muted })
                                    .child(filename.clone().unwrap_or_else(|| {
                                        I18n::t(locale, "database-backup", "noFile").to_string()
                                    })),
                            ),
                    )
                    .when(checking, |this| {
                        this.child(div().text_meta().text_color(p.muted).child(I18n::t(
                            locale,
                            "database-backup",
                            "checking",
                        )))
                    })
                    .when(compatible, |this| {
                        this.child(
                            StatusBanner::new(
                                "backup-compatible",
                                BannerKind::Success,
                                I18n::t(locale, "database-backup", "compatible"),
                            )
                            .palette(p),
                        )
                    })
                    .when(
                        matches!(self.check_phase, Some(CheckPhase::Incompatible(_))),
                        |this| {
                            this.child(
                                StatusBanner::new(
                                    "backup-incompatible",
                                    BannerKind::Error,
                                    I18n::t(locale, "database-backup", "incompatible"),
                                )
                                .palette(p),
                            )
                        },
                    )
                    .when_some(contents, |this, c| {
                        let fname = filename.as_deref().unwrap_or("");
                        this.child(
                            div()
                                .h_flex()
                                .gap(px(16.0))
                                .flex_wrap()
                                .p(px(10.0))
                                .rounded(px(6.0))
                                .bg(p.surface_alt)
                                .text_table()
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "database-backup", "createdAt"),
                                    format_backup_created_at(locale, c, fname)
                                ))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "database-backup", "matches"),
                                    format_int(locale, c.matches as i64)
                                ))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "database-backup", "savedPositions"),
                                    format_int(locale, c.saved_positions as i64)
                                ))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "database-backup", "learningCards"),
                                    format_int(locale, c.learning_cards as i64)
                                )),
                        )
                    })
                    .when_some(
                        match &self.check_phase {
                            Some(CheckPhase::Error(msg)) => Some(msg.clone()),
                            _ => None,
                        },
                        |this, msg| {
                            let selected = self.selected_file.clone();
                            this.child(
                                StatusBanner::new(
                                    "backup-check-error",
                                    BannerKind::Error,
                                    format!(
                                        "{} {msg}",
                                        I18n::t(locale, "database-backup", "checkFailure")
                                    ),
                                )
                                .palette(p)
                                .on_retry(
                                    I18n::t(locale, "database-backup", "checkRetry"),
                                    move |_, _, cx| {
                                        if let Some(p) = selected.clone() {
                                            retry_ent.update(cx, |ws, cx| ws.check_path(p, cx));
                                        }
                                    },
                                ),
                            )
                        },
                    )
                    .child(
                        div().pt(px(4.0)).child(
                            ActionButton::new("backup-review-btn")
                                .icon(ActionIconName::Import)
                                .label(I18n::t(locale, "database-backup", "review"))
                                .primary()
                                .disabled(!compatible || exporting)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    review_ent
                                        .update(cx, |ws, cx| ws.open_review_dialog(window, cx));
                                }),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_backup_diff_produces_positive_negative_and_zero_directions() {
        let contents = BackupContents {
            created_at: None,
            matches: 150,
            saved_positions: 20,
            learning_cards: 40,
        };
        let target = BackupTargetCounts {
            matches: 100,
            saved_positions: 35,
            learning_cards: 40,
        };
        let rows = compute_backup_diff(&contents, &target);
        assert_eq!(rows[0].diff, 50);
        assert_eq!(rows[0].direction, CountDiffDirection::Increase);
        assert_eq!(format_signed_diff(Locale::En, rows[0].diff), "+50");

        assert_eq!(rows[1].diff, -15);
        assert_eq!(rows[1].direction, CountDiffDirection::Decrease);
        assert_eq!(format_signed_diff(Locale::En, rows[1].diff), "-15");

        assert_eq!(rows[2].diff, 0);
        assert_eq!(rows[2].direction, CountDiffDirection::Unchanged);
        assert_eq!(format_signed_diff(Locale::En, rows[2].diff), "0");
    }

    #[test]
    fn parses_valid_backup_filename_timestamps_and_rejects_impossible_dates() {
        let valid = parse_backup_filename_timestamp("neondojo-2026-11-22-140509.neondojo-backup");
        assert!(valid.is_some());
        assert_eq!(valid.unwrap().to_rfc3339(), "2026-11-22T14:05:09+00:00");
        assert!(
            parse_backup_filename_timestamp("neondojo-2026-02-30-120000.neondojo-backup").is_none()
        );
        assert!(
            parse_backup_filename_timestamp("neondojo-2026-11-22-240000.neondojo-backup").is_none()
        );
        assert!(parse_backup_filename_timestamp("other.neondojo-backup").is_none());
    }

    #[test]
    fn classifies_409_busy_and_503_drain_restore_errors_distinctly() {
        let busy = DomainError::conflict("Database backup or restore is already in progress");
        assert!(matches!(
            classify_restore_error(Locale::En, &busy),
            RestoreFailureKind::BusyConflict(_)
        ));
        let drain = DomainError::restore_in_progress();
        assert!(matches!(
            classify_restore_error(Locale::En, &drain),
            RestoreFailureKind::DrainUnavailable(_)
        ));
    }
}
