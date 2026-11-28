//! Mutually-exclusive accordion search panels above the library table:
//! 1. **Filters** + [`AppliedFilterChips`]
//! 2. **Notes** search (`facade.search_annotations`)
//! 3. **Position ID** search (`/^[A-Za-z0-9+/]{14}$/` → `facade.search_matches`)
//! 4. **DuplicateMatchFinder** (cursor-paged 50/page → `facade.find_duplicates` + `Select copies`)

use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::filter_chips::AppliedFilterChips;
use crate::ui::icons::ActionIconName;
use crate::ui::pagination::NumberedSearchPagination;
use crate::workspaces::library::LibraryWorkspace;
use crate::workspaces::library::state::{
    NOTES_PAGE_SIZE, SearchPanelKind, clone_database_match, database_display_name,
    format_naive_date, format_utc_date, is_valid_position_id,
};
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt,
    base::StyledExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::annotations::AnnotationKind;
use neondojo::duplicate_matches::DuplicateMatchKind;
use neondojo_i18n::{I18n, Locale, Param};

impl LibraryWorkspace {
    pub(super) fn render_search_section(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let chips = self.filters.applied_chips(locale);

        let search_bar = div()
            .id("library-search-bar")
            .test_support()
            .h_flex()
            .items_center()
            .flex_wrap()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(260.0))
                    .child(Input::new(&self.inputs.q).id("library-search-q-input")),
            )
            .child({
                let ent = entity.clone();
                ActionButton::new("library-search-submit-btn")
                    .icon(ActionIconName::Search)
                    .label(I18n::t(locale, "app", "app.searchMatches"))
                    .primary()
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| this.apply_current_filters(cx));
                    })
            })
            .child({
                let ent = entity.clone();
                ActionButton::new("library-panel-toggle-filters")
                    .icon(ActionIconName::Filter)
                    .label(I18n::t(locale, "library", "filters"))
                    .active(self.active_search_panel == Some(SearchPanelKind::Filters))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| {
                            this.toggle_panel(SearchPanelKind::Filters, cx);
                        });
                    })
            })
            .child({
                let ent = entity.clone();
                ActionButton::new("library-panel-toggle-notes")
                    .icon(ActionIconName::Notes)
                    .label(I18n::t(locale, "workspace", "notes"))
                    .active(self.active_search_panel == Some(SearchPanelKind::Notes))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| {
                            this.toggle_panel(SearchPanelKind::Notes, cx);
                        });
                    })
            })
            .child({
                let ent = entity.clone();
                ActionButton::new("library-panel-toggle-position")
                    .icon(ActionIconName::Search)
                    .label(I18n::t(locale, "library", "position"))
                    .active(self.active_search_panel == Some(SearchPanelKind::Position))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| {
                            this.toggle_panel(SearchPanelKind::Position, cx);
                        });
                    })
            })
            .child({
                let ent = entity.clone();
                ActionButton::new("library-panel-toggle-duplicates")
                    .icon(ActionIconName::Duplicates)
                    .label(I18n::t(locale, "library", "duplicates"))
                    .active(self.active_search_panel == Some(SearchPanelKind::Duplicates))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| {
                            this.toggle_panel(SearchPanelKind::Duplicates, cx);
                        });
                    })
            });

        let mut section = div()
            .id("library-search-section")
            .v_flex()
            .gap(px(8.0))
            .child(search_bar);

        if !chips.is_empty() {
            let ent_remove = entity.clone();
            let ent_clear = entity.clone();
            section = section.child(
                AppliedFilterChips::new("library-applied-filter-chips", chips)
                    .clear_all_label(I18n::t(locale, "library", "clearFilters"))
                    .palette(p)
                    .on_remove(move |chip_id, window, cx| {
                        let id = chip_id.to_string();
                        ent_remove.update(cx, |this, cx| {
                            this.remove_filter_chip(&id, Some(window), cx);
                        });
                    })
                    .on_clear_all(move |window, cx| {
                        ent_clear.update(cx, |this, cx| {
                            this.clear_all_filters(Some(window), cx);
                        });
                    }),
            );
        }

        match self.active_search_panel {
            Some(SearchPanelKind::Filters) => {
                section = section.child(self.render_filters_panel(p, locale, cx));
            }
            Some(SearchPanelKind::Notes) => {
                section = section.child(self.render_notes_panel(p, locale, cx));
            }
            Some(SearchPanelKind::Position) => {
                section = section.child(self.render_position_panel(p, locale, cx));
            }
            Some(SearchPanelKind::Duplicates) => {
                section = section.child(self.render_duplicates_panel(p, locale, cx));
            }
            None => {}
        }

        section
    }

    fn render_labeled_input(
        label: &str,
        input_id: &'static str,
        input: &Entity<InputState>,
        width_px: f32,
        p: NeonPalette,
    ) -> impl IntoElement {
        div()
            .v_flex()
            .gap(px(4.0))
            .w(px(width_px))
            .child(
                div()
                    .text_meta()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .child(label.to_string()),
            )
            .child(Input::new(input).id(input_id))
    }

    fn render_filters_panel(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();

        let date_field_toggle = {
            let is_imported = self.filters.date_field == "imported";
            let ent_played = entity.clone();
            let ent_imported = entity.clone();
            div()
                .v_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_meta()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(I18n::t(locale, "library", "dateField")),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(4.0))
                        .child(
                            ActionButton::new("library-filter-date-played")
                                .label(I18n::t(locale, "library", "played"))
                                .active(!is_imported)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_played.update(cx, |this, cx| {
                                        this.filters.date_field = "played".to_string();
                                        this.apply_current_filters(cx);
                                    });
                                }),
                        )
                        .child(
                            ActionButton::new("library-filter-date-imported")
                                .label(I18n::t(locale, "library", "imported"))
                                .active(is_imported)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_imported.update(cx, |this, cx| {
                                        this.filters.date_field = "imported".to_string();
                                        this.apply_current_filters(cx);
                                    });
                                }),
                        ),
                )
        };

        let match_type_options = [
            ("", I18n::t(locale, "library", "any")),
            ("match", I18n::t(locale, "library", "match")),
            ("money", I18n::t(locale, "library", "money")),
        ];
        let mut match_type_row = div().h_flex().gap(px(4.0)).flex_wrap();
        for (val, label) in match_type_options {
            let active = self.filters.match_type == val;
            let ent = entity.clone();
            let val_owned = val.to_string();
            match_type_row = match_type_row.child(
                ActionButton::new(SharedString::from(format!("library-filter-type-{val}")))
                    .label(label)
                    .active(active)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        let v = val_owned.clone();
                        ent.update(cx, |this, cx| {
                            this.filters.match_type = v;
                            this.apply_current_filters(cx);
                        });
                    }),
            );
        }

        let result_options = [
            ("", I18n::t(locale, "library", "any")),
            ("completed", I18n::t(locale, "library", "completed")),
            ("white_won", I18n::t(locale, "library", "whiteWon")),
            ("black_won", I18n::t(locale, "library", "blackWon")),
            ("draw", I18n::t(locale, "library", "draw")),
            ("ongoing", I18n::t(locale, "library", "ongoing")),
            ("unknown", I18n::t(locale, "library", "unknown")),
        ];
        let mut result_row = div().h_flex().gap(px(4.0)).flex_wrap();
        for (val, label) in result_options {
            let active = self.filters.result == val;
            let ent = entity.clone();
            let val_owned = val.to_string();
            result_row = result_row.child(
                ActionButton::new(SharedString::from(format!("library-filter-result-{val}")))
                    .label(label)
                    .active(active)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        let v = val_owned.clone();
                        ent.update(cx, |this, cx| {
                            this.filters.result = v;
                            this.apply_current_filters(cx);
                        });
                    }),
            );
        }

        div()
            .id("library-panel-filters")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "player"),
                        "library-filter-player-input",
                        &self.inputs.player,
                        170.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "matchName"),
                        "library-filter-match-name-input",
                        &self.inputs.match_name,
                        170.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "tournament"),
                        "library-filter-tournament-input",
                        &self.inputs.tournament,
                        160.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "round"),
                        "library-filter-round-input",
                        &self.inputs.round,
                        120.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "tag"),
                        "library-filter-tag-input",
                        &self.inputs.tag,
                        150.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "comment"),
                        "library-filter-comment-input",
                        &self.inputs.comment,
                        180.0,
                        p,
                    ))
                    .child(date_field_toggle)
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "dateFrom"),
                        "library-filter-date-from-input",
                        &self.inputs.date_from,
                        130.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "dateTo"),
                        "library-filter-date-to-input",
                        &self.inputs.date_to,
                        130.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "minPoints"),
                        "library-filter-min-points-input",
                        &self.inputs.match_points_min,
                        120.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "library", "maxPoints"),
                        "library-filter-max-points-input",
                        &self.inputs.match_points_max,
                        120.0,
                        p,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_meta()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child(I18n::t(locale, "library", "type")),
                            )
                            .child(match_type_row),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_meta()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child(I18n::t(locale, "library", "result")),
                            )
                            .child(result_row),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-filters-apply-btn")
                                    .icon(ActionIconName::Search)
                                    .label(I18n::t(locale, "app", "app.searchMatches"))
                                    .primary()
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| this.apply_current_filters(cx));
                                    })
                            })
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-filters-clear-btn")
                                    .icon(ActionIconName::Clear)
                                    .label(I18n::t(locale, "library", "clearFilters"))
                                    .palette(p)
                                    .on_click(move |_, window, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.clear_all_filters(Some(window), cx);
                                        });
                                    })
                            }),
                    ),
            )
    }

    fn render_notes_panel(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let notes = &self.notes_search;

        let kind_options: [(Option<AnnotationKind>, &str, &str); 3] = [
            (None, "all", I18n::t(locale, "annotation", "allKinds")),
            (
                Some(AnnotationKind::Match),
                "match",
                I18n::t(locale, "annotation", "matches"),
            ),
            (
                Some(AnnotationKind::Position),
                "position",
                I18n::t(locale, "annotation", "positions"),
            ),
        ];

        let mut kind_row = div().h_flex().gap(px(4.0));
        for (kind_opt, slug, label) in kind_options {
            let ent = entity.clone();
            kind_row = kind_row.child(
                ActionButton::new(SharedString::from(format!("library-notes-kind-{slug}")))
                    .label(label)
                    .active(notes.kind == kind_opt)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |this, cx| {
                            this.notes_search.kind = kind_opt;
                            cx.notify();
                        });
                    }),
            );
        }

        let total_pages = notes
            .total
            .map(|t| (t.max(0) as usize).div_ceil(NOTES_PAGE_SIZE))
            .unwrap_or(1)
            .max(1);

        let mut results_box = div()
            .id("library-notes-results")
            .v_flex()
            .gap(px(6.0))
            .max_h(px(280.0))
            .overflow_y_scroll();

        for (idx, hit) in notes.hits.iter().enumerate() {
            let hit_clone = hit.clone();
            let ent_open = entity.clone();
            let kind_label = match hit.target_kind {
                AnnotationKind::Match => I18n::t(locale, "annotation", "match"),
                AnnotationKind::Position => I18n::t(locale, "annotation", "position"),
                AnnotationKind::Game => I18n::t(locale, "library", "game"),
                AnnotationKind::Move => I18n::t(locale, "library", "sourceMove"),
            };
            let open_label = if matches!(hit.target_kind, AnnotationKind::Match) {
                I18n::t(locale, "annotation", "openMatch")
            } else {
                I18n::t(locale, "annotation", "openPosition")
            };

            let mut meta_parts = vec![
                kind_label.to_string(),
                format!("{} — {}", hit.white, hit.black),
            ];
            if let Some(g) = hit.game_no {
                meta_parts.push(I18n::format(
                    locale,
                    "annotation",
                    "game",
                    &[("number", Param::from(g as i64))],
                ));
            }
            if let Some(ply) = hit.ply {
                meta_parts.push(I18n::format(
                    locale,
                    "annotation",
                    "ply",
                    &[("number", Param::from(ply as i64))],
                ));
            }
            meta_parts.push(format_utc_date(hit.updated_at));

            let mut tag_row = div().h_flex().flex_wrap().gap(px(4.0));
            for (t_idx, tag) in hit.tags.iter().enumerate() {
                let tag_clone = tag.clone();
                let ent_tag = entity.clone();
                tag_row = tag_row.child(
                    div()
                        .id(SharedString::from(format!(
                            "library-note-tag-{idx}-{t_idx}"
                        )))
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(p.surface_alt)
                        .border_1()
                        .border_color(p.border)
                        .text_meta()
                        .text_color(p.accent)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.selected))
                        .on_click(move |_, window, cx| {
                            let tag_val = tag_clone.clone();
                            ent_tag.update(cx, |this, cx| {
                                this.search_notes_by_exact_tag(tag_val, window, cx);
                            });
                        })
                        .child(tag.clone()),
                );
            }

            results_box = results_box.child(
                div()
                    .id(SharedString::from(format!("library-note-hit-{idx}")))
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
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
                                    .text_label()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.text)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .child(meta_parts.join(" · ")),
                            )
                            .child(
                                ActionButton::new(SharedString::from(format!(
                                    "library-note-open-{idx}"
                                )))
                                .icon(ActionIconName::Open)
                                .label(open_label)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    let target = hit_clone.clone();
                                    ent_open.update(cx, |this, cx| {
                                        this.open_annotation_hit(target, cx);
                                    });
                                }),
                            ),
                    )
                    .when(!hit.tags.is_empty(), |el| el.child(tag_row))
                    .when(!hit.comment_excerpt.trim().is_empty(), |el| {
                        el.child(
                            div()
                                .text_body()
                                .text_color(p.muted)
                                .child(hit.comment_excerpt.clone()),
                        )
                    }),
            );
        }

        div()
            .id("library-panel-notes")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(div().text_panel().text_color(p.text).child(I18n::t(
                locale,
                "annotation",
                "searchTitle",
            )))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap(px(10.0))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "annotation", "query"),
                        "library-notes-query-input",
                        &self.inputs.notes_query,
                        220.0,
                        p,
                    ))
                    .child(Self::render_labeled_input(
                        I18n::t(locale, "annotation", "exactTag"),
                        "library-notes-tag-input",
                        &self.inputs.notes_tag,
                        180.0,
                        p,
                    ))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_meta()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child(I18n::t(locale, "annotation", "kind")),
                            )
                            .child(kind_row),
                    )
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-notes-search-btn")
                            .icon(ActionIconName::Search)
                            .label(if notes.loading {
                                I18n::t(locale, "annotation", "searching")
                            } else {
                                I18n::t(locale, "annotation", "search")
                            })
                            .primary()
                            .disabled(notes.loading || self.facade.is_none())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| this.run_notes_search(1, cx));
                            })
                    }),
            )
            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "annotation",
                "searchHelp",
            )))
            .when_some(notes.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-notes-error", BannerKind::Error, err).palette(p),
                )
            })
            .when(
                notes.searched && !notes.loading && notes.error.is_none(),
                |el| {
                    if notes.hits.is_empty() {
                        el.child(div().text_body().text_color(p.muted).child(I18n::t(
                            locale,
                            "annotation",
                            "noResults",
                        )))
                    } else {
                        let count_text = I18n::format(
                            locale,
                            "annotation",
                            "resultCount",
                            &[("count", Param::from(notes.hits.len() as i64))],
                        );
                        let ent_page = entity.clone();
                        el.child(
                            div()
                                .v_flex()
                                .gap(px(8.0))
                                .child(div().text_meta().text_color(p.muted).child(count_text))
                                .child(results_box)
                                .when(total_pages > 1, |inner| {
                                    inner.child(
                                        NumberedSearchPagination::new(
                                            "library-notes-pagination",
                                            notes.page,
                                            total_pages,
                                        )
                                        .disabled(notes.loading)
                                        .palette(p)
                                        .on_select(
                                            move |page, _, cx| {
                                                ent_page.update(cx, |this, cx| {
                                                    this.run_notes_search(page, cx);
                                                });
                                            },
                                        ),
                                    )
                                }),
                        )
                    }
                },
            )
    }

    fn render_position_panel(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let active_pos = self.filters.has_position_filter();

        div()
            .id("library-panel-position")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_end()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .w(px(260.0))
                            .child(
                                div()
                                    .text_meta()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child(I18n::t(locale, "app", "app.positionId")),
                            )
                            .child(
                                Input::new(&self.inputs.position_id)
                                    .id("library-position-id-input"),
                            ),
                    )
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("library-position-search-btn")
                            .icon(ActionIconName::Search)
                            .label(I18n::t(locale, "app", "app.exactSearch"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |this, cx| this.submit_position_search_input(cx));
                            })
                    })
                    .when(active_pos, |el| {
                        let ent = entity.clone();
                        el.child(
                            ActionButton::new("library-position-clear-btn")
                                .icon(ActionIconName::Clear)
                                .label(I18n::t(locale, "library", "clearPositionFilter"))
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ent.update(cx, |this, cx| {
                                        this.remove_filter_chip("position_id", Some(window), cx);
                                    });
                                }),
                        )
                    }),
            )
            .when_some(self.position_error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-position-error", BannerKind::Error, err).palette(p),
                )
            })
            .when(
                self.position_error.is_none()
                    && !self.position_draft.trim().is_empty()
                    && !is_valid_position_id(&self.position_draft),
                |el| {
                    el.child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "library",
                        "invalidPositionId",
                    )))
                },
            )
    }

    fn render_duplicates_panel(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let dups = &self.duplicates;

        let summary = I18n::format(
            locale,
            "library",
            "duplicateSummary",
            &[
                ("groups", Param::from(dups.total_groups)),
                ("matches", Param::from(dups.total_matches)),
            ],
        );

        let mut groups_container = div()
            .id("library-duplicates-groups")
            .v_flex()
            .gap(px(8.0))
            .max_h(px(320.0))
            .overflow_y_scroll();

        for (g_idx, group) in dups.groups.iter().enumerate() {
            let kind_badge = match group.kind {
                DuplicateMatchKind::IdenticalFiles => {
                    I18n::t(locale, "library", "duplicateIdenticalFiles")
                }
                DuplicateMatchKind::SameMoves => I18n::t(locale, "library", "duplicateSameMoves"),
            };
            let group_title = I18n::format(
                locale,
                "library",
                "duplicateGroup",
                &[("number", Param::from((g_idx + 1) as i64))],
            );
            let count_text = I18n::format(
                locale,
                "library",
                "duplicateGroupMatches",
                &[("count", Param::from(group.match_count))],
            );

            let mut card = div()
                .id(SharedString::from(format!("library-dup-group-{g_idx}")))
                .v_flex()
                .gap(px(4.0))
                .p(px(8.0))
                .rounded(px(6.0))
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
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(format!("{group_title} · {count_text}")),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(p.selected)
                                .text_meta()
                                .text_color(p.accent)
                                .child(kind_badge),
                        ),
                );

            for (m_idx, item) in group.items.iter().enumerate() {
                let checked = self.selection.contains(item.id);
                let disabled_check = !checked && self.selection.is_at_cap();
                let match_for_check = clone_database_match(item);
                let match_for_open = clone_database_match(item);
                let ent_check = entity.clone();
                let ent_open = entity.clone();
                let db_name = self
                    .databases
                    .iter()
                    .find(|d| d.id == item.database_id)
                    .map(|d| database_display_name(d, locale))
                    .unwrap_or_else(|| "—".to_string());

                card = card.child(
                    div()
                        .id(SharedString::from(format!(
                            "library-dup-item-{g_idx}-{m_idx}"
                        )))
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(if checked { p.selected } else { p.surface })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(8.0))
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "library-dup-check-{g_idx}-{m_idx}"
                                        )))
                                        .h_flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(18.0))
                                        .h(px(18.0))
                                        .rounded(px(4.0))
                                        .border_1()
                                        .border_color(if checked {
                                            p.accent
                                        } else {
                                            p.control_border
                                        })
                                        .bg(if checked { p.accent } else { p.surface })
                                        .text_color(p.accent_text)
                                        .text_meta()
                                        .when(!disabled_check, |el| {
                                            el.cursor_pointer().on_click(move |_, _, cx| {
                                                let m = clone_database_match(&match_for_check);
                                                ent_check.update(cx, |this, cx| {
                                                    this.selection.set_many(&[m], !checked);
                                                    cx.notify();
                                                });
                                            })
                                        })
                                        .child(if checked { "✓" } else { "" }),
                                )
                                .child(
                                    div()
                                        .text_table()
                                        .text_color(p.muted)
                                        .child(format!("#{}", item.match_no)),
                                )
                                .child(
                                    div()
                                        .text_table()
                                        .text_color(p.text)
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .child(format!(
                                            "{} — {} ({})",
                                            item.white, item.black, item.display_name
                                        )),
                                )
                                .child(div().text_meta().text_color(p.muted).child(format!(
                                    "{} · {} · {}",
                                    db_name,
                                    item.format.to_uppercase(),
                                    format_naive_date(item.played_on)
                                ))),
                        )
                        .child(
                            ActionButton::new(SharedString::from(format!(
                                "library-dup-open-{g_idx}-{m_idx}"
                            )))
                            .icon(ActionIconName::Open)
                            .tooltip(I18n::t(locale, "workspace", "openMatch"))
                            .dense_nav()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                let m = clone_database_match(&match_for_open);
                                ent_open.update(cx, |this, cx| {
                                    this.open_library_match(&m, cx);
                                });
                            }),
                        ),
                );
            }
            groups_container = groups_container.child(card);
        }

        div()
            .id("library-panel-duplicates")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .child(div().text_panel().text_color(p.text).child(I18n::t(
                                locale,
                                "library",
                                "duplicateFinder",
                            )))
                            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                locale,
                                "library",
                                "duplicateHelp",
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-duplicates-select-copies-btn")
                                    .icon(ActionIconName::SelectAll)
                                    .label(I18n::t(locale, "library", "selectCopies"))
                                    .tooltip(I18n::t(locale, "library", "selectCopiesHelp"))
                                    .disabled(dups.groups.is_empty())
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.select_duplicate_copies(cx);
                                        });
                                    })
                            })
                            .child({
                                let ent = entity.clone();
                                ActionButton::new("library-duplicates-refresh-btn")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::t(locale, "library", "searchAgain"))
                                    .disabled(dups.loading || self.facade.is_none())
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |this, cx| {
                                            this.load_duplicates(false, cx);
                                        });
                                    })
                            }),
                    ),
            )
            .when_some(dups.error.clone(), |el, err| {
                el.child(
                    StatusBanner::new("library-duplicates-error", BannerKind::Error, err)
                        .palette(p),
                )
            })
            .when(dups.loading, |el| {
                el.child(
                    div()
                        .text_body()
                        .text_color(p.muted)
                        .child(I18n::t(locale, "library", "loading")),
                )
            })
            .when(
                dups.searched && !dups.loading && dups.error.is_none(),
                |el| {
                    if dups.groups.is_empty() {
                        el.child(div().text_body().text_color(p.muted).child(I18n::t(
                            locale,
                            "library",
                            "noDuplicates",
                        )))
                    } else {
                        let has_more = dups.next_cursor.is_some();
                        let ent_more = entity.clone();
                        el.child(
                            div()
                                .v_flex()
                                .gap(px(8.0))
                                .child(div().text_meta().text_color(p.muted).child(summary))
                                .child(groups_container)
                                .when(has_more, |inner| {
                                    inner.child(
                                        ActionButton::new("library-duplicates-load-more-btn")
                                            .icon(ActionIconName::Expand)
                                            .label(I18n::t(
                                                locale,
                                                "library",
                                                "moreDuplicateGroups",
                                            ))
                                            .disabled(dups.loading_more)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_more.update(cx, |this, cx| {
                                                    this.load_duplicates(true, cx);
                                                });
                                            }),
                                    )
                                }),
                        )
                    }
                },
            )
    }
}
