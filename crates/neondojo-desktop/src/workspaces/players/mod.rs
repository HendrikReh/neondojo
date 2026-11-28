//! Players workspace: Master-detail Player Directory & Virtual Profiles.
//!
//! Features:
//! - `VirtualPlayerStrip` at top opening `is_me` ("My player profile") first by default
//! - Search input + sortable player results (`Name`, `Matches`, `Games`) with numbered pagination
//! - Shared detail view for both real players and virtual profiles:
//!   - Overall / Checker / Cube PR breakdown + `Analyze unrated matches…` (`StartBulkAnalysis`)
//!   - Custom GPUI `canvas()` `PlayerRatingTrendView` chart per profile (`>= 2` rated matches)
//!   - Opponent Breakdown table (clicking an opponent switches selection to that opponent)
//!   - Database-filtered paginated Match List (`OpenMatchInAnalysis` + clickable opponent)
//! - Edit Player, Merge Players, and Virtual Profile CRUD dialogs with unsaved-discard
//!   guards and 409 revision-conflict reload recovery.

pub mod dialogs;
pub mod rating_trend;
pub mod summary;

use crate::app_context::DesktopAppContext;
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::pagination::NumberedSearchPagination;
use dialogs::{
    is_revision_conflict, open_add_virtual_member_dialog, open_create_virtual_profile_dialog,
    open_delete_virtual_profile_dialog, open_edit_player_dialog, open_merge_player_dialog,
    open_rename_virtual_profile_dialog,
};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, TestSupportExt, Window,
    base::StyledExt,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::databases::{Database, DatabaseMatch, LibraryQuery};
use neondojo::facade::BackendFacade;
use neondojo::player_directory::{
    PlayerDetails, PlayerQuery, PlayerSearchHit, PlayerSort, PlayerSortDirection,
    VirtualPlayerDetails, VirtualPlayerProfile,
};
use neondojo_i18n::{I18n, Locale, Param, format_int, format_naive_date_medium, format_pr};
use rating_trend::PlayerRatingTrendView;
use std::future::Future;
use summary::{
    PlayerMatchOutcome, PlayerSelection, choose_initial_selection, compute_opponent_breakdown,
    compute_pr_split_summary, filter_unrated_analyzable_match_ids, profile_depth,
    rating_histories_by_profile, resolve_match_perspective,
};
use uuid::Uuid;

const PLAYERS_PAGE_SIZE: i64 = 50;
const MATCHES_PAGE_SIZE: i64 = 25;
const MAX_BULK_SCAN_MATCHES: usize = 2_000;

/// Runs an abortable read against [`BackendFacade`] on Tokio's blocking pool
/// (cross-crate SQLx HRTB `Send` workaround).
pub(crate) fn spawn_facade_read<F, Fut, R>(cx: &App, op: F) -> Option<ReadTask<R>>
where
    F: FnOnce(BackendFacade) -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let facade = cx.try_global::<DesktopAppContext>()?.facade()?;
    if !cx.has_global::<TokioBridge>() {
        return None;
    }
    Some(TokioBridge::spawn_read(cx, async move {
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || handle.block_on(op(facade)))
            .await
            .expect("players facade read panicked")
    }))
}

/// Runs a non-abortable write mutation against [`BackendFacade`] on Tokio's blocking pool.
pub(crate) fn spawn_facade_write<F, Fut, R>(
    cx: &App,
    op: F,
) -> Option<Task<Result<R, tokio::task::JoinError>>>
where
    F: FnOnce(BackendFacade) -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let facade = cx.try_global::<DesktopAppContext>()?.facade()?;
    if !cx.has_global::<TokioBridge>() {
        return None;
    }
    Some(TokioBridge::spawn_write(cx, async move {
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || handle.block_on(op(facade)))
            .await
            .expect("players facade write panicked")
    }))
}

#[derive(Clone)]
enum SelectedDetails {
    Real(PlayerDetails),
    Virtual(VirtualPlayerDetails),
}

/// Master-detail Players workspace (`PlayersWorkspace.tsx`).
pub struct PlayersWorkspace {
    visible: bool,
    initialized: bool,
    // Virtual profile strip
    virtuals: Vec<VirtualPlayerProfile>,
    virtuals_loading: bool,
    // Left player directory search + list
    search_input: Entity<InputState>,
    sort: PlayerSort,
    direction: PlayerSortDirection,
    players_page: usize,
    players_total: i64,
    players: Vec<PlayerSearchHit>,
    players_loading: bool,
    // Selected detail state
    selection: Option<PlayerSelection>,
    details: Option<SelectedDetails>,
    details_loading: bool,
    trend_views: Vec<Entity<PlayerRatingTrendView>>,
    // Matches & Opponent Breakdown
    databases: Vec<Database>,
    selected_database_id: Option<Uuid>,
    matches_page: usize,
    matches_total: i64,
    matches: Vec<DatabaseMatch>,
    matches_loading: bool,
    // Bulk analyze & member remove actions
    analyzing_unrated: bool,
    removing_member: Option<Uuid>,
    status_notice: Option<SharedString>,
    error_message: Option<SharedString>,
    conflict_message: Option<SharedString>,
    // Background task handles
    _subs: Vec<Subscription>,
    _virtuals_task: Option<Task<()>>,
    _players_task: Option<Task<()>>,
    _details_task: Option<Task<()>>,
    _matches_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl PlayersWorkspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = current_locale(cx);
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(
                locale,
                "player-directory",
                "searchHint",
            ))
        });

        let sub = cx.subscribe(&search_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.players_page = 1;
                this.load_players(cx);
            }
        });

        let mut ws = Self {
            visible: true,
            initialized: false,
            virtuals: Vec::new(),
            virtuals_loading: false,
            search_input,
            sort: PlayerSort::Matches,
            direction: PlayerSortDirection::Desc,
            players_page: 1,
            players_total: 0,
            players: Vec::new(),
            players_loading: false,
            selection: None,
            details: None,
            details_loading: false,
            trend_views: Vec::new(),
            databases: Vec::new(),
            selected_database_id: None,
            matches_page: 1,
            matches_total: 0,
            matches: Vec::new(),
            matches_loading: false,
            analyzing_unrated: false,
            removing_member: None,
            status_notice: None,
            error_message: None,
            conflict_message: None,
            _subs: vec![sub],
            _virtuals_task: None,
            _players_task: None,
            _details_task: None,
            _matches_task: None,
            _action_task: None,
        };
        ws.reload_directory(cx);
        ws
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.reload_directory(cx);
        }
        cx.notify();
    }

    pub fn current_selection(&self) -> Option<PlayerSelection> {
        self.selection
    }

    fn reload_directory(&mut self, cx: &mut Context<Self>) {
        self.load_virtuals_and_databases(cx);
        self.load_players(cx);
    }

    fn load_virtuals_and_databases(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let vps = facade.list_virtual_profiles().await;
            let dbs = facade.list_databases().await;
            (vps, dbs)
        }) else {
            return;
        };
        self.virtuals_loading = true;
        cx.notify();

        self._virtuals_task = Some(cx.spawn(async move |this, cx| {
            let Ok((vps_res, dbs_res)) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.virtuals_loading = false;
                if let Ok(dbs) = dbs_res {
                    ws.databases = dbs;
                }
                match vps_res {
                    Ok(list) => {
                        ws.virtuals = list.items;
                        if !ws.initialized {
                            ws.initialized = true;
                            if ws.selection.is_none()
                                && let Some(initial) = choose_initial_selection(&ws.virtuals)
                            {
                                ws.select_entry(initial, cx);
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn load_players(&mut self, cx: &mut Context<Self>) {
        let q_raw = self.search_input.read(cx).value().trim().to_string();
        let query = PlayerQuery {
            q: (!q_raw.is_empty()).then_some(q_raw),
            after: None,
            sort: self.sort,
            direction: Some(self.direction),
            exclude_id: None,
            page: Some(self.players_page as i64),
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
        self.players_loading = true;
        cx.notify();

        self._players_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.players_loading = false;
                match res {
                    Ok(page) => {
                        ws.players_total = page.total.unwrap_or(page.items.len() as i64);
                        ws.players = page.items;
                        // If no virtual profile was selected by default, pick the first real player.
                        if ws.selection.is_none()
                            && !ws.virtuals_loading
                            && let Some(first) = ws.players.first()
                        {
                            ws.select_entry(PlayerSelection::Real(first.profile.id), cx);
                            return;
                        }
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn select_entry(&mut self, selection: PlayerSelection, cx: &mut Context<Self>) {
        self.selection = Some(selection);
        self.selected_database_id = None;
        self.matches_page = 1;
        self.status_notice = None;
        self.conflict_message = None;
        self.load_selected_details(cx);
        self.load_selected_matches(cx);
        cx.notify();
    }

    fn select_opponent(
        &mut self,
        opponent_id: Option<Uuid>,
        opponent_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = opponent_id {
            self.select_entry(PlayerSelection::Real(id), cx);
            return;
        }
        // Fallback when an uncatalogued opponent row is clicked: filter search input by name.
        self.search_input
            .update(cx, |inp, cx| inp.set_value(opponent_name, window, cx));
        self.players_page = 1;
        self.load_players(cx);
    }

    fn toggle_sort(&mut self, sort: PlayerSort, cx: &mut Context<Self>) {
        if self.sort == sort {
            self.direction = match self.direction {
                PlayerSortDirection::Asc => PlayerSortDirection::Desc,
                PlayerSortDirection::Desc => PlayerSortDirection::Asc,
            };
        } else {
            self.sort = sort;
            self.direction = match sort {
                PlayerSort::Name => PlayerSortDirection::Asc,
                PlayerSort::Matches | PlayerSort::Games => PlayerSortDirection::Desc,
            };
        }
        self.players_page = 1;
        self.load_players(cx);
        cx.notify();
    }

    fn load_selected_details(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            self.details = None;
            self.trend_views.clear();
            return;
        };
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            match selection {
                PlayerSelection::Real(id) => facade
                    .get_player_summary(id)
                    .await
                    .map(SelectedDetails::Real),
                PlayerSelection::Virtual(id) => facade
                    .get_virtual_profile(id)
                    .await
                    .map(SelectedDetails::Virtual),
            }
        }) else {
            return;
        };
        self.details_loading = true;
        self.error_message = None;
        cx.notify();

        self._details_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.details_loading = false;
                match res {
                    Ok(det) => {
                        ws.rebuild_trend_views(&det, cx);
                        ws.details = Some(det);
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn rebuild_trend_views(&mut self, details: &SelectedDetails, cx: &mut Context<Self>) {
        let history = match details {
            SelectedDetails::Real(d) => &d.rating_history,
            SelectedDetails::Virtual(v) => &v.rating_history,
        };
        let grouped = rating_histories_by_profile(history);
        self.trend_views = grouped
            .into_iter()
            .enumerate()
            .map(|(idx, (profile, pts))| {
                let label = profile_depth(&profile);
                cx.new(|cx| {
                    PlayerRatingTrendView::new(
                        format!("player-trend-{idx}-{profile}"),
                        label,
                        pts,
                        cx,
                    )
                })
            })
            .collect();
    }

    fn load_selected_matches(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            self.matches.clear();
            self.matches_total = 0;
            return;
        };
        let mut query = LibraryQuery {
            database_id: self.selected_database_id,
            sort: Some("played_at".to_string()),
            direction: Some("desc".to_string()),
            page: Some(self.matches_page as i64),
            limit: Some(MATCHES_PAGE_SIZE),
            ..Default::default()
        };
        match selection {
            PlayerSelection::Real(id) => query.player_id = Some(id),
            PlayerSelection::Virtual(id) => query.virtual_player_id = Some(id),
        }

        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.search_matches(query).await
        }) else {
            return;
        };
        self.matches_loading = true;
        cx.notify();

        self._matches_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.matches_loading = false;
                match res {
                    Ok(page) => {
                        ws.matches_total = page.total.unwrap_or(page.items.len() as i64);
                        ws.matches = page.items;
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn start_analyze_unrated(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        if self.analyzing_unrated {
            return;
        }
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let mut collected: Vec<DatabaseMatch> = Vec::new();
            let mut after: Option<String> = None;
            loop {
                let mut q = LibraryQuery {
                    sort: Some("played_at".to_string()),
                    direction: Some("desc".to_string()),
                    after: after.clone(),
                    limit: Some(200),
                    ..Default::default()
                };
                match selection {
                    PlayerSelection::Real(id) => q.player_id = Some(id),
                    PlayerSelection::Virtual(id) => q.virtual_player_id = Some(id),
                }
                let page = facade.search_matches(q).await?;
                collected.extend(page.items);
                if page.next_cursor.is_none() || collected.len() >= MAX_BULK_SCAN_MATCHES {
                    break;
                }
                after = page.next_cursor;
            }
            Ok::<Vec<Uuid>, neondojo::domain_error::DomainError>(
                filter_unrated_analyzable_match_ids(&collected),
            )
        }) else {
            return;
        };

        self.analyzing_unrated = true;
        self.status_notice = None;
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.analyzing_unrated = false;
                match res {
                    Ok(match_ids) if match_ids.is_empty() => {
                        ws.status_notice =
                            Some(I18n::t(locale, "player-directory", "nothingToAnalyze").into());
                    }
                    Ok(match_ids) => {
                        cx.emit(WorkspaceEvent::StartBulkAnalysis { match_ids });
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn remove_virtual_member(
        &mut self,
        vp_id: Uuid,
        profile_id: Uuid,
        version: i64,
        cx: &mut Context<Self>,
    ) {
        if self.removing_member.is_some() {
            return;
        }
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .remove_virtual_member(vp_id, profile_id, version)
                .await
        }) else {
            return;
        };
        self.removing_member = Some(profile_id);
        self.conflict_message = None;
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.removing_member = None;
                match res {
                    Ok(Ok(updated)) => {
                        ws.on_virtual_profile_saved(updated, cx);
                    }
                    Ok(Err(err)) => {
                        let msg: SharedString = format_domain_error(locale, &err).into();
                        if is_revision_conflict(&err) {
                            ws.conflict_message = Some(msg);
                        } else {
                            ws.error_message = Some(msg);
                        }
                    }
                    Err(err) => {
                        ws.error_message = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    // Callbacks invoked by child modals on successful mutations
    pub(crate) fn on_player_updated(&mut self, details: PlayerDetails, cx: &mut Context<Self>) {
        let sel = PlayerSelection::Real(details.profile.id);
        self.selection = Some(sel);
        let wrapped = SelectedDetails::Real(details);
        self.rebuild_trend_views(&wrapped, cx);
        self.details = Some(wrapped);
        self.load_players(cx);
        self.load_virtuals_and_databases(cx);
        cx.notify();
    }

    pub(crate) fn on_player_merged(&mut self, details: PlayerDetails, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        self.status_notice = Some(I18n::t(locale, "player-directory", "merged").into());
        self.on_player_updated(details, cx);
        self.load_selected_matches(cx);
    }

    pub(crate) fn on_virtual_profile_saved(
        &mut self,
        details: VirtualPlayerDetails,
        cx: &mut Context<Self>,
    ) {
        self.selection = Some(PlayerSelection::Virtual(details.profile.id));
        let wrapped = SelectedDetails::Virtual(details);
        self.rebuild_trend_views(&wrapped, cx);
        self.details = Some(wrapped);
        self.load_virtuals_and_databases(cx);
        self.load_players(cx);
        self.load_selected_matches(cx);
        cx.notify();
    }

    pub(crate) fn on_virtual_profile_deleted(&mut self, deleted_id: Uuid, cx: &mut Context<Self>) {
        if self.selection == Some(PlayerSelection::Virtual(deleted_id)) {
            self.selection = None;
            self.details = None;
            self.trend_views.clear();
            self.matches.clear();
        }
        self.virtuals.retain(|v| v.id != deleted_id);
        if self.selection.is_none()
            && let Some(next) = choose_initial_selection(&self.virtuals)
        {
            self.select_entry(next, cx);
        }
        self.load_virtuals_and_databases(cx);
        self.load_players(cx);
        cx.notify();
    }

    fn render_virtual_strip(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let create_ent = entity.clone();

        let mut cards = div().h_flex().items_center().gap(px(8.0)).flex_wrap();
        if self.virtuals.is_empty() {
            cards = cards.child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "virtual-player",
                "empty",
            )));
        } else {
            for vp in &self.virtuals {
                let id = vp.id;
                let selected = self.selection == Some(PlayerSelection::Virtual(id));
                let ent = entity.clone();
                cards = cards.child(
                    div()
                        .id(SharedString::from(format!("virtual-profile-pill-{id}")))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .bg(if selected { p.selected } else { p.surface_alt })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            ent.update(cx, |ws, cx| {
                                ws.select_entry(PlayerSelection::Virtual(id), cx);
                            });
                        })
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(vp.name.clone()),
                        )
                        .when(vp.is_me, |this| {
                            this.child(
                                div()
                                    .text_micro()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(p.accent)
                                    .text_color(p.accent_text)
                                    .child(I18n::t(locale, "virtual-player", "me")),
                            )
                        })
                        .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                            format!(
                                "{} · {}",
                                I18n::format(
                                    locale,
                                    "virtual-player",
                                    "memberCount",
                                    &[("count", Param::from(vp.member_count))],
                                ),
                                I18n::format(
                                    locale,
                                    "virtual-player",
                                    "matchCount",
                                    &[("count", Param::from(vp.match_count))],
                                )
                            ),
                        )),
                );
            }
        }

        div()
            .id("virtual-player-strip")
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .flex_wrap()
            .px(px(16.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(10.0))
                    .flex_wrap()
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.muted)
                            .child(I18n::t(locale, "virtual-player", "title")),
                    )
                    .child(cards),
            )
            .child(
                ActionButton::new("virtual-profile-create-btn")
                    .icon(ActionIconName::Add)
                    .label(I18n::t(locale, "virtual-player", "create"))
                    .palette(p)
                    .on_click(move |_, window, cx| {
                        open_create_virtual_profile_dialog(create_ent.clone(), window, cx);
                    }),
            )
    }

    fn render_master_list(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let last_page = ((self.players_total.max(0) + PLAYERS_PAGE_SIZE - 1) / PLAYERS_PAGE_SIZE)
            .max(1) as usize;

        let sort_btn = |id: &'static str, sort: PlayerSort, label_key: &'static str| {
            let active = self.sort == sort;
            let arrow = if active {
                if self.direction == PlayerSortDirection::Asc {
                    " ↑"
                } else {
                    " ↓"
                }
            } else {
                ""
            };
            let ent = entity.clone();
            ActionButton::new(id)
                .label(format!(
                    "{}{arrow}",
                    I18n::t(locale, "player-directory", label_key)
                ))
                .active(active)
                .ghost()
                .palette(p)
                .on_click(move |_, _, cx| {
                    ent.update(cx, |ws, cx| ws.toggle_sort(sort, cx));
                })
        };

        let mut rows = div()
            .id("players-results-list")
            .v_flex()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .gap(px(4.0))
            .p(px(8.0));

        if self.players_loading && self.players.is_empty() {
            rows = rows.child(
                div()
                    .p(px(12.0))
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "loading")),
            );
        } else if self.players.is_empty() {
            rows = rows.child(
                div()
                    .p(px(12.0))
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "empty")),
            );
        } else {
            for hit in &self.players {
                let pid = hit.profile.id;
                let selected = self.selection == Some(PlayerSelection::Real(pid));
                let ent = entity.clone();
                let sub_meta = if let Some(alias) = &hit.matching_alias {
                    Some(format!(
                        "{}: {} ({})",
                        I18n::t(locale, "player-directory", "matchedAlias"),
                        alias.name,
                        alias.platform
                    ))
                } else if !hit.name_platforms.is_empty() {
                    Some(hit.name_platforms.join(", "))
                } else {
                    None
                };

                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("player-row-{pid}")))
                        .v_flex()
                        .gap(px(2.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .bg(if selected { p.selected } else { p.surface })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            ent.update(cx, |ws, cx| {
                                ws.select_entry(PlayerSelection::Real(pid), cx);
                            });
                        })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_label()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(p.text)
                                        .child(hit.profile.display_name.clone()),
                                )
                                .when_some(hit.profile.virtual_profile.as_ref(), |this, vp| {
                                    this.child(
                                        div()
                                            .text_micro()
                                            .px(px(5.0))
                                            .py(px(1.0))
                                            .rounded(px(3.0))
                                            .bg(p.surface_alt)
                                            .text_color(p.muted)
                                            .child(vp.name.clone()),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .text_meta()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(format!(
                                    "{} {} · {} {}",
                                    format_int(locale, hit.profile.match_count),
                                    I18n::t(locale, "player-directory", "matches"),
                                    format_int(locale, hit.profile.game_count),
                                    I18n::t(locale, "player-directory", "games"),
                                )),
                        )
                        .when_some(sub_meta, |this, meta| {
                            this.child(div().text_micro().text_color(p.muted).child(meta))
                        }),
                );
            }
        }

        let page_ent = entity.clone();
        div()
            .v_flex()
            .w(px(320.0))
            .h_full()
            .min_h(px(0.0))
            .border_r_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_panel().text_color(p.text).child(I18n::t(
                                locale,
                                "player-directory",
                                "title",
                            )))
                            .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                                I18n::format(
                                    locale,
                                    "player-directory",
                                    "playerCount",
                                    &[("count", Param::from(self.players_total))],
                                ),
                            )),
                    )
                    .child(Input::new(&self.search_input))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(sort_btn("players-sort-name", PlayerSort::Name, "player"))
                            .child(sort_btn(
                                "players-sort-matches",
                                PlayerSort::Matches,
                                "matches",
                            ))
                            .child(sort_btn("players-sort-games", PlayerSort::Games, "games")),
                    ),
            )
            .child(rows)
            .when(last_page > 1, |this| {
                this.child(
                    div()
                        .h_flex()
                        .justify_center()
                        .p(px(8.0))
                        .border_t_1()
                        .border_color(p.border)
                        .child(
                            NumberedSearchPagination::new(
                                "players-list-pagination",
                                self.players_page,
                                last_page,
                            )
                            .disabled(self.players_loading)
                            .palette(p)
                            .on_select(move |page, _, cx| {
                                page_ent.update(cx, |ws, cx| {
                                    ws.players_page = page;
                                    ws.load_players(cx);
                                });
                            }),
                        ),
                )
            })
    }

    fn render_detail_pane(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let Some(details) = &self.details else {
            let msg = if self.details_loading {
                I18n::t(locale, "player-directory", "loading")
            } else {
                I18n::t(locale, "player-directory", "choose")
            };
            return div()
                .flex_1()
                .h_full()
                .p(px(24.0))
                .text_body()
                .text_color(p.muted)
                .child(msg)
                .into_any_element();
        };

        let (
            title,
            match_count,
            rateable_count,
            game_count,
            ratings,
            rating_history,
            is_virtual,
            is_me,
        ) = match details {
            SelectedDetails::Real(d) => (
                d.profile.display_name.clone(),
                d.profile.match_count,
                d.rateable_match_count,
                d.profile.game_count,
                &d.ratings,
                &d.rating_history,
                false,
                false,
            ),
            SelectedDetails::Virtual(v) => (
                v.profile.name.clone(),
                v.profile.match_count,
                v.profile.rateable_match_count,
                v.profile.game_count,
                &v.ratings,
                &v.rating_history,
                true,
                v.profile.is_me,
            ),
        };

        // Header action buttons
        let header_actions = match details {
            SelectedDetails::Real(d) => {
                let edit_det = d.clone();
                let merge_det = d.clone();
                let ent_edit = entity.clone();
                let ent_merge = entity.clone();
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("player-action-edit")
                            .icon(ActionIconName::Edit)
                            .label(I18n::t(locale, "player-directory", "editShort"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_edit_player_dialog(
                                    edit_det.clone(),
                                    ent_edit.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        ActionButton::new("player-action-merge")
                            .icon(ActionIconName::Merge)
                            .label(I18n::t(locale, "player-directory", "mergeShort"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_merge_player_dialog(
                                    merge_det.clone(),
                                    ent_merge.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .into_any_element()
            }
            SelectedDetails::Virtual(v) => {
                let add_det = v.clone();
                let ren_det = v.clone();
                let del_det = v.clone();
                let ent_add = entity.clone();
                let ent_ren = entity.clone();
                let ent_del = entity.clone();
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("vp-action-add-member")
                            .icon(ActionIconName::Add)
                            .label(I18n::t(locale, "virtual-player", "addMember"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_add_virtual_member_dialog(
                                    add_det.clone(),
                                    ent_add.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        ActionButton::new("vp-action-rename")
                            .icon(ActionIconName::Edit)
                            .label(I18n::t(locale, "virtual-player", "rename"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_rename_virtual_profile_dialog(
                                    &ren_det,
                                    ent_ren.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        ActionButton::new("vp-action-delete")
                            .icon(ActionIconName::Delete)
                            .label(I18n::t(locale, "virtual-player", "delete"))
                            .danger()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_delete_virtual_profile_dialog(
                                    &del_det,
                                    ent_del.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .into_any_element()
            }
        };

        // Aliases (real) or Members (virtual) strip
        let identity_strip = match details {
            SelectedDetails::Real(d) => {
                let mut strip = div().h_flex().items_center().gap(px(6.0)).flex_wrap();
                if let Some(vp) = &d.profile.virtual_profile {
                    let vp_id = vp.id;
                    let ent = entity.clone();
                    strip = strip.child(
                        div()
                            .id("player-part-of-vp")
                            .h_flex()
                            .items_center()
                            .gap(px(4.0))
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(p.accent)
                            .bg(p.selected)
                            .cursor_pointer()
                            .text_meta()
                            .text_color(p.accent)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |ws, cx| {
                                    ws.select_entry(PlayerSelection::Virtual(vp_id), cx);
                                });
                            })
                            .child(format!(
                                "{}: {}",
                                I18n::t(locale, "virtual-player", "partOf"),
                                vp.name
                            )),
                    );
                }
                for (idx, alias) in d.aliases.iter().enumerate() {
                    strip = strip.child(
                        div()
                            .id(SharedString::from(format!("detail-alias-{idx}")))
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.surface_alt)
                            .text_meta()
                            .text_color(p.text)
                            .child(format!("{} · {}", alias.name, alias.platform)),
                    );
                }
                strip.into_any_element()
            }
            SelectedDetails::Virtual(v) => {
                let vp_id = v.profile.id;
                let vp_ver = v.profile.version;
                let mut strip = div().h_flex().items_center().gap(px(6.0)).flex_wrap();
                if v.members.is_empty() {
                    strip = strip.child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "virtual-player",
                        "noMembers",
                    )));
                } else {
                    for member in &v.members {
                        let mid = member.id;
                        let ent_open = entity.clone();
                        let ent_rem = entity.clone();
                        strip = strip.child(
                            div()
                                .id(SharedString::from(format!("vp-member-chip-{mid}")))
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(p.border)
                                .bg(p.surface_alt)
                                .child(
                                    div()
                                        .id(SharedString::from(format!("vp-member-open-{mid}")))
                                        .cursor_pointer()
                                        .text_meta()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(p.accent)
                                        .on_click(move |_, _, cx| {
                                            ent_open.update(cx, |ws, cx| {
                                                ws.select_entry(PlayerSelection::Real(mid), cx);
                                            });
                                        })
                                        .child(format!(
                                            "{} ({})",
                                            member.display_name,
                                            format_int(locale, member.match_count)
                                        )),
                                )
                                .child(
                                    ActionButton::new(SharedString::from(format!(
                                        "vp-member-remove-{mid}"
                                    )))
                                    .icon(ActionIconName::Close)
                                    .tooltip(I18n::t(locale, "virtual-player", "removeMember"))
                                    .ghost()
                                    .dense_nav()
                                    .disabled(self.removing_member.is_some())
                                    .palette(p)
                                    .on_click(
                                        move |_, _, cx| {
                                            ent_rem.update(cx, |ws, cx| {
                                                ws.remove_virtual_member(vp_id, mid, vp_ver, cx);
                                            });
                                        },
                                    ),
                                ),
                        );
                    }
                }
                strip.into_any_element()
            }
        };

        // PR breakdown section (overall / checker / cube + per-profile badges + Analyze unrated)
        let primary_profile = ratings.first().map(|r| r.profile.as_str());
        let split = compute_pr_split_summary(rating_history, primary_profile);
        let max_rated = ratings.iter().map(|r| r.match_count).max().unwrap_or(0);
        let unrated_remaining = (rateable_count - max_rated).max(0);
        let analyze_ent = entity.clone();

        let pr_card = |label: SharedString, value: Option<f64>, accent_color: Hsla| {
            div()
                .v_flex()
                .gap(px(2.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt)
                .min_w(px(120.0))
                .child(div().text_meta().text_color(p.muted).child(label))
                .child(
                    div()
                        .text_subject()
                        .tabular_nums()
                        .text_color(if value.is_some() {
                            accent_color
                        } else {
                            p.muted
                        })
                        .child(
                            value
                                .map(|v| format_pr(locale, v))
                                .unwrap_or_else(|| "—".to_string()),
                        ),
                )
        };

        let mut profile_badges = div().h_flex().items_center().gap(px(8.0)).flex_wrap();
        for r in ratings {
            profile_badges = profile_badges.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(profile_depth(&r.profile)),
                    )
                    .child(
                        div()
                            .text_label()
                            .tabular_nums()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(format_pr(locale, r.value)),
                    )
                    .child(div().text_micro().tabular_nums().text_color(p.muted).child(
                        I18n::format(
                            locale,
                            "player-directory",
                            "coverage",
                            &[
                                ("rated", Param::from(r.match_count)),
                                ("total", Param::from(rateable_count)),
                            ],
                        ),
                    )),
            );
        }

        let pr_section = div()
            .v_flex()
            .gap(px(10.0))
            .p(px(14.0))
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
                    .flex_wrap()
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "player-directory",
                        "pr",
                    )))
                    .when(
                        unrated_remaining > 0 || ratings.is_empty() && rateable_count > 0,
                        |this| {
                            this.child(
                                ActionButton::new("player-analyze-unrated-btn")
                                    .icon(ActionIconName::Analysis)
                                    .label(I18n::format(
                                        locale,
                                        "player-directory",
                                        "analyzeRest",
                                        &[("count", Param::from(unrated_remaining.max(1)))],
                                    ))
                                    .disabled(self.analyzing_unrated)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        analyze_ent
                                            .update(cx, |ws, cx| ws.start_analyze_unrated(cx));
                                    }),
                            )
                        },
                    ),
            )
            .child(if ratings.is_empty() && split.overall_pr.is_none() {
                div()
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "noPr"))
                    .into_any_element()
            } else {
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .flex_wrap()
                            .child(pr_card(
                                I18n::t(locale, "player-directory", "prTrendTotal").into(),
                                split
                                    .overall_pr
                                    .or_else(|| ratings.first().map(|r| r.value)),
                                p.accent,
                            ))
                            .child(pr_card(
                                I18n::t(locale, "player-directory", "prTrendChecker").into(),
                                split.checker_pr,
                                p.success_text,
                            ))
                            .child(pr_card(
                                I18n::t(locale, "player-directory", "prTrendCube").into(),
                                split.cube_pr,
                                p.bad_text,
                            )),
                    )
                    .when(!ratings.is_empty(), |this| this.child(profile_badges))
                    .into_any_element()
            });

        // Rating Trend charts
        let mut trends_section = div().v_flex().gap(px(10.0));
        for trend in &self.trend_views {
            trends_section = trends_section.child(trend.clone());
        }

        // Opponent Breakdown table
        let opponents = compute_opponent_breakdown(&self.matches);
        let opponent_section = (!opponents.is_empty()).then(|| {
            let mut tbl = div()
                .v_flex()
                .gap(px(4.0))
                .p(px(14.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(div().text_panel().text_color(p.text).child(I18n::t(
                    locale,
                    "position-browser",
                    "opponent",
                )))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .border_b_1()
                        .border_color(p.border)
                        .text_table_heading()
                        .text_color(p.muted)
                        .child(div().flex_1().child(I18n::t(
                            locale,
                            "position-browser",
                            "opponent",
                        )))
                        .child(div().w(px(72.0)).child(I18n::t(
                            locale,
                            "player-directory",
                            "matches",
                        )))
                        .child(
                            div()
                                .w(px(90.0))
                                .child(I18n::t(locale, "library", "result")),
                        )
                        .child(div().w(px(76.0)).child(I18n::t(
                            locale,
                            "player-directory",
                            "prColumn",
                        ))),
                );

            for (idx, opp) in opponents.iter().take(12).enumerate() {
                let opp_id = opp.opponent_id;
                let opp_name = opp.name.clone();
                let ent = entity.clone();
                let record = if opp.draws > 0 {
                    format!("{}–{}–{}", opp.wins, opp.losses, opp.draws)
                } else {
                    format!("{}–{}", opp.wins, opp.losses)
                };
                tbl = tbl.child(
                    div()
                        .id(SharedString::from(format!("opponent-breakdown-row-{idx}")))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .border_b_1()
                        .border_color(p.border)
                        .text_table()
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.surface_alt))
                        .on_click(move |_, window, cx| {
                            let name = opp_name.clone();
                            ent.update(cx, |ws, cx| {
                                ws.select_opponent(opp_id, name, window, cx);
                            });
                        })
                        .child(
                            div()
                                .flex_1()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.accent)
                                .child(opp.name.clone()),
                        )
                        .child(
                            div()
                                .w(px(72.0))
                                .text_color(p.text)
                                .child(format_int(locale, opp.matches as i64)),
                        )
                        .child(div().w(px(90.0)).text_color(p.text).child(record))
                        .child(
                            div().w(px(76.0)).text_color(p.text).child(
                                opp.avg_pr
                                    .map(|v| format_pr(locale, v))
                                    .unwrap_or_else(|| "—".to_string()),
                            ),
                        ),
                );
            }
            tbl
        });

        // Match list with database filter + pagination
        let matches_last_page = ((self.matches_total.max(0) + MATCHES_PAGE_SIZE - 1)
            / MATCHES_PAGE_SIZE)
            .max(1) as usize;

        let mut db_bar = div().h_flex().items_center().gap(px(4.0)).flex_wrap();
        {
            let ent = entity.clone();
            db_bar = db_bar.child(
                ActionButton::new("player-matches-db-all")
                    .label(I18n::t(locale, "player-directory", "all"))
                    .active(self.selected_database_id.is_none())
                    .ghost()
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |ws, cx| {
                            ws.selected_database_id = None;
                            ws.matches_page = 1;
                            ws.load_selected_matches(cx);
                        });
                    }),
            );
        }
        for db in &self.databases {
            let db_id = db.id;
            let active = self.selected_database_id == Some(db_id);
            let ent = entity.clone();
            db_bar = db_bar.child(
                ActionButton::new(SharedString::from(format!("player-matches-db-{db_id}")))
                    .label(db.name.clone())
                    .active(active)
                    .ghost()
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |ws, cx| {
                            ws.selected_database_id = Some(db_id);
                            ws.matches_page = 1;
                            ws.load_selected_matches(cx);
                        });
                    }),
            );
        }

        let mut match_table = div()
            .v_flex()
            .gap(px(4.0))
            .p(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .flex_wrap()
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "player-directory",
                        "matches",
                    )))
                    .child(db_bar),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(
                        div()
                            .w(px(96.0))
                            .child(I18n::t(locale, "player-directory", "date")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(I18n::t(locale, "position-browser", "opponent")),
                    )
                    .child(div().w(px(86.0)).child(I18n::t(
                        locale,
                        "player-directory",
                        "matchFormat",
                    )))
                    .child(
                        div()
                            .w(px(80.0))
                            .child(I18n::t(locale, "library", "result")),
                    )
                    .child(
                        div()
                            .w(px(64.0))
                            .child(I18n::t(locale, "player-directory", "prColumn")),
                    )
                    .child(div().w(px(90.0))),
            );

        if self.matches_loading && self.matches.is_empty() {
            match_table = match_table.child(
                div()
                    .p(px(12.0))
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "loading")),
            );
        } else if self.matches.is_empty() {
            match_table = match_table.child(
                div()
                    .p(px(12.0))
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "player-directory", "noMatches")),
            );
        } else {
            for m in &self.matches {
                let view = resolve_match_perspective(m);
                let mid = m.id;
                let ent_open = entity.clone();
                let ent_opp = entity.clone();
                let opp_id = view.opponent_id;
                let opp_name = view.opponent_name.clone();

                let date_text = m
                    .played_on
                    .map(|d| format_naive_date_medium(locale, d))
                    .unwrap_or_else(|| "—".to_string());
                let format_text = if m.match_length > 0 {
                    I18n::format(
                        locale,
                        "app",
                        "matchLength",
                        &[("points", Param::from(i64::from(m.match_length)))],
                    )
                } else {
                    I18n::t(locale, "app", "common.moneyGame").to_string()
                };
                let score_text = match (view.player_score, view.opponent_score) {
                    (Some(ps), Some(os)) => format!("{ps}–{os}"),
                    _ => "—".to_string(),
                };
                let score_color = match view.outcome {
                    PlayerMatchOutcome::Win => p.success_text,
                    PlayerMatchOutcome::Loss => p.danger_text,
                    _ => p.text,
                };
                let opp_cell = if !view.both_sides {
                    div()
                        .id(SharedString::from(format!("match-opp-{mid}")))
                        .flex_1()
                        .cursor_pointer()
                        .text_color(p.accent)
                        .font_weight(FontWeight::MEDIUM)
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            let name = opp_name.clone();
                            ent_opp.update(cx, |ws, cx| {
                                ws.select_opponent(opp_id, name, window, cx);
                            });
                        })
                        .child(view.opponent_name)
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .text_color(p.muted)
                        .child(I18n::t(locale, "player-directory", "bothSides"))
                        .into_any_element()
                };

                match_table = match_table.child(
                    div()
                        .id(SharedString::from(format!("player-match-row-{mid}")))
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .border_b_1()
                        .border_color(p.border)
                        .text_table()
                        .child(div().w(px(96.0)).text_color(p.muted).child(date_text))
                        .child(opp_cell)
                        .child(div().w(px(86.0)).text_color(p.text).child(format_text))
                        .child(
                            div()
                                .w(px(80.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(score_color)
                                .child(score_text),
                        )
                        .child(
                            div().w(px(64.0)).text_color(p.text).child(
                                view.player_pr
                                    .map(|v| format_pr(locale, v))
                                    .unwrap_or_else(|| "—".to_string()),
                            ),
                        )
                        .child(
                            div().w(px(90.0)).h_flex().justify_end().child(
                                ActionButton::new(SharedString::from(format!(
                                    "player-open-match-{mid}"
                                )))
                                .icon(ActionIconName::Analysis)
                                .label(I18n::t(locale, "player-directory", "openMatch"))
                                .ghost()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_open.update(cx, |_, cx| {
                                        cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                            match_id: mid,
                                            game_no: None,
                                            position_id: None,
                                            decision_no: None,
                                        });
                                    });
                                }),
                            ),
                        ),
                );
            }
        }

        if matches_last_page > 1 {
            let page_ent = entity.clone();
            match_table = match_table.child(
                div().h_flex().justify_center().pt(px(8.0)).child(
                    NumberedSearchPagination::new(
                        "player-matches-pagination",
                        self.matches_page,
                        matches_last_page,
                    )
                    .disabled(self.matches_loading)
                    .palette(p)
                    .on_select(move |page, _, cx| {
                        page_ent.update(cx, |ws, cx| {
                            ws.matches_page = page;
                            ws.load_selected_matches(cx);
                        });
                    }),
                ),
            );
        }

        let reload_ent = entity;

        div()
            .id("players-detail-pane")
            .v_flex()
            .flex_1()
            .h_full()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .gap(px(14.0))
            .p(px(20.0))
            .when_some(self.status_notice.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("players-status-notice", BannerKind::Success, msg).palette(p),
                )
            })
            .when_some(self.conflict_message.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("players-conflict-banner", BannerKind::Warning, msg)
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "player-directory", "reload"),
                            move |_, _, cx| {
                                reload_ent.update(cx, |ws, cx| {
                                    ws.conflict_message = None;
                                    ws.load_selected_details(cx);
                                    ws.load_virtuals_and_databases(cx);
                                });
                            },
                        ),
                )
            })
            .when_some(self.error_message.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("players-error-banner", BannerKind::Error, msg).palette(p),
                )
            })
            // Header card
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
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .flex_wrap()
                            .child(
                                div()
                                    .v_flex()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div().text_page().text_color(p.text).child(title),
                                            )
                                            .when(is_virtual, |this| {
                                                this.child(
                                                    div()
                                                        .text_meta()
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(p.surface_alt)
                                                        .text_color(p.muted)
                                                        .child(I18n::t(
                                                            locale,
                                                            "virtual-player",
                                                            "kind",
                                                        )),
                                                )
                                            })
                                            .when(is_me, |this| {
                                                this.child(
                                                    div()
                                                        .text_meta()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(p.accent)
                                                        .text_color(p.accent_text)
                                                        .child(I18n::t(
                                                            locale,
                                                            "virtual-player",
                                                            "me",
                                                        )),
                                                )
                                            }),
                                    )
                                    .child(
                                        div().text_meta().tabular_nums().text_color(p.muted).child(
                                            I18n::format(
                                                locale,
                                                "player-directory",
                                                "counts",
                                                &[
                                                    ("matches", Param::from(match_count)),
                                                    ("games", Param::from(game_count)),
                                                ],
                                            ),
                                        ),
                                    ),
                            )
                            .child(header_actions),
                    )
                    .child(identity_strip),
            )
            .child(pr_section)
            .when(!self.trend_views.is_empty(), |this| {
                this.child(trends_section)
            })
            .when_some(opponent_section, |this, opp| this.child(opp))
            .child(match_table)
            .into_any_element()
    }
}

impl EventEmitter<WorkspaceEvent> for PlayersWorkspace {}

impl Render for PlayersWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);

        div()
            .id("players-workspace")
            .test_support()
            .v_flex()
            .size_full()
            .min_h(px(0.0))
            .bg(p.bg)
            .text_color(p.text)
            .child(self.render_virtual_strip(p, locale, cx))
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_master_list(p, locale, cx))
                    .child(self.render_detail_pane(p, locale, cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure_app;
    use crate::local_state::LocalUiState;
    use chrono::NaiveDate;
    use gpui_kit::{WindowOptions, test::TestWindowExt};
    use neondojo::player_directory::{PlayerAlias, PlayerProfile, PlayerRating, PlayerRatingPoint};
    use std::sync::{Arc, Mutex};

    #[gpui_kit::test]
    async fn players_workspace_renders_and_emits_analysis_events(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let opened = Arc::new(Mutex::new(None::<Uuid>));
        let bulk = Arc::new(Mutex::new(Vec::<Uuid>::new()));
        let opened_slot = Arc::clone(&opened);
        let bulk_slot = Arc::clone(&bulk);

        let (window, workspace) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
                    let ws = cx.new(|cx| PlayersWorkspace::new(window, cx));
                    cx.subscribe(&ws, move |_, event: &WorkspaceEvent, _| match event {
                        WorkspaceEvent::OpenMatchInAnalysis { match_id, .. } => {
                            *opened_slot.lock().unwrap() = Some(*match_id);
                        }
                        WorkspaceEvent::StartBulkAnalysis { match_ids } => {
                            *bulk_slot.lock().unwrap() = match_ids.clone();
                        }
                        _ => {}
                    })
                    .detach();
                    ws
                })
            })
            .expect("open players window");

        // Populate synthetic real player details + rating trend and draw full detail layout.
        let pid = Uuid::new_v4();
        let sample_details = PlayerDetails {
            profile: PlayerProfile {
                id: pid,
                display_name: "Hendrik".into(),
                first_name: "Hendrik".into(),
                last_name: "Reh".into(),
                version: 1,
                match_count: 12,
                game_count: 64,
                virtual_profile: None,
            },
            rateable_match_count: 12,
            aliases: vec![PlayerAlias {
                platform: "local".into(),
                name: "Hendrik".into(),
            }],
            ratings: vec![PlayerRating {
                profile: "gnubg-2ply".into(),
                value: 4.25,
                match_count: 3,
            }],
            rating_history: vec![
                PlayerRatingPoint {
                    match_id: Uuid::new_v4(),
                    date: NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                    profile: "gnubg-2ply".into(),
                    value: 5.0,
                    decisions: 100,
                    checker_value: Some(4.4),
                    cube_value: Some(6.2),
                },
                PlayerRatingPoint {
                    match_id: Uuid::new_v4(),
                    date: NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
                    profile: "gnubg-2ply".into(),
                    value: 3.5,
                    decisions: 100,
                    checker_value: Some(3.0),
                    cube_value: Some(4.8),
                },
            ],
        };

        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("players-workspace").visible());
            assert!(window.find("virtual-player-strip").visible());

            workspace.update(cx, |ws, cx| {
                ws.on_player_updated(sample_details, cx);
                assert_eq!(ws.current_selection(), Some(PlayerSelection::Real(pid)));
                assert_eq!(ws.trend_views.len(), 1);
            });
            window.render_frame(cx);
            assert!(window.find("player-trend-0-gnubg-2ply").visible());

            workspace.update(cx, |_, cx| {
                cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                    match_id: m1,
                    game_no: None,
                    position_id: None,
                    decision_no: None,
                });
                cx.emit(WorkspaceEvent::StartBulkAnalysis {
                    match_ids: vec![m1, m2],
                });
            });
        })
        .expect("update players window");

        cx.run_until_parked();

        assert_eq!(*opened.lock().unwrap(), Some(m1));
        assert_eq!(*bulk.lock().unwrap(), vec![m1, m2]);
    }

    #[gpui_kit::test]
    async fn clicking_opponent_breakdown_row_switches_selection_and_virtual_profile_renders(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use chrono::Utc;
        use neondojo::player_directory::VirtualPlayerMember;

        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let (window, workspace) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| PlayersWorkspace::new(window, cx))
                })
            })
            .expect("open players window");

        let vp_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let opp_id = Uuid::new_v4();

        let vp_details = VirtualPlayerDetails {
            profile: VirtualPlayerProfile {
                id: vp_id,
                name: "Hendrik (All)".into(),
                is_me: true,
                version: 2,
                member_count: 1,
                match_count: 5,
                rateable_match_count: 5,
                game_count: 25,
            },
            members: vec![VirtualPlayerMember {
                id: member_id,
                display_name: "Hendrik".into(),
                aliases: Vec::new(),
                match_count: 5,
            }],
            ratings: Vec::new(),
            rating_history: Vec::new(),
        };

        let match_row = DatabaseMatch {
            occurrence: None,
            player_sides: Some(vec![0]),
            opponent_player_id: Some(opp_id),
            id: Uuid::new_v4(),
            source_available: true,
            play_session_id: None,
            play_paused: false,
            study_report_id: None,
            play_phase: None,
            play_mode: None,
            play_assisted: false,
            match_no: 7,
            filename: "club.xg".into(),
            format: "xg".into(),
            white: "Hendrik".into(),
            black: "Opponent Alice".into(),
            match_length: 7,
            position_count: 88,
            created_at: Utc::now(),
            database_id: Uuid::new_v4(),
            display_name: "club".into(),
            played_on: NaiveDate::from_ymd_opt(2026, 3, 15),
            tournament: None,
            round: None,
            place: None,
            result_state: "white_won".into(),
            white_score: Some(7),
            black_score: Some(4),
            white_pr: Some(3.9),
            black_pr: Some(5.4),
            pr_method: None,
            pr_profile: None,
            analysis_status: Some("completed".into()),
            game_count: 5,
            import_warning_count: 0,
            tags: Vec::new(),
            comment: String::new(),
            version: 1,
        };

        cx.update_window(window, |_, window, cx| {
            workspace.update(cx, |ws, cx| {
                ws.virtuals = vec![vp_details.profile.clone()];
                ws.on_virtual_profile_saved(vp_details, cx);
                ws.matches = vec![match_row];
                ws.matches_total = 1;
                cx.notify();
            });
            window.render_frame(cx);
            assert_eq!(
                workspace.read(cx).current_selection(),
                Some(PlayerSelection::Virtual(vp_id))
            );
            assert!(
                window
                    .find(SharedString::from(format!("virtual-profile-pill-{vp_id}")))
                    .visible()
            );

            // Clicking the opponent breakdown row switches selection directly to that opponent!
            window.click("opponent-breakdown-row-0", cx);
            window.render_frame(cx);
            assert_eq!(
                workspace.read(cx).current_selection(),
                Some(PlayerSelection::Real(opp_id))
            );
        })
        .expect("update players window for opponent switch");
    }
}
