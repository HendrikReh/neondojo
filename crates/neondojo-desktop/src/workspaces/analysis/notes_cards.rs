//! Notes and Cards Inspector sub-views + `MoveNotesDialog`:
//! - **Notes tab**: `NotesPanel` (`Match` / `Game` / `Position` scopes),
//!   read-only `ImportNotes` warnings, persistent drafts surviving game &
//!   position navigation (`NotesDraftCache`), HTTP 409 `expected_version`
//!   conflict banner preserving user drafts, ≤30 tags / ≤64 chars, and
//!   `MoveNotesDialog` browsing all game decisions (incl. cube-only actions)
//!   with mini `BoardView` + imported analysis + per-move notes.
//! - **Cards tab**: `AddCardsPanel` + `CardSuggestionsPanel` with deck picker,
//!   inline deck creation, and mini `BoardView` previews per suggestion.

use super::state::{
    ActionCb, ClickCb, MAX_ANNOTATION_TAGS, MAX_COMMENT_CHARS, NoteDraft, NoteTargetKey,
    NotesDraftCache, run_facade,
};
use crate::app_context::DesktopAppContext;
use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use gpui_kit::{
    App, AppContext, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
    base::StyledExt,
    component::input::{Input, InputEvent, InputState, Textarea, TextareaState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::{
    annotations::{AnnotationKind, AnnotationUpdate},
    imported_analysis::ImportedAnalysisRecord,
    learning::{CardSuggestion, CardSuggestions, Deck, SuggestionReference},
    model::{ImportWarning, ImportWarningKind},
    move_catalog::StoredMove,
};
use neondojo_board::{BoardPosition, BoardView};
use neondojo_i18n::{I18n, format_decimal};
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

// ── Notes Panel ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct NotesCallbacks {
    pub on_select_scope: ActionCb<AnnotationKind>,
    pub on_add_tag: ClickCb,
    pub on_remove_tag: ActionCb<String>,
    pub on_save_note: ClickCb,
    pub on_load_latest_conflict: ClickCb,
    pub on_rebase_and_save_conflict: ClickCb,
    pub on_open_move_notes_dialog: ClickCb,
}

pub struct NotesViewProps<'a> {
    pub scope: AnnotationKind,
    pub target_key: Option<NoteTargetKey>,
    pub draft: Option<&'a NoteDraft>,
    pub saving: bool,
    pub saved_feedback: bool,
    pub validation_error: Option<&'a str>,
    pub tag_input: &'a Entity<InputState>,
    pub comment_input: &'a Entity<TextareaState>,
    pub import_warnings: &'a [ImportWarning],
    pub has_game_target: bool,
    pub has_position_target: bool,
    pub source_available: bool,
    pub callbacks: NotesCallbacks,
    pub palette: NeonPalette,
}

/// Renders the Notes inspector tab (`Match` / `Game` / `Position` scopes + ImportNotes).
pub fn render_notes_inspector(props: NotesViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;

    let mut scope_items = vec![
        TextNavItem::new("match", "Match")
            .element_id("notes-scope-match")
            .disabled(!props.source_available),
        TextNavItem::new("game", "Game")
            .element_id("notes-scope-game")
            .disabled(!props.has_game_target),
        TextNavItem::new("position", "Position")
            .element_id("notes-scope-position")
            .disabled(!props.has_position_target),
    ];
    if !props.source_available {
        scope_items.retain(|i| i.id.as_ref() == "position");
    }

    let active_scope_id = match props.scope {
        AnnotationKind::Match => "match",
        AnnotationKind::Game => "game",
        AnnotationKind::Position | AnnotationKind::Move => "position",
    };

    let on_scope = props.callbacks.on_select_scope.clone();
    let scope_nav = TextNavBar::new("notes-scope-nav", scope_items)
        .active(Some(active_scope_id))
        .density(TextNavDensity::Compact)
        .palette(p)
        .on_select(move |id, w, cx| {
            let kind = match id {
                "match" => AnnotationKind::Match,
                "game" => AnnotationKind::Game,
                _ => AnnotationKind::Position,
            };
            on_scope(kind, w, cx);
        });

    let on_move_notes = props.callbacks.on_open_move_notes_dialog.clone();
    let on_add_tag = props.callbacks.on_add_tag.clone();
    let on_save = props.callbacks.on_save_note.clone();
    let on_load_latest = props.callbacks.on_load_latest_conflict.clone();
    let on_rebase = props.callbacks.on_rebase_and_save_conflict.clone();

    let tags = props.draft.map(|d| d.tags.clone()).unwrap_or_default();
    let dirty = props.draft.is_some_and(|d| d.dirty);
    let version = props.draft.map(|d| d.base_version).unwrap_or(0);
    let conflict_err = props.draft.and_then(|d| d.conflict_error.as_deref());
    let comment_len = props.comment_input.read(cx).value().chars().count();

    let mut root = div()
        .id("analysis-notes-inspector")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .pb(px(4.0))
                .border_b_1()
                .border_color(p.border)
                .child(scope_nav)
                .when(props.has_game_target, |el| {
                    el.child(
                        ActionButton::new("open-move-notes-dialog-btn")
                            .label("Move notes")
                            .icon(ActionIconName::Notes)
                            .palette(p)
                            .on_click(move |ev, w, cx| on_move_notes(ev, w, cx)),
                    )
                }),
        );

    // 409 optimistic-concurrency conflict banner preserving the user's draft
    if let Some(err_msg) = conflict_err {
        let load_cb = on_load_latest.clone();
        let rebase_cb = on_rebase.clone();
        root = root.child(
            div()
                .id("notes-conflict-banner")
                .test_support()
                .v_flex()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(6.0))
                .bg(p.warning_bg)
                .border_1()
                .border_color(p.danger_border)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.warning_text)
                        .child("Conflict (HTTP 409) — your draft was preserved"),
                )
                .child(
                    div()
                        .text_meta()
                        .text_color(p.text)
                        .child(err_msg.to_string()),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("notes-conflict-load-latest")
                                .label("Load latest (discard draft)")
                                .palette(p)
                                .on_click(move |ev, w, cx| load_cb(ev, w, cx)),
                        )
                        .child(
                            ActionButton::new("notes-conflict-overwrite")
                                .label("Overwrite with my draft")
                                .primary()
                                .palette(p)
                                .on_click(move |ev, w, cx| rebase_cb(ev, w, cx)),
                        ),
                ),
        );
    }

    if let Some(val_err) = props.validation_error {
        root = root.child(
            StatusBanner::new(
                "notes-validation-error",
                BannerKind::Error,
                val_err.to_string(),
            )
            .palette(p),
        );
    }

    // Tag editor (<= 30 tags)
    let tag_section = div()
        .v_flex()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child("Tags"),
                )
                .child(
                    div()
                        .text_micro()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(format!("{}/{MAX_ANNOTATION_TAGS}", tags.len())),
                ),
        )
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .gap(px(4.0))
                .children(tags.iter().enumerate().map(|(i, tag)| {
                    let t_rem = tag.clone();
                    let on_rem = props.callbacks.on_remove_tag.clone();
                    div()
                        .id(SharedString::from(format!("note-tag-chip-{i}")))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(7.0))
                        .py(px(2.0))
                        .rounded(px(99.0))
                        .bg(p.selected)
                        .border_1()
                        .border_color(p.border)
                        .text_micro()
                        .text_color(p.text)
                        .child(tag.clone())
                        .child(
                            div()
                                .id(SharedString::from(format!("note-tag-del-{i}")))
                                .cursor_pointer()
                                .text_color(p.muted)
                                .on_click(move |_, w, cx| on_rem(t_rem.clone(), w, cx))
                                .child("×"),
                        )
                })),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .child(div().flex_1().child(Input::new(props.tag_input)))
                .child(
                    ActionButton::new("note-add-tag-btn")
                        .label("Add tag")
                        .icon(ActionIconName::Tags)
                        .disabled(tags.len() >= MAX_ANNOTATION_TAGS || props.target_key.is_none())
                        .palette(p)
                        .on_click(move |ev, w, cx| on_add_tag(ev, w, cx)),
                ),
        );

    // Comment editor (<= 20,000 chars) + Save button
    let comment_section = div()
        .v_flex()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child("Comment (Markdown)"),
                )
                .child(
                    div()
                        .text_micro()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(format!(
                            "v{version} · {comment_len}/{MAX_COMMENT_CHARS}{}",
                            if dirty { " · Unsaved" } else { "" }
                        )),
                ),
        )
        .child(Textarea::new(props.comment_input).aria_label("Comment (Markdown)"))
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_micro()
                        .text_color(if props.saved_feedback {
                            p.success_text
                        } else {
                            p.muted
                        })
                        .child(if props.saved_feedback {
                            "Saved"
                        } else if dirty {
                            "Draft preserved across navigation"
                        } else {
                            ""
                        }),
                )
                .child(
                    ActionButton::new("note-save-btn")
                        .label(if props.saving {
                            "Saving…"
                        } else {
                            "Save note"
                        })
                        .icon(ActionIconName::Save)
                        .primary()
                        .disabled(props.saving || props.target_key.is_none())
                        .palette(p)
                        .on_click(move |ev, w, cx| on_save(ev, w, cx)),
                ),
        );

    root = root.child(tag_section).child(comment_section);

    // Read-only ImportNotes section
    if !props.import_warnings.is_empty() {
        root = root.child(
            div()
                .id("analysis-import-notes-section")
                .test_support()
                .v_flex()
                .gap(px(4.0))
                .p(px(10.0))
                .rounded(px(6.0))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.warning_text)
                        .child(format!("Import notes ({})", props.import_warnings.len())),
                )
                .children(props.import_warnings.iter().map(|w| {
                    let desc = format_import_warning(w);
                    div().text_meta().text_color(p.muted).child(desc)
                })),
        );
    }

    let _ = locale;
    root
}

fn format_import_warning(w: &ImportWarning) -> String {
    let kind_str = match w.kind {
        ImportWarningKind::IllegalPlayReplaced => "Illegal play replaced with setup",
        ImportWarningKind::CustomStartPosition => "Custom start position",
        ImportWarningKind::UnfinishedGame => "Unfinished game",
        ImportWarningKind::IllegalPlayTruncated => "Illegal play truncated",
        ImportWarningKind::UnreadablePlayTruncated => "Unreadable play truncated",
        ImportWarningKind::ScoreMismatch => "Score mismatch with header",
    };
    let ply_part = w
        .source_ply
        .map(|p| format!(" · move {p}"))
        .unwrap_or_default();
    let text_part = w
        .source_text
        .as_deref()
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    format!("Game {}{ply_part}: {kind_str}{text_part}", w.game_no)
}

// ── MoveNotesDialog Modal ────────────────────────────────────────────────────

struct MoveNotesDialogState {
    moves: Vec<StoredMove>,
    selected_idx: usize,
    imported: Vec<ImportedAnalysisRecord>,
    drafts: NotesDraftCache,
    saving: bool,
    status_msg: Option<String>,
    board_view: Entity<BoardView>,
    comment_input: Entity<TextareaState>,
    _comment_subscription: Subscription,
    tag_input: Entity<InputState>,
}

/// Opens the `MoveNotesDialog` modal browsing all game decisions (checker + cube)
/// with a mini `BoardView`, imported analysis records, and per-move notes editor.
pub fn open_move_notes_dialog(
    match_id: Uuid,
    game_no: i32,
    moves: Vec<StoredMove>,
    initial_move_id: Option<Uuid>,
    players: [String; 2],
    window: &mut Window,
    cx: &mut App,
) {
    let atlas = cx
        .try_global::<DesktopAppContext>()
        .map(|ctx| ctx.board_atlas.clone())
        .unwrap_or_default();

    let initial_idx = initial_move_id
        .and_then(|mid| moves.iter().position(|m| m.id == mid))
        .unwrap_or(0);

    let board_view = cx.new(|cx| {
        let mut bv = BoardView::new("move-notes-mini-board", atlas);
        bv.set_players(players[0].clone(), players[1].clone(), cx);
        bv.set_show_player_bars(false, cx);
        bv
    });
    let comment_input = cx.new(|cx| {
        TextareaState::new(window, cx)
            .rows(4)
            .placeholder("Move comment…")
    });
    let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add tag…"));

    let state = cx.new(|cx| {
        let subscription = cx.subscribe(
            &comment_input,
            |m: &mut MoveNotesDialogState, field, event, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(mv) = m.moves.get(m.selected_idx)
                {
                    m.status_msg = m
                        .drafts
                        .ensure_entry(move_note_key(mv))
                        .set_comment(field.read(cx).value().to_string())
                        .err();
                    cx.notify();
                }
            },
        );
        MoveNotesDialogState {
            moves,
            selected_idx: initial_idx,
            imported: Vec::new(),
            drafts: NotesDraftCache::default(),
            saving: false,
            status_msg: None,
            board_view,
            comment_input,
            _comment_subscription: subscription,
            tag_input,
        }
    });

    load_move_notes_selection(&state, window, cx);

    let cfg = ModalConfig::new(format!("Move notes · Game {game_no}")).size(ModalSize::Wide);
    let body_state = state.clone();
    open_modal_dialog(
        window,
        cx,
        cfg,
        move |_window: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let s = body_state.read(cx);
            let cur_move = s.moves.get(s.selected_idx);
            let total = s.moves.len();
            let sel_idx = s.selected_idx;

            let move_list = div()
                .id("move-notes-dialog-list")
                .v_flex()
                .w(px(210.0))
                .max_h(px(420.0))
                .overflow_y_scroll()
                .gap(px(3.0))
                .pr(px(6.0))
                .border_r_1()
                .border_color(p.border)
                .children(s.moves.iter().enumerate().map(|(ix, mv)| {
                    let is_sel = ix == sel_idx;
                    let st = body_state.clone();
                    div()
                        .id(SharedString::from(format!("move-notes-item-{ix}")))
                        .test_support()
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(if is_sel { p.selected } else { p.surface })
                        .border_1()
                        .border_color(if is_sel { p.accent } else { p.border })
                        .cursor_pointer()
                        .text_micro()
                        .text_color(p.text)
                        .on_click(move |_, w, cx| {
                            st.update(cx, |m, cx| {
                                m.selected_idx = ix;
                                m.imported.clear();
                                m.status_msg = None;
                                cx.notify();
                            });
                            load_move_notes_selection(&st, w, cx);
                        })
                        .child(format!("#{} [{}] {}", mv.move_no, mv.kind, mv.notation))
                }));

            let right_detail = if let Some(mv) = cur_move {
                let st_save = body_state.clone();
                let st_tag = body_state.clone();
                let key = move_note_key(mv);
                let draft = s.drafts.get(key);
                let tags = draft.map(|d| d.tags.clone()).unwrap_or_default();
                let saving = s.saving;
                let status = s.status_msg.clone();

                div()
                    .v_flex()
                    .flex_1()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "Event #{}/{total} · {} · {}",
                                        mv.move_no, mv.kind, mv.notation
                                    )),
                            )
                            .child(
                                div()
                                    .mono_font()
                                    .text_micro()
                                    .text_color(p.muted)
                                    .child(mv.position_id.clone()),
                            ),
                    )
                    .child(
                        div()
                            .w(px(360.0))
                            .h(px(230.0))
                            .rounded(px(6.0))
                            .overflow_hidden()
                            .border_1()
                            .border_color(p.border)
                            .child(s.board_view.clone()),
                    )
                    .when(!s.imported.is_empty(), |el| {
                        el.child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .p(px(6.0))
                                .rounded(px(5.0))
                                .bg(p.surface_alt)
                                .child(
                                    div()
                                        .text_micro()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(p.muted)
                                        .child("Imported source analysis"),
                                )
                                .children(s.imported.iter().take(3).map(|rec| {
                                    div().text_micro().text_color(p.text).child(format!(
                                        "{} · {} ({})",
                                        rec.data.engine,
                                        rec.data.depth.as_deref().unwrap_or("—"),
                                        rec.data.method
                                    ))
                                })),
                        )
                    })
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(div().flex_1().child(Input::new(&s.tag_input)))
                            .child(
                                ActionButton::new("move-notes-add-tag")
                                    .label("Add tag")
                                    .palette(p)
                                    .on_click(move |_, w, cx| {
                                        let raw =
                                            st_tag.read(cx).tag_input.read(cx).value().to_string();
                                        let tag_ent = st_tag.read(cx).tag_input.clone();
                                        st_tag.update(cx, |m, cx| {
                                            if let Err(err) =
                                                m.drafts.ensure_entry(key).add_tag(&raw)
                                            {
                                                m.status_msg = Some(err);
                                            }
                                            cx.notify();
                                        });
                                        tag_ent.update(cx, |inp, cx| inp.set_value("", w, cx));
                                    }),
                            ),
                    )
                    .when(!tags.is_empty(), |el| {
                        el.child(div().h_flex().flex_wrap().gap(px(4.0)).children(
                            tags.into_iter().map(|t| {
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(99.0))
                                    .bg(p.selected)
                                    .text_micro()
                                    .text_color(p.text)
                                    .child(t)
                            }),
                        ))
                    })
                    .child(
                        div()
                            .id("move-notes-comment")
                            .test_support()
                            .child(Textarea::new(&s.comment_input).aria_label("Move comment")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_micro()
                                    .text_color(p.muted)
                                    .child(status.unwrap_or_default()),
                            )
                            .child(
                                ActionButton::new("move-notes-save")
                                    .label(if saving {
                                        "Saving…"
                                    } else {
                                        "Save move note"
                                    })
                                    .primary()
                                    .disabled(saving)
                                    .palette(p)
                                    .on_click(move |_, _w, cx| {
                                        save_move_note_from_dialog(&st_save, match_id, key, cx);
                                    }),
                            ),
                    )
                    .into_any_element()
            } else {
                div()
                    .text_meta()
                    .text_color(p.muted)
                    .child("No moves recorded in this game.")
                    .into_any_element()
            };

            div()
                .id("move-notes-dialog-body")
                .test_support()
                .h_flex()
                .gap(px(12.0))
                .child(move_list)
                .child(right_detail)
                .into_any_element()
        },
        None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        None,
    );
}

fn move_note_key(mv: &StoredMove) -> NoteTargetKey {
    if mv.kind == "checker"
        && let Some(position_id) = mv.position_uuid
    {
        NoteTargetKey {
            kind: AnnotationKind::Position,
            target_id: position_id,
        }
    } else {
        NoteTargetKey {
            kind: AnnotationKind::Move,
            target_id: mv.id,
        }
    }
}

fn load_move_notes_selection(
    state: &Entity<MoveNotesDialogState>,
    window: &mut Window,
    cx: &mut App,
) {
    let (mv_opt, board_view, comment_input) = {
        let s = state.read(cx);
        (
            s.moves.get(s.selected_idx).cloned(),
            s.board_view.clone(),
            s.comment_input.clone(),
        )
    };
    let Some(mv) = mv_opt else {
        return;
    };

    let mut board_arr = [0i32; 26];
    for (dst, &src) in board_arr.iter_mut().zip(mv.board.iter()) {
        *dst = src;
    }
    let bp = BoardPosition {
        board: board_arr,
        player: u8::try_from(mv.player).unwrap_or(0).min(1),
        dice: mv
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok())
            .collect(),
        gnubg_match_id: mv.gnubg_match_id.clone(),
        notation: mv.notation.clone(),
    };
    board_view.update(cx, |bv, cx| bv.set_position(bp, cx));

    let key = move_note_key(&mv);
    let move_id = mv.id;
    let comment = state
        .read(cx)
        .drafts
        .get(key)
        .map(|d| d.comment.clone())
        .unwrap_or_default();
    comment_input.update(cx, |field, cx| field.set_value(comment, window, cx));

    let Some(facade) = cx
        .try_global::<DesktopAppContext>()
        .and_then(|ctx| ctx.facade())
    else {
        return;
    };

    let read = TokioBridge::spawn_read(cx, async move {
        run_facade(move || async move {
            let ann = facade.get_annotations(key.kind, key.target_id).await?;
            let imported = facade
                .get_move_imported_analysis(move_id)
                .await
                .unwrap_or_default();
            Ok((ann, imported))
        })
        .await
    });

    let state_weak = state.downgrade();
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok((ann, imported))) = read.await else {
                return;
            };
            let _ = cx.update(|window, cx| {
                let Some(st) = state_weak.upgrade() else {
                    return;
                };
                st.update(cx, |m, cx| {
                    let draft = m.drafts.merge_loaded(&ann);
                    if m.moves
                        .get(m.selected_idx)
                        .is_some_and(|mv| mv.id == move_id)
                    {
                        if comment_input.read(cx).value().as_ref() != draft.comment {
                            comment_input.update(cx, |inp, cx| {
                                inp.set_value(draft.comment.clone(), window, cx)
                            });
                        }
                        m.imported = imported;
                        cx.notify();
                    }
                });
            });
        })
        .detach();
}

fn save_move_note_from_dialog(
    state: &Entity<MoveNotesDialogState>,
    _match_id: Uuid,
    key: NoteTargetKey,
    cx: &mut App,
) {
    let Some(facade) = cx
        .try_global::<DesktopAppContext>()
        .and_then(|ctx| ctx.facade())
    else {
        return;
    };
    let comment_val = state.read(cx).comment_input.read(cx).value().to_string();
    let Some((tags, expected_version)) = state.update(cx, |m, cx| {
        let d = m.drafts.ensure_entry(key);
        if let Err(error) = d.set_comment(comment_val.clone()) {
            m.status_msg = Some(error);
            cx.notify();
            return None;
        }
        m.saving = true;
        cx.notify();
        Some((d.tags.clone(), d.base_version))
    }) else {
        return;
    };

    let submitted = AnnotationUpdate {
        tags,
        comment: comment_val,
        expected_version,
    };
    let write = TokioBridge::spawn_write(cx, super::state::save_note(facade, key, submitted));

    let state_weak = state.downgrade();
    cx.spawn(async move |cx| {
        let res = write.await;
        cx.update(|cx| {
            let Some(st) = state_weak.upgrade() else {
                return;
            };
            st.update(cx, |m, cx| {
                m.saving = false;
                match res {
                    Ok(Ok((saved, submitted))) => {
                        m.drafts.mark_saved(&saved, &submitted);
                        m.status_msg = (m
                            .moves
                            .get(m.selected_idx)
                            .is_some_and(|mv| move_note_key(mv) == key)
                            && !m.drafts.has_dirty(key))
                        .then(|| "Saved".to_string());
                    }
                    Ok(Err(err)) => {
                        m.drafts.mark_conflict(key, &err);
                        m.status_msg = Some(err.to_string());
                    }
                    Err(err) => {
                        m.status_msg = Some(format!("Save interrupted: {err}"));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}

// ── Cards Inspector Tab ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CardsCallbacks {
    pub on_select_deck: ActionCb<Uuid>,
    pub on_create_deck: ClickCb,
    pub on_add_current_position: ClickCb,
    pub on_add_match_errors: ClickCb,
    pub on_refresh_suggestions: ClickCb,
    pub on_toggle_suggestion: ActionCb<SuggestionReference>,
    pub on_save_selected_suggestions: ClickCb,
}

pub struct CardsViewProps<'a> {
    pub decks: &'a [Deck],
    pub selected_deck_id: Option<Uuid>,
    pub new_deck_input: &'a Entity<InputState>,
    pub has_position: bool,
    pub source_available: bool,
    pub busy: bool,
    pub status_message: Option<&'a str>,
    pub suggestions: Option<&'a CardSuggestions>,
    pub suggestions_loading: bool,
    pub selected_suggestions: &'a BTreeSet<SuggestionReference>,
    pub suggestion_boards: &'a HashMap<SuggestionReference, Entity<BoardView>>,
    pub callbacks: CardsCallbacks,
    pub palette: NeonPalette,
}

/// Renders the Cards inspector tab (`AddCardsPanel` + `CardSuggestionsPanel` with mini `BoardView`s).
pub fn render_cards_inspector(props: CardsViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;

    let on_create = props.callbacks.on_create_deck.clone();
    let on_add_pos = props.callbacks.on_add_current_position.clone();
    let on_add_err = props.callbacks.on_add_match_errors.clone();
    let on_refresh_sug = props.callbacks.on_refresh_suggestions.clone();
    let on_save_sug = props.callbacks.on_save_selected_suggestions.clone();

    let deck_picker = div()
        .v_flex()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .text_label()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child("Add flashcards to deck"),
        )
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .gap(px(4.0))
                .children(props.decks.iter().map(|deck| {
                    let did = deck.id;
                    let is_sel = props.selected_deck_id == Some(did);
                    let on_pick = props.callbacks.on_select_deck.clone();
                    ActionButton::new(SharedString::from(format!("cards-deck-{did}")))
                        .label(format!("{} ({})", deck.name, deck.card_count))
                        .active(is_sel)
                        .palette(p)
                        .on_click(move |_, w, cx| on_pick(did, w, cx))
                })),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .child(div().flex_1().child(Input::new(props.new_deck_input)))
                .child(
                    ActionButton::new("cards-create-deck-btn")
                        .label("Create deck")
                        .icon(ActionIconName::Add)
                        .disabled(props.busy)
                        .palette(p)
                        .on_click(move |ev, w, cx| on_create(ev, w, cx)),
                ),
        )
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .gap(px(6.0))
                .child(
                    ActionButton::new("cards-add-current-position")
                        .label("Add current position")
                        .icon(ActionIconName::Cards)
                        .primary()
                        .disabled(
                            props.busy || !props.has_position || props.selected_deck_id.is_none(),
                        )
                        .palette(p)
                        .on_click(move |ev, w, cx| on_add_pos(ev, w, cx)),
                )
                .when(props.source_available, |el| {
                    el.child(
                        ActionButton::new("cards-add-match-errors")
                            .label("Add match mistakes")
                            .icon(ActionIconName::Deck)
                            .disabled(props.busy || props.selected_deck_id.is_none())
                            .palette(p)
                            .on_click(move |ev, w, cx| on_add_err(ev, w, cx)),
                    )
                }),
        )
        .when_some(props.status_message, |el, msg| {
            el.child(
                div()
                    .text_meta()
                    .text_color(p.accent)
                    .child(msg.to_string()),
            )
        });

    // CardSuggestionsPanel with mini BoardView previews
    let mut suggestions_box = div()
        .id("analysis-card-suggestions-panel")
        .test_support()
        .v_flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(I18n::t(locale, "card-suggestion", "suggest")),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .child(
                            ActionButton::new("cards-refresh-suggestions")
                                .label("Refresh")
                                .icon(ActionIconName::Refresh)
                                .disabled(
                                    props.suggestions_loading || props.selected_deck_id.is_none(),
                                )
                                .palette(p)
                                .on_click(move |ev, w, cx| on_refresh_sug(ev, w, cx)),
                        )
                        .when(!props.selected_suggestions.is_empty(), |el| {
                            el.child(
                                ActionButton::new("cards-save-suggestions")
                                    .label(format!(
                                        "Save selected ({})",
                                        props.selected_suggestions.len()
                                    ))
                                    .primary()
                                    .disabled(props.busy)
                                    .palette(p)
                                    .on_click(move |ev, w, cx| on_save_sug(ev, w, cx)),
                            )
                        }),
                ),
        );

    if props.suggestions_loading {
        suggestions_box =
            suggestions_box.child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "card-suggestion",
                "loading",
            )));
    } else if let Some(sug) = props.suggestions {
        if !sug.has_analysis {
            suggestions_box =
                suggestions_box.child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "card-suggestion",
                    "noAnalysis",
                )));
        } else if sug.items.is_empty() {
            suggestions_box =
                suggestions_box.child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "card-suggestion",
                    "empty",
                )));
        } else {
            suggestions_box =
                suggestions_box.children(sug.items.iter().enumerate().map(|(ix, item)| {
                    render_suggestion_card_row(
                        ix,
                        item,
                        props.selected_suggestions.contains(&item.reference),
                        props.suggestion_boards.get(&item.reference),
                        &props.callbacks,
                        p,
                        cx,
                    )
                }));
        }
    } else {
        suggestions_box = suggestions_box.child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child("Choose a deck and click Refresh to preview suggested flashcards."),
        );
    }

    div()
        .id("analysis-cards-inspector")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .child(deck_picker)
        .when(props.source_available, |el| el.child(suggestions_box))
}

fn render_suggestion_card_row(
    ix: usize,
    item: &CardSuggestion,
    checked: bool,
    mini_board: Option<&Entity<BoardView>>,
    callbacks: &CardsCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let r = item.reference;
    let on_toggle = callbacks.on_toggle_suggestion.clone();

    div()
        .id(SharedString::from(format!("card-suggestion-{ix}")))
        .test_support()
        .h_flex()
        .items_center()
        .gap(px(10.0))
        .p(px(8.0))
        .rounded(px(6.0))
        .bg(if checked { p.selected } else { p.surface_alt })
        .border_1()
        .border_color(if checked { p.accent } else { p.border })
        .cursor_pointer()
        .on_click(move |_, w, cx| on_toggle(r, w, cx))
        .when_some(mini_board, |el, bv| {
            el.child(
                div()
                    .w(px(136.0))
                    .h(px(94.0))
                    .flex_shrink_0()
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(p.border)
                    .child(bv.clone()),
            )
        })
        .child(
            div()
                .v_flex()
                .flex_1()
                .gap(px(3.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(format!(
                                    "G{} #{} · {}",
                                    r.game_no, r.decision_no, item.player_name
                                )),
                        )
                        .child(
                            div()
                                .text_micro()
                                .tabular_nums()
                                .text_color(p.danger_text)
                                .child(format!(
                                    "-{}",
                                    format_decimal(locale, item.equity_loss, 3, 3)
                                )),
                        ),
                )
                .child(div().text_micro().text_color(p.muted).child(format!(
                    "{:?} · {:?}{}",
                    item.mistake_kind,
                    item.game_plan,
                    if item.in_deck {
                        " · Already in deck"
                    } else {
                        ""
                    }
                )))
                .child(
                    div()
                        .text_micro()
                        .text_color(if checked { p.accent } else { p.muted })
                        .child(if checked {
                            "Selected to add"
                        } else {
                            "Click to select"
                        }),
                ),
        )
}
