//! Players workspace modals:
//! - Edit Player (`PlayerEditDialogView`): first/last name + aliases + unsaved-discard guard + 409 reload recovery
//! - Merge Players (`PlayerMergeDialogView`): seeded candidate search + target/source compare + 409 reload recovery
//! - Virtual Profile Name (`VirtualProfileNameDialogView`): create / rename + unsaved-discard guard + 409 recovery
//! - Virtual Profile Member (`VirtualProfileMemberDialogView`): search & add member + 409 reload recovery
//! - Virtual Profile Delete (`VirtualProfileDeleteDialogView`): confirm delete + 409 reload recovery

use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::workspaces::players::summary::merge_search_seed;
use crate::workspaces::players::{PlayersWorkspace, spawn_facade_read, spawn_facade_write};
use gpui_kit::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window,
    base::StyledExt,
    component::{
        WindowExt,
        input::{Input, InputEvent, InputState},
    },
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::domain_error::{DomainError, ErrorKind};
use neondojo::player_directory::{
    PlayerAliasWrite, PlayerDetails, PlayerMerge, PlayerNameWrite, PlayerQuery, PlayerSearchHit,
    PlayerSort, PlayerSortDirection, VirtualPlayerDetails,
};
use neondojo_i18n::{I18n, Param, format_int};
use uuid::Uuid;

const ALIAS_PLATFORMS: &[(&str, &str)] = &[
    ("local", "Local"),
    ("backgammongalaxy", "Backgammon Galaxy"),
    ("backgammonstudio", "Backgammon Studio"),
    ("xg", "eXtreme Gammon"),
    ("gnubg", "GNUbg"),
    ("dailygammon", "DailyGammon"),
    ("opengammon", "Opengammon"),
    ("gridgammon", "GridGammon"),
    ("fibs", "FIBS"),
];

pub fn is_revision_conflict(err: &DomainError) -> bool {
    matches!(err.kind, ErrorKind::Conflict | ErrorKind::NotFound)
}

// ---------------------------------------------------------------------------
// Edit Player Dialog
// ---------------------------------------------------------------------------

struct PlayerEditDialogView {
    details: PlayerDetails,
    first_input: Entity<InputState>,
    last_input: Entity<InputState>,
    alias_platform: String,
    alias_name_input: Entity<InputState>,
    confirm_discard: bool,
    conflict: Option<SharedString>,
    error: Option<SharedString>,
    saved_notice: bool,
    busy: ModalBusyLock,
    parent: Entity<PlayersWorkspace>,
    _subs: Vec<Subscription>,
    _task: Option<Task<()>>,
}

impl PlayerEditDialogView {
    fn new(
        details: PlayerDetails,
        busy: ModalBusyLock,
        parent: Entity<PlayersWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let first_val = details.profile.first_name.clone();
        let last_val = details.profile.last_name.clone();

        let first_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(first_val)
                .placeholder(I18n::t(locale, "player-directory", "first"))
        });
        let last_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(last_val)
                .placeholder(I18n::t(locale, "player-directory", "last"))
        });
        let alias_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "player-directory", "alias"))
        });

        let mut subs = Vec::new();
        for inp in [&first_input, &last_input, &alias_name_input] {
            subs.push(cx.subscribe(inp, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.confirm_discard = false;
                    this.saved_notice = false;
                    cx.notify();
                }
            }));
        }

        Self {
            details,
            first_input,
            last_input,
            alias_platform: "local".to_string(),
            alias_name_input,
            confirm_discard: false,
            conflict: None,
            error: None,
            saved_notice: false,
            busy,
            parent,
            _subs: subs,
            _task: None,
        }
    }

    fn is_dirty(&self, cx: &App) -> bool {
        let first = self.first_input.read(cx).value().trim().to_string();
        let last = self.last_input.read(cx).value().trim().to_string();
        let alias = self.alias_name_input.read(cx).value().trim().to_string();
        first != self.details.profile.first_name.trim()
            || last != self.details.profile.last_name.trim()
            || !alias.is_empty()
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        if self.is_dirty(cx) && !self.confirm_discard {
            self.confirm_discard = true;
            cx.notify();
            return;
        }
        window.close_dialog(cx);
    }

    fn save_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let id = self.details.profile.id;
        let version = self.details.profile.version;
        let first_name = self.first_input.read(cx).value().trim().to_string();
        let last_name = self.last_input.read(cx).value().trim().to_string();
        let locale = current_locale(cx);

        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .update_player(
                    id,
                    PlayerNameWrite {
                        first_name,
                        last_name,
                        version,
                    },
                )
                .await
        }) else {
            return;
        };

        self.busy.set_busy(true);
        self.error = None;
        self.conflict = None;
        self.confirm_discard = false;
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(updated)) => {
                        let first_v = updated.profile.first_name.clone();
                        let last_v = updated.profile.last_name.clone();
                        dlg.details = updated.clone();
                        dlg.first_input
                            .update(cx, |inp, cx| inp.set_value(first_v, window, cx));
                        dlg.last_input
                            .update(cx, |inp, cx| inp.set_value(last_v, window, cx));
                        dlg.saved_notice = true;
                        parent.update(cx, |ws, cx| ws.on_player_updated(updated, cx));
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn add_alias(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let name = self.alias_name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let id = self.details.profile.id;
        let version = self.details.profile.version;
        let platform = self.alias_platform.clone();
        let locale = current_locale(cx);

        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .add_player_alias(
                    id,
                    PlayerAliasWrite {
                        platform,
                        name,
                        version,
                    },
                )
                .await
        }) else {
            return;
        };

        self.busy.set_busy(true);
        self.error = None;
        self.conflict = None;
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(updated)) => {
                        dlg.details = updated.clone();
                        dlg.alias_name_input
                            .update(cx, |inp, cx| inp.set_value("", window, cx));
                        dlg.saved_notice = true;
                        parent.update(cx, |ws, cx| ws.on_player_updated(updated, cx));
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn reload_after_conflict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.details.profile.id;
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.get_player_summary(id).await
        }) else {
            return;
        };

        self.busy.set_busy(true);
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(fresh) => {
                        let first_v = fresh.profile.first_name.clone();
                        let last_v = fresh.profile.last_name.clone();
                        dlg.details = fresh.clone();
                        dlg.first_input
                            .update(cx, |inp, cx| inp.set_value(first_v, window, cx));
                        dlg.last_input
                            .update(cx, |inp, cx| inp.set_value(last_v, window, cx));
                        dlg.conflict = None;
                        dlg.error = None;
                        dlg.confirm_discard = false;
                        parent.update(cx, |ws, cx| ws.on_player_updated(fresh, cx));
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }
}

impl Render for PlayerEditDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();
        let is_busy = self.busy.is_busy();
        let alias_can_add = !is_busy && !self.alias_name_input.read(cx).value().trim().is_empty();

        let mut platform_bar = div().h_flex().gap(px(4.0)).flex_wrap();
        for &(id, label) in ALIAS_PLATFORMS {
            let active = self.alias_platform == id;
            let ent = entity.clone();
            let id_owned = id.to_string();
            platform_bar = platform_bar.child(
                ActionButton::new(SharedString::from(format!("player-alias-plat-{id}")))
                    .label(label)
                    .active(active)
                    .disabled(is_busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        let next = id_owned.clone();
                        ent.update(cx, |dlg, cx| {
                            dlg.alias_platform = next;
                            cx.notify();
                        });
                    }),
            );
        }

        let mut alias_chips = div().h_flex().gap(px(6.0)).flex_wrap();
        for (idx, alias) in self.details.aliases.iter().enumerate() {
            alias_chips = alias_chips.child(
                div()
                    .id(SharedString::from(format!("edit-alias-chip-{idx}")))
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface_alt)
                    .text_meta()
                    .child(div().text_color(p.text).child(alias.name.clone()))
                    .child(
                        div()
                            .text_color(p.muted)
                            .child(format!("({})", alias.platform)),
                    ),
            );
        }

        let add_ent = entity.clone();
        let reload_ent = entity.clone();
        let keep_ent = entity.clone();

        div()
            .v_flex()
            .gap(px(12.0))
            .child(
                div()
                    .h_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "player-directory",
                                "first",
                            )))
                            .child(Input::new(&self.first_input)),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "player-directory",
                                "last",
                            )))
                            .child(Input::new(&self.last_input)),
                    ),
            )
            // Aliases section
            .child(
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(I18n::t(locale, "player-directory", "aliases")),
                    )
                    .child(alias_chips)
                    .child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "player-directory",
                        "source",
                    )))
                    .child(platform_bar)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().flex_1().child(Input::new(&self.alias_name_input)))
                            .child(
                                ActionButton::new("player-edit-add-alias")
                                    .icon(ActionIconName::Add)
                                    .label(I18n::t(locale, "player-directory", "add"))
                                    .disabled(!alias_can_add)
                                    .palette(p)
                                    .on_click(move |_, window, cx| {
                                        add_ent.update(cx, |dlg, cx| dlg.add_alias(window, cx));
                                    }),
                            ),
                    ),
            )
            .when(self.saved_notice, |this| {
                this.child(
                    StatusBanner::new(
                        "player-edit-saved",
                        BannerKind::Success,
                        I18n::t(locale, "player-directory", "saved"),
                    )
                    .palette(p),
                )
            })
            .when_some(self.conflict.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("player-edit-conflict", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, window, cx| {
                                reload_ent
                                    .update(cx, |dlg, cx| dlg.reload_after_conflict(window, cx));
                            },
                        ),
                )
            })
            .when_some(self.error.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("player-edit-error", BannerKind::Error, msg).palette(p),
                )
            })
            .when(self.confirm_discard, |this| {
                this.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(p.warning_text)
                        .bg(p.warning_bg)
                        .child(div().text_label().text_color(p.warning_text).child(I18n::t(
                            locale,
                            "player-directory",
                            "discardQuestion",
                        )))
                        .child(
                            div()
                                .h_flex()
                                .gap(px(6.0))
                                .child(
                                    ActionButton::new("player-edit-keep")
                                        .label(I18n::t(locale, "player-directory", "keepEditing"))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            keep_ent.update(cx, |dlg, cx| {
                                                dlg.confirm_discard = false;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    ActionButton::new("player-edit-discard")
                                        .label(I18n::t(locale, "player-directory", "discard"))
                                        .danger()
                                        .palette(p)
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                ),
                        ),
                )
            })
    }
}

pub fn open_edit_player_dialog(
    details: PlayerDetails,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let busy = ModalBusyLock::new(false);
    let title = format!(
        "{}: {}",
        I18n::t(locale, "player-directory", "edit"),
        details.profile.display_name
    );
    let dlg = cx.new(|cx| PlayerEditDialogView::new(details, busy.clone(), parent, window, cx));
    let body_dlg = dlg.clone();
    let footer_dlg = dlg;

    open_modal_dialog(
        window,
        cx,
        ModalConfig::new(title)
            .size(ModalSize::Standard)
            .overlay_closable(false)
            .keyboard_closable(false)
            .show_close_button(false)
            .busy_lock(busy),
        move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let loc = current_locale(cx);
            let is_busy = footer_dlg.read(cx).busy.is_busy();
            let close_dlg = footer_dlg.clone();
            let save_dlg = footer_dlg.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("player-edit-close")
                        .icon(ActionIconName::Close)
                        .label(I18n::t(loc, "library", "cancel"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            close_dlg.update(cx, |d, cx| d.request_close(window, cx));
                        }),
                )
                .child(
                    ActionButton::new("player-edit-save")
                        .icon(ActionIconName::Save)
                        .label(I18n::t(loc, "player-directory", "save"))
                        .primary()
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            save_dlg.update(cx, |d, cx| d.save_name(window, cx));
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

// ---------------------------------------------------------------------------
// Merge Players Dialog
// ---------------------------------------------------------------------------

struct PlayerMergeDialogView {
    target: PlayerDetails,
    search_input: Entity<InputState>,
    candidates: Vec<PlayerSearchHit>,
    selected_source: Option<PlayerSearchHit>,
    loading: bool,
    conflict: Option<SharedString>,
    error: Option<SharedString>,
    busy: ModalBusyLock,
    parent: Entity<PlayersWorkspace>,
    _sub: Subscription,
    _search_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl PlayerMergeDialogView {
    fn new(
        target: PlayerDetails,
        busy: ModalBusyLock,
        parent: Entity<PlayersWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let seed = merge_search_seed(&target.profile.display_name);
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(seed)
                .placeholder(I18n::t(locale, "player-directory", "searchHint"))
        });
        let sub = cx.subscribe(&search_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_candidates(cx);
            }
        });
        let mut dlg = Self {
            target,
            search_input,
            candidates: Vec::new(),
            selected_source: None,
            loading: false,
            conflict: None,
            error: None,
            busy,
            parent,
            _sub: sub,
            _search_task: None,
            _action_task: None,
        };
        dlg.search_candidates(cx);
        dlg
    }

    fn search_candidates(&mut self, cx: &mut Context<Self>) {
        let q_raw = self.search_input.read(cx).value().trim().to_string();
        let query = PlayerQuery {
            q: (!q_raw.is_empty()).then_some(q_raw),
            after: None,
            sort: PlayerSort::Matches,
            direction: Some(PlayerSortDirection::Desc),
            exclude_id: Some(self.target.profile.id),
            page: Some(1),
        };
        let locale = current_locale(cx);
        let Some(read_task) =
            spawn_facade_read(
                cx,
                move |facade| async move { facade.list_players(query).await },
            )
        else {
            return;
        };
        self.loading = true;
        cx.notify();

        self._search_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                dlg.loading = false;
                match res {
                    Ok(page) => {
                        dlg.candidates = page.items;
                        if let Some(sel) = &dlg.selected_source
                            && let Some(refreshed) = dlg
                                .candidates
                                .iter()
                                .find(|c| c.profile.id == sel.profile.id)
                        {
                            dlg.selected_source = Some(refreshed.clone());
                        }
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn can_merge(&self) -> bool {
        !self.busy.is_busy() && self.selected_source.is_some()
    }

    fn execute_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.selected_source.clone() else {
            return;
        };
        if self.busy.is_busy() {
            return;
        }
        let target_id = self.target.profile.id;
        let input = PlayerMerge {
            source_id: source.profile.id,
            source_version: source.profile.version,
            version: self.target.profile.version,
        };
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.merge_players(target_id, input).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        self.conflict = None;
        self.error = None;
        cx.notify();

        let parent = self.parent.clone();
        self._action_task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(merged)) => {
                        parent.update(cx, |ws, cx| ws.on_player_merged(merged, cx));
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                        cx.notify();
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            });
        }));
    }

    fn reload_after_conflict(&mut self, cx: &mut Context<Self>) {
        let id = self.target.profile.id;
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.get_player_summary(id).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(fresh) => {
                        dlg.target = fresh;
                        dlg.conflict = None;
                        dlg.error = None;
                        dlg.search_candidates(cx);
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }
}

impl Render for PlayerMergeDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();
        let reload_ent = entity.clone();

        let mut list = div()
            .id("merge-candidates-list")
            .v_flex()
            .gap(px(4.0))
            .max_h(px(220.0))
            .overflow_y_scroll()
            .p(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface_alt);

        if self.loading && self.candidates.is_empty() {
            list = list.child(
                div()
                    .p(px(8.0))
                    .text_meta()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "loading")),
            );
        } else if self.candidates.is_empty() {
            list = list.child(
                div()
                    .p(px(8.0))
                    .text_meta()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "empty")),
            );
        } else {
            for hit in &self.candidates {
                let selected = self
                    .selected_source
                    .as_ref()
                    .is_some_and(|s| s.profile.id == hit.profile.id);
                let hit_clone = hit.clone();
                let ent = entity.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("merge-cand-{}", hit.profile.id)))
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .bg(if selected { p.selected } else { p.surface })
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .on_click(move |_, _, cx| {
                            let chosen = hit_clone.clone();
                            ent.update(cx, |dlg, cx| {
                                dlg.selected_source = Some(chosen);
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text)
                                .child(hit.profile.display_name.clone()),
                        )
                        .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                            format!(
                                "{} · {}",
                                format_int(locale, hit.profile.match_count),
                                I18n::t(locale, "player-directory", "matches")
                            ),
                        )),
                );
            }
        }

        div()
            .v_flex()
            .gap(px(12.0))
            .child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "player-directory",
                "mergeHint",
            )))
            .child(
                div()
                    .h_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .p(px(10.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(p.accent)
                            .bg(p.selected)
                            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                locale,
                                "player-directory",
                                "mergeTarget",
                            )))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(self.target.profile.display_name.clone()),
                            )
                            .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                                format!(
                                    "{} {}",
                                    format_int(locale, self.target.profile.match_count),
                                    I18n::t(locale, "player-directory", "matches")
                                ),
                            )),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .p(px(10.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.surface_alt)
                            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                locale,
                                "player-directory",
                                "mergeSource",
                            )))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(
                                        self.selected_source
                                            .as_ref()
                                            .map(|s| s.profile.display_name.clone())
                                            .unwrap_or_else(|| {
                                                I18n::t(locale, "player-directory", "choose")
                                                    .to_string()
                                            }),
                                    ),
                            )
                            .when_some(self.selected_source.as_ref(), |this, s| {
                                this.child(
                                    div().text_meta().tabular_nums().text_color(p.muted).child(
                                        format!(
                                            "{} {}",
                                            format_int(locale, s.profile.match_count),
                                            I18n::t(locale, "player-directory", "matches")
                                        ),
                                    ),
                                )
                            }),
                    ),
            )
            .child(Input::new(&self.search_input))
            .child(list)
            .when_some(self.conflict.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("player-merge-conflict", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, _, cx| {
                                reload_ent.update(cx, |dlg, cx| dlg.reload_after_conflict(cx));
                            },
                        ),
                )
            })
            .when_some(self.error.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("player-merge-error", BannerKind::Error, msg).palette(p),
                )
            })
    }
}

pub fn open_merge_player_dialog(
    target: PlayerDetails,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let busy = ModalBusyLock::new(false);
    let title = format!(
        "{}: {}",
        I18n::t(locale, "player-directory", "mergeWith"),
        target.profile.display_name
    );
    let dlg = cx.new(|cx| PlayerMergeDialogView::new(target, busy.clone(), parent, window, cx));
    let body_dlg = dlg.clone();
    let footer_dlg = dlg;

    open_modal_dialog(
        window,
        cx,
        ModalConfig::new(title)
            .size(ModalSize::Wide)
            .busy_lock(busy),
        move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let loc = current_locale(cx);
            let state = footer_dlg.read(cx);
            let can_merge = state.can_merge();
            let is_busy = state.busy.is_busy();
            let exec_dlg = footer_dlg.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("player-merge-cancel")
                        .icon(ActionIconName::Cancel)
                        .label(I18n::t(loc, "library", "cancel"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("player-merge-confirm")
                        .icon(ActionIconName::Merge)
                        .label(I18n::t(loc, "player-directory", "merge"))
                        .danger_solid()
                        .disabled(!can_merge)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            exec_dlg.update(cx, |d, cx| d.execute_merge(window, cx));
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

// ---------------------------------------------------------------------------
// Virtual Profile Create / Rename Dialog
// ---------------------------------------------------------------------------

enum VirtualNameMode {
    Create,
    Rename {
        id: Uuid,
        version: i64,
        original: String,
    },
}

struct VirtualProfileNameDialogView {
    mode: VirtualNameMode,
    name_input: Entity<InputState>,
    confirm_discard: bool,
    conflict: Option<SharedString>,
    error: Option<SharedString>,
    busy: ModalBusyLock,
    parent: Entity<PlayersWorkspace>,
    _sub: Subscription,
    _task: Option<Task<()>>,
}

impl VirtualProfileNameDialogView {
    fn new(
        mode: VirtualNameMode,
        busy: ModalBusyLock,
        parent: Entity<PlayersWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let initial = match &mode {
            VirtualNameMode::Create => String::new(),
            VirtualNameMode::Rename { original, .. } => original.clone(),
        };
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial)
                .placeholder(I18n::t(locale, "virtual-player", "name"))
        });
        let sub = cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.confirm_discard = false;
                cx.notify();
            }
        });
        Self {
            mode,
            name_input,
            confirm_discard: false,
            conflict: None,
            error: None,
            busy,
            parent,
            _sub: sub,
            _task: None,
        }
    }

    fn is_dirty(&self, cx: &App) -> bool {
        let val = self.name_input.read(cx).value().trim().to_string();
        match &self.mode {
            VirtualNameMode::Create => !val.is_empty(),
            VirtualNameMode::Rename { original, .. } => val != original.trim(),
        }
    }

    fn can_submit(&self, cx: &App) -> bool {
        !self.busy.is_busy() && !self.name_input.read(cx).value().trim().is_empty()
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        if self.is_dirty(cx) && !self.confirm_discard {
            self.confirm_discard = true;
            cx.notify();
            return;
        }
        window.close_dialog(cx);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let name = self.name_input.read(cx).value().trim().to_string();
        let locale = current_locale(cx);
        let rename_target = match &self.mode {
            VirtualNameMode::Create => None,
            VirtualNameMode::Rename { id, version, .. } => Some((*id, *version)),
        };

        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            match rename_target {
                None => facade.create_virtual_profile(name).await,
                Some((id, version)) => facade.update_virtual_profile(id, name, version).await,
            }
        }) else {
            return;
        };

        self.busy.set_busy(true);
        self.error = None;
        self.conflict = None;
        self.confirm_discard = false;
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(details)) => {
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_saved(details, cx));
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if matches!(dlg.mode, VirtualNameMode::Rename { .. })
                            && is_revision_conflict(&err)
                        {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                        cx.notify();
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            });
        }));
    }

    fn reload_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let VirtualNameMode::Rename { id, .. } = self.mode else {
            return;
        };
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.get_virtual_profile(id).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(fresh) => {
                        let fresh_name = fresh.profile.name.clone();
                        dlg.mode = VirtualNameMode::Rename {
                            id: fresh.profile.id,
                            version: fresh.profile.version,
                            original: fresh_name.clone(),
                        };
                        dlg.name_input
                            .update(cx, |inp, cx| inp.set_value(fresh_name, window, cx));
                        dlg.conflict = None;
                        dlg.error = None;
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_saved(fresh, cx));
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }
}

impl Render for VirtualProfileNameDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();
        let reload_ent = entity.clone();
        let keep_ent = entity.clone();

        div()
            .v_flex()
            .gap(px(10.0))
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "virtual-player",
                        "name",
                    )))
                    .child(Input::new(&self.name_input)),
            )
            .when_some(self.conflict.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("vp-name-conflict", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, window, cx| {
                                reload_ent.update(cx, |d, cx| d.reload_rename(window, cx));
                            },
                        ),
                )
            })
            .when_some(self.error.clone(), |this, msg| {
                this.child(StatusBanner::new("vp-name-error", BannerKind::Error, msg).palette(p))
            })
            .when(self.confirm_discard, |this| {
                this.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(p.warning_text)
                        .bg(p.warning_bg)
                        .child(div().text_label().text_color(p.warning_text).child(I18n::t(
                            locale,
                            "player-directory",
                            "discardQuestion",
                        )))
                        .child(
                            div()
                                .h_flex()
                                .gap(px(6.0))
                                .child(
                                    ActionButton::new("vp-name-keep")
                                        .label(I18n::t(locale, "player-directory", "keepEditing"))
                                        .palette(p)
                                        .on_click(move |_, _, cx| {
                                            keep_ent.update(cx, |d, cx| {
                                                d.confirm_discard = false;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    ActionButton::new("vp-name-discard")
                                        .label(I18n::t(locale, "player-directory", "discard"))
                                        .danger()
                                        .palette(p)
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                ),
                        ),
                )
            })
    }
}

pub fn open_create_virtual_profile_dialog(
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    open_virtual_name_dialog(VirtualNameMode::Create, parent, window, cx);
}

pub fn open_rename_virtual_profile_dialog(
    details: &VirtualPlayerDetails,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    open_virtual_name_dialog(
        VirtualNameMode::Rename {
            id: details.profile.id,
            version: details.profile.version,
            original: details.profile.name.clone(),
        },
        parent,
        window,
        cx,
    );
}

fn open_virtual_name_dialog(
    mode: VirtualNameMode,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let is_create = matches!(mode, VirtualNameMode::Create);
    let title = I18n::t(
        locale,
        "virtual-player",
        if is_create {
            "createTitle"
        } else {
            "renameTitle"
        },
    );
    let busy = ModalBusyLock::new(false);
    let dlg =
        cx.new(|cx| VirtualProfileNameDialogView::new(mode, busy.clone(), parent, window, cx));
    let body_dlg = dlg.clone();
    let footer_dlg = dlg;

    open_modal_dialog(
        window,
        cx,
        ModalConfig::new(title)
            .size(ModalSize::Compact)
            .overlay_closable(false)
            .keyboard_closable(false)
            .show_close_button(false)
            .busy_lock(busy),
        move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let loc = current_locale(cx);
            let can_submit = footer_dlg.read(cx).can_submit(cx);
            let is_busy = footer_dlg.read(cx).busy.is_busy();
            let close_dlg = footer_dlg.clone();
            let save_dlg = footer_dlg.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("vp-name-cancel")
                        .icon(ActionIconName::Cancel)
                        .label(I18n::t(loc, "library", "cancel"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            close_dlg.update(cx, |d, cx| d.request_close(window, cx));
                        }),
                )
                .child(
                    ActionButton::new("vp-name-submit")
                        .icon(if is_create {
                            ActionIconName::Add
                        } else {
                            ActionIconName::Save
                        })
                        .label(I18n::t(
                            loc,
                            "virtual-player",
                            if is_create { "createAction" } else { "save" },
                        ))
                        .primary()
                        .disabled(!can_submit)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            save_dlg.update(cx, |d, cx| d.submit(window, cx));
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

// ---------------------------------------------------------------------------
// Virtual Profile Add Member Dialog
// ---------------------------------------------------------------------------

struct VirtualProfileMemberDialogView {
    details: VirtualPlayerDetails,
    search_input: Entity<InputState>,
    candidates: Vec<PlayerSearchHit>,
    loading: bool,
    conflict: Option<SharedString>,
    error: Option<SharedString>,
    busy: ModalBusyLock,
    parent: Entity<PlayersWorkspace>,
    _sub: Subscription,
    _search_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl VirtualProfileMemberDialogView {
    fn new(
        details: VirtualPlayerDetails,
        busy: ModalBusyLock,
        parent: Entity<PlayersWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(
                locale,
                "virtual-player",
                "memberSearch",
            ))
        });
        let sub = cx.subscribe(&search_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_candidates(cx);
            }
        });
        let mut dlg = Self {
            details,
            search_input,
            candidates: Vec::new(),
            loading: false,
            conflict: None,
            error: None,
            busy,
            parent,
            _sub: sub,
            _search_task: None,
            _action_task: None,
        };
        dlg.search_candidates(cx);
        dlg
    }

    fn search_candidates(&mut self, cx: &mut Context<Self>) {
        let q_raw = self.search_input.read(cx).value().trim().to_string();
        let query = PlayerQuery {
            q: (!q_raw.is_empty()).then_some(q_raw),
            after: None,
            sort: PlayerSort::Matches,
            direction: Some(PlayerSortDirection::Desc),
            exclude_id: None,
            page: Some(1),
        };
        let locale = current_locale(cx);
        let Some(read_task) =
            spawn_facade_read(
                cx,
                move |facade| async move { facade.list_players(query).await },
            )
        else {
            return;
        };
        self.loading = true;
        cx.notify();

        self._search_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                dlg.loading = false;
                match res {
                    Ok(page) => dlg.candidates = page.items,
                    Err(err) => dlg.error = Some(format_domain_error(locale, &err).into()),
                }
                cx.notify();
            });
        }));
    }

    fn add_member(&mut self, profile_id: Uuid, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let vp_id = self.details.profile.id;
        let version = self.details.profile.version;
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.add_virtual_member(vp_id, profile_id, version).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        self.error = None;
        self.conflict = None;
        cx.notify();

        let parent = self.parent.clone();
        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |dlg, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(updated)) => {
                        dlg.details = updated.clone();
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_saved(updated, cx));
                        dlg.search_candidates(cx);
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn reload_after_conflict(&mut self, cx: &mut Context<Self>) {
        let vp_id = self.details.profile.id;
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.get_virtual_profile(vp_id).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        cx.notify();

        let parent = self.parent.clone();
        self._action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(fresh) => {
                        dlg.details = fresh.clone();
                        dlg.conflict = None;
                        dlg.error = None;
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_saved(fresh, cx));
                        dlg.search_candidates(cx);
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }
}

impl Render for VirtualProfileMemberDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();
        let reload_ent = entity.clone();
        let vp_id = self.details.profile.id;
        let is_busy = self.busy.is_busy();

        let mut list = div()
            .id("vp-add-member-list")
            .v_flex()
            .gap(px(4.0))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .p(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface_alt);

        if self.loading && self.candidates.is_empty() {
            list = list.child(
                div()
                    .p(px(8.0))
                    .text_meta()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "loading")),
            );
        } else if self.candidates.is_empty() {
            list = list.child(
                div()
                    .p(px(8.0))
                    .text_meta()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "empty")),
            );
        } else {
            for hit in &self.candidates {
                let pid = hit.profile.id;
                let already_here = self.details.members.iter().any(|m| m.id == pid)
                    || hit
                        .profile
                        .virtual_profile
                        .as_ref()
                        .is_some_and(|v| v.id == vp_id);
                let other_group = hit
                    .profile
                    .virtual_profile
                    .as_ref()
                    .filter(|v| v.id != vp_id);
                let ent = entity.clone();

                let right_slot = if already_here {
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(I18n::t(locale, "virtual-player", "alreadyMember"))
                        .into_any_element()
                } else if let Some(other) = other_group {
                    div()
                        .text_meta()
                        .text_color(p.warning_text)
                        .child(I18n::format(
                            locale,
                            "virtual-player",
                            "otherGroup",
                            &[("name", Param::from(other.name.as_str()))],
                        ))
                        .into_any_element()
                } else {
                    ActionButton::new(SharedString::from(format!("vp-member-add-{pid}")))
                        .icon(ActionIconName::Add)
                        .label(I18n::t(locale, "virtual-player", "add"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent.update(cx, |dlg, cx| dlg.add_member(pid, cx));
                        })
                        .into_any_element()
                };

                list = list.child(
                    div()
                        .id(SharedString::from(format!("vp-cand-{pid}")))
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.surface)
                        .child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_label()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(p.text)
                                        .child(hit.profile.display_name.clone()),
                                )
                                .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                                    format!(
                                        "{} {}",
                                        format_int(locale, hit.profile.match_count),
                                        I18n::t(locale, "player-directory", "matches")
                                    ),
                                )),
                        )
                        .child(right_slot),
                );
            }
        }

        div()
            .v_flex()
            .gap(px(10.0))
            .child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "virtual-player",
                "addMemberHint",
            )))
            .child(Input::new(&self.search_input))
            .child(list)
            .when_some(self.conflict.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("vp-member-conflict", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, _, cx| {
                                reload_ent.update(cx, |d, cx| d.reload_after_conflict(cx));
                            },
                        ),
                )
            })
            .when_some(self.error.clone(), |this, msg| {
                this.child(StatusBanner::new("vp-member-error", BannerKind::Error, msg).palette(p))
            })
    }
}

pub fn open_add_virtual_member_dialog(
    details: VirtualPlayerDetails,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let busy = ModalBusyLock::new(false);
    let title = format!(
        "{}: {}",
        I18n::t(locale, "virtual-player", "addMemberTitle"),
        details.profile.name
    );
    let dlg =
        cx.new(|cx| VirtualProfileMemberDialogView::new(details, busy.clone(), parent, window, cx));
    let body_dlg = dlg.clone();
    let footer_dlg = dlg;

    open_modal_dialog(
        window,
        cx,
        ModalConfig::new(title)
            .size(ModalSize::Standard)
            .busy_lock(busy),
        move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let loc = current_locale(cx);
            let is_busy = footer_dlg.read(cx).busy.is_busy();
            ActionButton::new("vp-member-done")
                .label(I18n::t(loc, "settings", "done"))
                .disabled(is_busy)
                .palette(p)
                .on_click(|_, window, cx| window.close_dialog(cx))
                .into_any_element()
        }),
        None,
    );
}

// ---------------------------------------------------------------------------
// Virtual Profile Delete Confirmation Dialog
// ---------------------------------------------------------------------------

struct VirtualProfileDeleteDialogView {
    id: Uuid,
    name: String,
    version: i64,
    conflict: Option<SharedString>,
    error: Option<SharedString>,
    busy: ModalBusyLock,
    parent: Entity<PlayersWorkspace>,
    _task: Option<Task<()>>,
}

impl VirtualProfileDeleteDialogView {
    fn new(
        details: &VirtualPlayerDetails,
        busy: ModalBusyLock,
        parent: Entity<PlayersWorkspace>,
    ) -> Self {
        Self {
            id: details.profile.id,
            name: details.profile.name.clone(),
            version: details.profile.version,
            conflict: None,
            error: None,
            busy,
            parent,
            _task: None,
        }
    }

    fn execute_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let id = self.id;
        let version = self.version;
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.delete_virtual_profile(id, version).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        self.error = None;
        self.conflict = None;
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(())) => {
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_deleted(id, cx));
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            dlg.conflict = Some(msg);
                        } else {
                            dlg.error = Some(msg);
                        }
                        cx.notify();
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            });
        }));
    }

    fn reload_after_conflict(&mut self, cx: &mut Context<Self>) {
        let id = self.id;
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.get_virtual_profile(id).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        cx.notify();

        let parent = self.parent.clone();
        self._task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |dlg, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(fresh) => {
                        dlg.version = fresh.profile.version;
                        dlg.name = fresh.profile.name.clone();
                        dlg.conflict = None;
                        dlg.error = None;
                        parent.update(cx, |ws, cx| ws.on_virtual_profile_saved(fresh, cx));
                    }
                    Err(err) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }
}

impl Render for VirtualProfileDeleteDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let reload_ent = cx.entity().clone();

        div()
            .v_flex()
            .gap(px(10.0))
            .child(
                div()
                    .text_body()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(self.name.clone()),
            )
            .child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "virtual-player",
                "deleteHint",
            )))
            .when_some(self.conflict.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("vp-delete-conflict", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, _, cx| {
                                reload_ent.update(cx, |d, cx| d.reload_after_conflict(cx));
                            },
                        ),
                )
            })
            .when_some(self.error.clone(), |this, msg| {
                this.child(StatusBanner::new("vp-delete-error", BannerKind::Error, msg).palette(p))
            })
    }
}

pub fn open_delete_virtual_profile_dialog(
    details: &VirtualPlayerDetails,
    parent: Entity<PlayersWorkspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let busy = ModalBusyLock::new(false);
    let dlg = cx.new(|_| VirtualProfileDeleteDialogView::new(details, busy.clone(), parent));
    let body_dlg = dlg.clone();
    let footer_dlg = dlg;

    open_modal_dialog(
        window,
        cx,
        ModalConfig::new(I18n::t(locale, "virtual-player", "deleteTitle"))
            .size(ModalSize::Compact)
            .busy_lock(busy),
        move |_: &mut Window, _: &mut App| body_dlg.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let loc = current_locale(cx);
            let is_busy = footer_dlg.read(cx).busy.is_busy();
            let exec_dlg = footer_dlg.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("vp-delete-cancel")
                        .icon(ActionIconName::Cancel)
                        .label(I18n::t(loc, "library", "cancel"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("vp-delete-confirm")
                        .icon(ActionIconName::Delete)
                        .label(I18n::t(loc, "virtual-player", "delete"))
                        .danger_solid()
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            exec_dlg.update(cx, |d, cx| d.execute_delete(window, cx));
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_conflict_recognizes_409_and_404_only() {
        assert!(is_revision_conflict(&DomainError::conflict("stale")));
        assert!(is_revision_conflict(&DomainError::missing()));
        assert!(!is_revision_conflict(&DomainError::invalid("bad input")));
    }
}
