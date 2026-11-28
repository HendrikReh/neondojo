//! Settings → Analysis section & nested `OpeningBookDialog`
//! (`AnalysisSettingsSection.tsx` & `OpeningBookDialog.tsx`).

use crate::i18n::{current_locale, format_domain_error};
use crate::settings::draft_guard::SettingsDraft;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::decimal_input::DecimalInput;
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use gpui_kit::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, Styled, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::configuration::{ConfigurationBookProvider, ConfigurationEngine, ConfigurationScope};
use neondojo::domain_error::ErrorKind;
use neondojo::facade::BackendFacade;
use neondojo::opening_book::{BookConfigWrite, BookStatus};
use neondojo::settings::EngineInfo;
use neondojo::xg_book::XgBookStatus;
use neondojo_i18n::{I18n, Param, format_ply};
use neondojo_rules::options::engine_depths;
use std::rc::Rc;

pub type SelectAnalysisEngineHandler =
    Rc<dyn Fn(ConfigurationEngine, &mut Window, &mut App) + 'static>;
pub type SelectPlyHandler = Rc<dyn Fn(i32, &mut Window, &mut App) + 'static>;
pub type SelectScopeHandler = Rc<dyn Fn(ConfigurationScope, &mut Window, &mut App) + 'static>;
pub type OpenBooksHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// Handles to the 4 `DecimalInput` entities in Settings → Analysis.
#[derive(Clone)]
pub struct ThresholdInputEntities {
    pub questionable: Entity<DecimalInput>,
    pub error: Entity<DecimalInput>,
    pub blunder: Entity<DecimalInput>,
    pub large_blunder: Entity<DecimalInput>,
}

/// Renders Settings → Analysis (engine, ply depth, scope, 4 strictly-increasing
/// `DecimalInput` thresholds, and Opening Books trigger).
#[allow(clippy::too_many_arguments)]
pub fn render_analysis_section(
    draft: &SettingsDraft,
    engines: &[EngineInfo],
    threshold_inputs: &ThresholdInputEntities,
    disabled: bool,
    palette: NeonPalette,
    on_engine: SelectAnalysisEngineHandler,
    on_ply: SelectPlyHandler,
    on_scope: SelectScopeHandler,
    on_manage_books: OpenBooksHandler,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let selected_engine = &draft.draft_config.analysis.engine;
    let engine_id_str = match selected_engine {
        ConfigurationEngine::Gnubg => "gnubg",
        ConfigurationEngine::OpenSage => "bgsage",
    };

    let engine_available = |id: &str| {
        engines
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.available)
            .unwrap_or(true)
    };

    let engine_row = div()
        .h_flex()
        .gap(px(8.0))
        .child({
            let on_engine = on_engine.clone();
            let avail = engine_available("gnubg");
            ActionButton::new("settings-analysis-engine-gnubg")
                .label(if avail {
                    "GNUbg".to_string()
                } else {
                    format!("GNUbg — {}", I18n::t(locale, "settings", "unavailable"))
                })
                .active(*selected_engine == ConfigurationEngine::Gnubg)
                .disabled(disabled || !avail)
                .palette(palette)
                .on_click(move |_, window, cx| on_engine(ConfigurationEngine::Gnubg, window, cx))
        })
        .child({
            let avail = engine_available("bgsage");
            ActionButton::new("settings-analysis-engine-bgsage")
                .label(if avail {
                    "NeonGammon".to_string()
                } else {
                    format!(
                        "NeonGammon — {}",
                        I18n::t(locale, "settings", "unavailable")
                    )
                })
                .active(*selected_engine == ConfigurationEngine::OpenSage)
                .disabled(disabled || !avail)
                .palette(palette)
                .on_click(move |_, window, cx| on_engine(ConfigurationEngine::OpenSage, window, cx))
        });

    let depths = engine_depths(engine_id_str, "analysis");
    let mut depth_row = div().flex().flex_row().flex_wrap().gap(px(6.0));
    for ply in depths {
        let on_ply = on_ply.clone();
        depth_row = depth_row.child(
            ActionButton::new(SharedString::from(format!("settings-analysis-ply-{ply}")))
                .label(format_ply(locale, ply.max(0) as u32))
                .active(draft.draft_config.analysis.plies == ply)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_ply(ply, window, cx)),
        );
    }

    let scope_row = div()
        .h_flex()
        .gap(px(8.0))
        .child({
            let on_scope = on_scope.clone();
            ActionButton::new("settings-analysis-scope-game")
                .label(I18n::t(locale, "app", "gameAnalysis.label"))
                .active(draft.draft_config.analysis.scope == ConfigurationScope::Game)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_scope(ConfigurationScope::Game, window, cx))
        })
        .child(
            ActionButton::new("settings-analysis-scope-match")
                .label(I18n::t(locale, "app", "gameAnalysis.scopeMatch"))
                .active(draft.draft_config.analysis.scope == ConfigurationScope::Match)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_scope(ConfigurationScope::Match, window, cx)),
        );

    let threshold_grid = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(12.0))
        .child(render_threshold_field(
            I18n::t(locale, "app", "severity.doubtful"),
            &threshold_inputs.questionable,
            palette,
        ))
        .child(render_threshold_field(
            I18n::t(locale, "app", "severity.bad"),
            &threshold_inputs.error,
            palette,
        ))
        .child(render_threshold_field(
            I18n::t(locale, "app", "severity.blunder"),
            &threshold_inputs.blunder,
            palette,
        ))
        .child(render_threshold_field(
            I18n::t(locale, "app", "severity.very_bad"),
            &threshold_inputs.large_blunder,
            palette,
        ));

    let book_provider_label = match draft.draft_config.opening_book.provider {
        ConfigurationBookProvider::Off => I18n::t(locale, "analysis-source", "off"),
        ConfigurationBookProvider::Automatic => I18n::t(locale, "analysis-source", "automatic"),
        ConfigurationBookProvider::Backgammondb => "BackgammonDB",
        ConfigurationBookProvider::Xg => "XG",
    };

    div()
        .id("settings-section-analysis")
        .v_flex()
        .gap(px(16.0))
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "analysisEngine",
                )))
                .child(engine_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "analysisProfile",
                )))
                .child(depth_row)
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "analysisHelp",
                ))),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_panel()
                        .text_color(palette.text)
                        .child(I18n::t(locale, "profile", "scope")),
                )
                .child(scope_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "errorThresholds",
                )))
                .child(threshold_grid)
                .when(!draft.thresholds_valid(), |this| {
                    this.child(
                        StatusBanner::new(
                            "settings-invalid-thresholds-banner",
                            BannerKind::Error,
                            I18n::t(locale, "settings", "invalidErrorThresholds"),
                        )
                        .palette(palette),
                    )
                })
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "errorThresholdsHelp",
                ))),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .pt(px(10.0))
                .border_t_1()
                .border_color(palette.border)
                .child(
                    div()
                        .v_flex()
                        .gap(px(2.0))
                        .child(div().text_panel().text_color(palette.text).child(I18n::t(
                            locale,
                            "analysis-source",
                            "books",
                        )))
                        .child(div().text_meta().text_color(palette.muted).child(format!(
                            "{}: {}",
                            I18n::t(locale, "analysis-source", "provider"),
                            book_provider_label
                        ))),
                )
                .child(
                    ActionButton::new("settings-manage-books-button")
                        .icon(ActionIconName::Open)
                        .label(I18n::t(locale, "settings", "manageBooks"))
                        .disabled(disabled)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_manage_books(window, cx)),
                ),
        )
}

fn render_threshold_field(
    label: &'static str,
    input: &Entity<DecimalInput>,
    palette: NeonPalette,
) -> impl IntoElement {
    div()
        .v_flex()
        .gap(px(4.0))
        .child(
            div()
                .text_label()
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.muted)
                .child(label),
        )
        .child(input.clone())
}

/// Interactive view hosted inside the nested Opening Book modal dialog.
pub struct OpeningBookDialogView {
    facade: Option<BackendFacade>,
    version: i64,
    provider: String,
    path_input: Entity<InputState>,
    xg_path_input: Entity<InputState>,
    bgdb_status: Option<BookStatus>,
    xg_status: Option<XgBookStatus>,
    busy: ModalBusyLock,
    error_message: Option<SharedString>,
    conflict: bool,
}

impl OpeningBookDialogView {
    pub fn new(
        facade: Option<BackendFacade>,
        initial_provider: String,
        initial_path: String,
        initial_xg_path: String,
        busy: ModalBusyLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_path));
        let xg_path_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_xg_path));

        let mut view = Self {
            facade,
            version: 1,
            provider: initial_provider,
            path_input,
            xg_path_input,
            bgdb_status: None,
            xg_status: None,
            busy,
            error_message: None,
            conflict: false,
        };
        view.reload(window, cx);
        view
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.busy.set_busy(true);
        self.error_message = None;
        self.conflict = false;
        let task = TokioBridge::spawn_read(cx, async move { facade.get_opening_book().await });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy.set_busy(false);
                match result {
                    Ok(Ok(settings)) => {
                        view.version = settings.config.version;
                        view.provider = settings.config.provider;
                        view.path_input.update(cx, |inp, cx| {
                            inp.set_value(settings.config.path, window, cx);
                        });
                        view.xg_path_input.update(cx, |inp, cx| {
                            inp.set_value(settings.config.xg_path, window, cx);
                        });
                        view.bgdb_status = Some(settings.status);
                        view.xg_status = Some(settings.xg_status);
                    }
                    Ok(Err(err)) => {
                        let locale = current_locale(cx);
                        view.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn check_bgdb(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let path = self.path_input.read(cx).value().to_string();
        self.busy.set_busy(true);
        let task =
            TokioBridge::spawn_read(cx, async move { facade.check_opening_book(path).await });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update_in(cx, |view, _, cx| {
                view.busy.set_busy(false);
                match res {
                    Ok(Ok(status)) => view.bgdb_status = Some(status),
                    Ok(Err(err)) => {
                        let locale = current_locale(cx);
                        view.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn check_xg(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let path = self.xg_path_input.read(cx).value().to_string();
        self.busy.set_busy(true);
        let task =
            TokioBridge::spawn_read(cx, async move { facade.check_xg_opening_book(path).await });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update_in(cx, |view, _, cx| {
                view.busy.set_busy(false);
                match res {
                    Ok(Ok(status)) => view.xg_status = Some(status),
                    Ok(Err(err)) => {
                        let locale = current_locale(cx);
                        view.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            window.close_dialog(cx);
            return;
        };
        let write = BookConfigWrite {
            version: self.version,
            provider: self.provider.clone(),
            path: self.path_input.read(cx).value().to_string(),
            xg_path: Some(self.xg_path_input.read(cx).value().to_string()),
        };
        self.busy.set_busy(true);
        self.error_message = None;
        self.conflict = false;
        let task =
            TokioBridge::spawn_write(cx, async move { facade.save_opening_book(write).await });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy.set_busy(false);
                match res {
                    Ok(Ok(saved)) => {
                        view.version = saved.config.version;
                        view.bgdb_status = Some(saved.status);
                        view.xg_status = Some(saved.xg_status);
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        let locale = current_locale(cx);
                        view.conflict = err.kind == ErrorKind::Conflict;
                        view.error_message = Some(if view.conflict {
                            I18n::t(locale, "analysis-source", "conflict").into()
                        } else {
                            format_domain_error(locale, &err).into()
                        });
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for OpeningBookDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let is_busy = self.busy.is_busy();
        let entity = cx.entity();

        let providers = [
            ("automatic", I18n::t(locale, "analysis-source", "automatic")),
            ("backgammondb", "BackgammonDB"),
            ("xg", "XG"),
            ("off", I18n::t(locale, "analysis-source", "off")),
        ];
        let mut provider_row = div().h_flex().gap(px(6.0));
        for (id, label) in providers {
            let ent = entity.clone();
            provider_row = provider_row.child(
                ActionButton::new(SharedString::from(format!("book-provider-{id}")))
                    .label(label)
                    .active(self.provider == id)
                    .disabled(is_busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |view, cx| {
                            view.provider = id.to_string();
                            cx.notify();
                        });
                    }),
            );
        }

        let bgdb_summary = self.bgdb_status.as_ref().map(|s| {
            if s.available {
                I18n::format(
                    locale,
                    "analysis-source",
                    "available",
                    &[("count", Param::from(s.entries))],
                )
            } else {
                s.error
                    .clone()
                    .unwrap_or_else(|| I18n::t(locale, "analysis-source", "unavailable").into())
            }
        });

        let xg_summary = self.xg_status.as_ref().map(|s| {
            if s.available {
                I18n::format(
                    locale,
                    "analysis-source",
                    "xgReady",
                    &[("count", Param::from(s.entries))],
                )
            } else {
                s.error
                    .clone()
                    .unwrap_or_else(|| I18n::t(locale, "analysis-source", "unavailable").into())
            }
        });

        div()
            .id("opening-book-dialog-body")
            .v_flex()
            .gap(px(14.0))
            .when_some(self.error_message.clone(), |this, msg| {
                this.child(StatusBanner::new("book-error", BannerKind::Error, msg).palette(p))
            })
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(I18n::t(locale, "analysis-source", "provider")),
                    )
                    .child(provider_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(p.text).child(I18n::t(
                        locale,
                        "analysis-source",
                        "path",
                    )))
                    .child(
                        div()
                            .h_flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.path_input).id("book-bgdb-path-input")),
                            )
                            .child(
                                ActionButton::new("book-check-bgdb")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::t(locale, "analysis-source", "check"))
                                    .disabled(is_busy)
                                    .palette(p)
                                    .on_click({
                                        let ent = entity.clone();
                                        move |_, window, cx| {
                                            ent.update(cx, |v, cx| v.check_bgdb(window, cx));
                                        }
                                    }),
                            ),
                    )
                    .when_some(bgdb_summary, |this, s| {
                        this.child(div().text_meta().text_color(p.muted).child(s))
                    }),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(p.text).child(I18n::t(
                        locale,
                        "analysis-source",
                        "xgPath",
                    )))
                    .child(
                        div()
                            .h_flex()
                            .gap(px(8.0))
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.xg_path_input).id("book-xg-path-input"),
                                ),
                            )
                            .child(
                                ActionButton::new("book-check-xg")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::t(locale, "analysis-source", "checkXg"))
                                    .disabled(is_busy)
                                    .palette(p)
                                    .on_click({
                                        let ent = entity.clone();
                                        move |_, window, cx| {
                                            ent.update(cx, |v, cx| v.check_xg(window, cx));
                                        }
                                    }),
                            ),
                    )
                    .when_some(xg_summary, |this, s| {
                        this.child(div().text_meta().text_color(p.muted).child(s))
                    }),
            )
            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "analysis-source",
                "configurationHelp",
            )))
    }
}

/// Opens the nested Opening Book management modal over the Settings dialog.
pub fn open_opening_book_dialog(
    window: &mut Window,
    cx: &mut App,
    facade: Option<BackendFacade>,
    initial_provider: String,
    initial_path: String,
    initial_xg_path: String,
) {
    let locale = current_locale(cx);
    let busy = ModalBusyLock::new(false);
    let view = cx.new(|cx| {
        OpeningBookDialogView::new(
            facade,
            initial_provider,
            initial_path,
            initial_xg_path,
            busy.clone(),
            window,
            cx,
        )
    });

    let config = ModalConfig::new(I18n::t(locale, "analysis-source", "books"))
        .size(ModalSize::Standard)
        .busy_lock(busy.clone());

    let view_for_body = view.clone();
    let view_for_footer = view;
    open_modal_dialog(
        window,
        cx,
        config,
        move |_: &mut Window, _: &mut App| view_for_body.clone().into_any_element(),
        Some(move |_: &mut Window, cx: &mut App| {
            let locale = current_locale(cx);
            let p = NeonTheme::current_palette(cx);
            let is_busy = busy.is_busy();
            let save_ent = view_for_footer.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("book-dialog-cancel")
                        .label(I18n::t(locale, "analysis-source", "cancel"))
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("book-dialog-save")
                        .icon(ActionIconName::Save)
                        .label(I18n::t(locale, "analysis-source", "save"))
                        .primary()
                        .disabled(is_busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            save_ent.update(cx, |v, cx| v.save(window, cx));
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}
