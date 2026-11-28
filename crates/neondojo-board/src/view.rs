//! Complete GPUI `BoardView` (`canvas()` 8-layer painter + interactive
//! click-to-move, hover destination dot, dice-pair click, cube click, move
//! preview arrows, animation clock, and audio cues) plus `BoardGalleryTile` for
//! Settings → Board.

use crate::animation::{ActiveVisual, BoardAnimationState, DieUsage, PlayFrame};
use crate::audio::PlayAudioEngine;
use crate::geometry::{
    ArrowGeometry, BoardGeometry, BoardHitboxes, BoardTransform, BoardViewMode, CheckerMotion,
    DiePlacement, LogicalPoint, LogicalRect, MovePreview, NATIVE_FIELD, NATIVE_HEIGHT,
    NATIVE_OFF_TRAY, NATIVE_POINT_PITCH, NATIVE_WIDTH, NumberOrientation, POINT_COLUMNS,
    PlayOrientation, board_flips, borne_off_counts, checker_center, checker_face_rect, cube_rect,
    decode_match_context, destination_dot_center, dice_placements, interactive_point_rects,
    move_arrow_geometry, number_strip_rects, pip_counts, point_stack_geometry, relative_point,
    tray_counts_for_frame, underlay_board,
};
use crate::themes::{
    BgBlitzThemeEntry, BoardThemeConfig, BuiltInThemeId, ThemeAtlasCache, builtin_gallery_assets,
};
use gpui_kit::{
    App, BorderStyle, Bounds, BoxShadow, Context, Corners, ElementId, EventEmitter, Font,
    FontWeight, Hsla, InteractiveElement, IntoElement, LineCap, LineJoin, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, PathStyle, Pixels,
    Point, Render, SharedString, StrokeOptions, Styled, TestSupportExt, TextAlign, TextRun, Window,
    canvas, div, fill, img, linear_color_stop, linear_gradient, point, px, quad, rgb, rgba, size,
};
use neondojo_rules::backgammon::initial_board;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Board position snapshot rendered by `BoardView`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardPosition {
    pub board: [i32; 26],
    /// Player on roll / active perspective (`0` White, `1` Black).
    pub player: u8,
    pub dice: Vec<u8>,
    pub gnubg_match_id: String,
    pub notation: String,
}

impl Default for BoardPosition {
    fn default() -> Self {
        Self {
            board: initial_board(),
            player: 0,
            dice: Vec::new(),
            gnubg_match_id: "MAAAAAAAAAAA".to_string(),
            notation: String::new(),
        }
    }
}

/// Interactive dice state attached to `BoardInteractionState`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardDiceInteraction {
    pub can_swap: bool,
    pub can_confirm: bool,
    pub can_roll: bool,
    pub can_pass: bool,
    pub used: [DieUsage; 2],
    /// Previous bot roll left faded on the bot's half before human rolls.
    pub previous_roll: Option<(u8, [u8; 2])>,
    /// Opening roll values `[white_die, black_die]` (each `None` while unrolled).
    pub opening: Option<[Option<u8>; 2]>,
    pub rolling: bool,
}

impl BoardDiceInteraction {
    pub fn has_pair_action(&self) -> bool {
        self.opening.is_none()
            && (self.can_roll || self.can_pass || self.can_confirm || self.can_swap)
    }
}

/// Interactive targets for live Play and Study `CheckerPracticeBoard`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardInteractionState {
    pub enabled_points: Vec<i32>,
    pub source_points: Vec<i32>,
    pub destination_points: Vec<i32>,
    pub hovered_source: Option<i32>,
    pub dice: Option<BoardDiceInteraction>,
    pub can_double: bool,
    pub cube_interactive: bool,
}

/// Events emitted by `BoardView` on user pointer interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardEvent {
    PointClicked(i32),
    SourceHovered(Option<i32>),
    DiceClicked,
    CubeClicked,
}

/// Interactive GPU-accelerated Backgammon Board view supporting all 9 built-in
/// themes, 13 BGBlitz ZIP themes, mixed piece artwork, 4 orientations, move
/// preview arrows, live animations, hit-testing, and game audio.
pub struct BoardView {
    id: ElementId,
    position: BoardPosition,
    white_name: String,
    black_name: String,
    theme_config: BoardThemeConfig,
    play_orientation: Option<PlayOrientation>,
    preview: Option<MovePreview>,
    view_mode: BoardViewMode,
    cube_offer_pending: bool,
    show_pip_counts: bool,
    show_player_bars: bool,
    show_both_dice_colors: bool,
    review_outline: Option<Hsla>,
    interaction: Option<BoardInteractionState>,
    animation: BoardAnimationState,
    audio: Option<PlayAudioEngine>,
    atlas: Arc<Mutex<ThemeAtlasCache>>,
    last_hitboxes: Arc<Mutex<BoardHitboxes>>,
    hover_dice: bool,
    hover_cube: bool,
    pressed_dice: bool,
    pressed_cube: bool,
}

impl EventEmitter<BoardEvent> for BoardView {}

impl BoardView {
    pub fn new(id: impl Into<ElementId>, atlas: Arc<Mutex<ThemeAtlasCache>>) -> Self {
        Self {
            id: id.into(),
            position: BoardPosition::default(),
            white_name: "White".to_string(),
            black_name: "Black".to_string(),
            theme_config: BoardThemeConfig::default(),
            play_orientation: None,
            preview: None,
            view_mode: BoardViewMode::Before,
            cube_offer_pending: false,
            show_pip_counts: true,
            show_player_bars: true,
            show_both_dice_colors: false,
            review_outline: None,
            interaction: None,
            animation: BoardAnimationState::default(),
            audio: None,
            atlas,
            last_hitboxes: Arc::new(Mutex::new(BoardHitboxes::default())),
            hover_dice: false,
            hover_cube: false,
            pressed_dice: false,
            pressed_cube: false,
        }
    }

    pub fn set_position(&mut self, position: BoardPosition, cx: &mut Context<Self>) {
        self.position = position;
        cx.notify();
    }

    pub fn set_players(
        &mut self,
        white: impl Into<String>,
        black: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.white_name = white.into();
        self.black_name = black.into();
        cx.notify();
    }

    pub fn set_theme_config(&mut self, config: BoardThemeConfig, cx: &mut Context<Self>) {
        self.theme_config = config;
        cx.notify();
    }

    pub fn set_play_orientation(
        &mut self,
        orientation: Option<PlayOrientation>,
        cx: &mut Context<Self>,
    ) {
        self.play_orientation = orientation;
        cx.notify();
    }

    pub fn set_preview(
        &mut self,
        preview: Option<MovePreview>,
        mode: BoardViewMode,
        cx: &mut Context<Self>,
    ) {
        self.preview = preview;
        self.view_mode = mode;
        cx.notify();
    }

    pub fn set_cube_offer_pending(&mut self, pending: bool, cx: &mut Context<Self>) {
        self.cube_offer_pending = pending;
        cx.notify();
    }

    pub fn set_show_pip_counts(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_pip_counts = show;
        cx.notify();
    }

    pub fn set_show_player_bars(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_player_bars = show;
        cx.notify();
    }

    pub fn show_player_bars(&self) -> bool {
        self.show_player_bars
    }

    pub fn set_show_both_dice_colors(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_both_dice_colors = show;
        cx.notify();
    }

    pub fn set_review_outline(&mut self, color: Option<Hsla>, cx: &mut Context<Self>) {
        self.review_outline = color;
        cx.notify();
    }

    pub fn set_interaction(
        &mut self,
        interaction: Option<BoardInteractionState>,
        cx: &mut Context<Self>,
    ) {
        self.interaction = interaction;
        cx.notify();
    }

    pub fn set_audio(&mut self, audio: Option<PlayAudioEngine>) {
        self.audio = audio;
    }

    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.animation.set_reduced_motion(reduced);
    }

    pub fn enqueue_animation(
        &mut self,
        frames: impl IntoIterator<Item = PlayFrame>,
        cx: &mut Context<Self>,
    ) {
        self.animation.enqueue(frames, Instant::now());
        cx.notify();
    }

    pub fn cancel_animation(&mut self, cx: &mut Context<Self>) {
        self.animation.cancel();
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        cx.notify();
    }

    pub fn is_animating(&self) -> bool {
        self.animation.is_active()
    }

    /// Returns a snapshot of the most recently painted screen-space hitboxes
    /// (also used in headless `#[gpui_kit::test]` verification).
    pub fn last_hitboxes(&self) -> BoardHitboxes {
        self.last_hitboxes
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hitboxes = self.last_hitboxes();
        let pos = event.position;
        let next_dice = self
            .interaction
            .as_ref()
            .and_then(|i| i.dice.as_ref())
            .is_some_and(BoardDiceInteraction::has_pair_action)
            && hitboxes.hit_test_dice(pos);
        let next_cube = self
            .interaction
            .as_ref()
            .is_some_and(|i| i.cube_interactive && i.can_double)
            && hitboxes.hit_test_cube(pos);

        let mut changed = false;
        if self.hover_dice != next_dice {
            self.hover_dice = next_dice;
            changed = true;
        }
        if self.hover_cube != next_cube {
            self.hover_cube = next_cube;
            changed = true;
        }

        if let Some(inter) = self.interaction.as_mut() {
            let hovered = hitboxes.hit_test_source_hover(pos);
            if inter.hovered_source != hovered {
                inter.hovered_source = hovered;
                cx.emit(BoardEvent::SourceHovered(hovered));
                changed = true;
            }
        }

        if changed {
            cx.notify();
        }
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let hitboxes = self.last_hitboxes();
        let pos = event.position;

        if let Some(inter) = &self.interaction {
            if inter.cube_interactive && inter.can_double && hitboxes.hit_test_cube(pos) {
                self.pressed_cube = true;
                cx.emit(BoardEvent::CubeClicked);
                cx.notify();
                return;
            }
            if inter
                .dice
                .as_ref()
                .is_some_and(BoardDiceInteraction::has_pair_action)
                && hitboxes.hit_test_dice(pos)
            {
                self.pressed_dice = true;
                cx.emit(BoardEvent::DiceClicked);
                cx.notify();
                return;
            }
            if let Some(pt) = hitboxes.hit_test_point(pos)
                && inter.enabled_points.contains(&pt)
            {
                cx.emit(BoardEvent::PointClicked(pt));
                cx.notify();
            }
        }
    }

    fn handle_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pressed_dice || self.pressed_cube {
            self.pressed_dice = false;
            self.pressed_cube = false;
            cx.notify();
        }
    }
}

struct PaintSnapshot {
    geom: BoardGeometry,
    board_theme_builtin: BuiltInThemeId,
    checker_theme_id: String,
    cube_theme_id: String,
    dice_theme_id: String,
    board_theme_id: String,
    flip_x: f32,
    flip_y: f32,
    number_player: u8,
    number_orientation: NumberOrientation,
    static_board: [i32; 26],
    tray_counts: [i32; 2],
    cube_value: i32,
    cube_owner: Option<u8>,
    cube_pending: bool,
    dice_entries: Vec<(u8, Option<u8>)>,
    dice_split: bool,
    side_by_owner: bool,
    dice_usage: [DieUsage; 2],
    dice_tumble_offset: (f32, f32),
    dice_opacity: f32,
    dice_hover: bool,
    dice_pressed: bool,
    cube_hover: bool,
    cube_pressed: bool,
    destinations: Vec<i32>,
    source_points: Vec<i32>,
    motions: Vec<CheckerMotion>,
    motion_before: [i32; 26],
    motion_after: [i32; 26],
    motion_progress: f32,
    motion_opacity: f32,
    arrows: Vec<ArrowGeometry>,
    atlas: Arc<Mutex<ThemeAtlasCache>>,
    last_hitboxes: Arc<Mutex<BoardHitboxes>>,
}

impl Render for BoardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let tick = self.animation.sample(now);
        if !tick.sounds.is_empty()
            && let Some(audio) = &self.audio
        {
            audio.play(&tick.sounds);
        }
        if tick.needs_next_frame {
            window.request_animation_frame();
        }

        let effective_mode = if self.preview.as_ref().is_some_and(MovePreview::is_ok) {
            self.view_mode
        } else {
            BoardViewMode::Before
        };

        // Resolve displayed position (Before / Arrows / After).
        let (displayed_board, displayed_dice) = match (effective_mode, &self.preview) {
            (BoardViewMode::After, Some(MovePreview::Ok { board, .. })) => (*board, Vec::new()),
            _ => (self.position.board, self.position.dice.clone()),
        };

        let (
            static_board,
            tray_counts,
            motions,
            motion_before,
            motion_after,
            motion_progress,
            motion_opacity,
            anim_dice,
            anim_player,
            anim_opening,
            dice_tumble_offset,
            dice_opacity,
        ) = match &tick.visual {
            Some(ActiveVisual::Checker {
                before,
                after,
                player,
                dice,
                motions,
                progress,
                opacity,
            }) => (
                underlay_board(before, motions),
                tray_counts_for_frame(before, motions),
                motions.clone(),
                *before,
                *after,
                *progress,
                *opacity,
                Some(dice.clone()),
                Some(*player),
                false,
                (0.0, 0.0),
                1.0,
            ),
            Some(ActiveVisual::Dice {
                before,
                player,
                dice,
                opening,
                tumble_offset,
                opacity,
                ..
            }) => (
                *before,
                borne_off_counts(before),
                Vec::new(),
                *before,
                *before,
                1.0,
                1.0,
                Some(dice.clone()),
                Some(*player),
                *opening,
                *tumble_offset,
                *opacity,
            ),
            None => (
                displayed_board,
                borne_off_counts(&displayed_board),
                Vec::new(),
                displayed_board,
                displayed_board,
                1.0,
                1.0,
                self.animation.pending_roll_faces(now).map(|f| f.to_vec()),
                None,
                false,
                (0.0, 0.0),
                1.0,
            ),
        };

        let number_player = self
            .play_orientation
            .map_or(self.position.player, |o| o.human);
        let (flip_x, flip_y) = self
            .play_orientation
            .map_or((1.0, 1.0), |o| board_flips(o.human, o.direction));
        let number_orientation = NumberOrientation::from_orientation(flip_x, number_player);

        let context = decode_match_context(&self.position.gnubg_match_id);
        let cube_pending = context.doubled || self.cube_offer_pending;
        let cube_value = context.cube * if cube_pending { 2 } else { 1 };
        let cube_owner = if cube_pending { None } else { context.owner };

        let dice_inter = self.interaction.as_ref().and_then(|i| i.dice.as_ref());
        let opening = if anim_opening {
            anim_dice
                .as_ref()
                .map(|d| [d.first().copied(), d.get(1).copied()])
        } else {
            dice_inter.and_then(|d| d.opening)
        };
        let dice_split = opening.is_some() || self.show_both_dice_colors;
        let (visible_dice, dice_owner) = if let Some(d) = &anim_dice {
            (d.clone(), anim_player.unwrap_or(self.position.player))
        } else if let Some((prev_player, prev_dice)) = dice_inter.and_then(|d| d.previous_roll) {
            (prev_dice.to_vec(), prev_player)
        } else {
            (displayed_dice, self.position.player)
        };

        let dice_entries: Vec<(u8, Option<u8>)> = if let Some(op) = opening {
            vec![(0, op[0]), (1, op[1])]
        } else {
            visible_dice
                .iter()
                .copied()
                .filter(|&v| (1..=6).contains(&v))
                .enumerate()
                .map(|(i, v)| {
                    (
                        if self.show_both_dice_colors {
                            (i % 2) as u8
                        } else {
                            dice_owner
                        },
                        Some(v),
                    )
                })
                .collect()
        };

        let dice_usage = if tick.visual.is_some() {
            [DieUsage::Unused, DieUsage::Unused]
        } else if dice_inter.and_then(|d| d.previous_roll).is_some() {
            [DieUsage::Used, DieUsage::Used]
        } else {
            dice_inter.map_or([DieUsage::Unused, DieUsage::Unused], |d| d.used)
        };

        // Resolve geometry and flush any previously evicted GPU textures.
        let geom = {
            let mut atlas = self.atlas.lock().expect("atlas lock");
            atlas.flush_evictions(Some(window), cx);
            atlas.geometry_for(&self.theme_config.board_theme)
        };

        let arrows = if effective_mode == BoardViewMode::Arrows
            && let Some(MovePreview::Ok { steps, .. }) = &self.preview
        {
            move_arrow_geometry(&self.position.board, self.position.player, steps, &geom)
        } else {
            Vec::new()
        };

        let board_theme_builtin =
            BuiltInThemeId::parse(&self.theme_config.board_theme).unwrap_or(BuiltInThemeId::Forest);
        let destinations = self
            .interaction
            .as_ref()
            .map_or_else(Vec::new, |i| i.destination_points.clone());
        let source_points = self
            .interaction
            .as_ref()
            .map_or_else(Vec::new, |i| i.source_points.clone());

        let snapshot = PaintSnapshot {
            geom,
            board_theme_builtin,
            board_theme_id: self.theme_config.board_theme.clone(),
            checker_theme_id: self.theme_config.resolved_checker_id().to_string(),
            cube_theme_id: self.theme_config.resolved_cube_id().to_string(),
            dice_theme_id: self.theme_config.resolved_dice_id().to_string(),
            flip_x,
            flip_y,
            number_player,
            number_orientation,
            static_board,
            tray_counts,
            cube_value,
            cube_owner,
            cube_pending,
            dice_entries,
            dice_split,
            side_by_owner: self.play_orientation.is_some(),
            dice_usage,
            dice_tumble_offset,
            dice_opacity,
            dice_hover: self.hover_dice,
            dice_pressed: self.pressed_dice,
            cube_hover: self.hover_cube,
            cube_pressed: self.pressed_cube,
            destinations,
            source_points,
            motions,
            motion_before,
            motion_after,
            motion_progress,
            motion_opacity,
            arrows,
            atlas: Arc::clone(&self.atlas),
            last_hitboxes: Arc::clone(&self.last_hitboxes),
        };

        let pips = pip_counts(&displayed_board);
        let off = borne_off_counts(&displayed_board);
        let human_is_black = self.play_orientation.is_some_and(|o| o.human == 1);

        let top_player_idx: usize = if human_is_black { 0 } else { 1 };
        let bottom_player_idx: usize = 1 - top_player_idx;
        let player_names = [&self.white_name, &self.black_name];
        let checker_palette = BuiltInThemeId::parse(self.theme_config.resolved_checker_id())
            .unwrap_or(BuiltInThemeId::Forest)
            .palette();
        let (white_bar_sprite, black_bar_sprite) = {
            let checker_id = self.theme_config.resolved_checker_id();
            let mut atlas = self.atlas.lock().expect("atlas lock");
            if let Some(bg) = atlas.get_or_load_bgblitz(checker_id) {
                (
                    bg.white_checkers.first().map(|(img, _)| Arc::clone(img)),
                    bg.black_checkers.first().map(|(img, _)| Arc::clone(img)),
                )
            } else if let Some(builtin) = BuiltInThemeId::parse(checker_id) {
                (
                    atlas
                        .builtin_checker_pair(builtin, true)
                        .map(|pair| Arc::clone(&pair[0])),
                    atlas
                        .builtin_checker_pair(builtin, false)
                        .map(|pair| Arc::clone(&pair[0])),
                )
            } else {
                (None, None)
            }
        };

        let render_player_bar = |player_idx: usize| {
            let dot_color = if player_idx == 0 {
                rgb(checker_palette.white_checker)
            } else {
                rgb(checker_palette.black_checker)
            };
            let marker_sprite = if player_idx == 0 {
                white_bar_sprite.clone()
            } else {
                black_bar_sprite.clone()
            };
            let crawford = if player_idx == 0 && context.crawford {
                " · Crawford"
            } else {
                ""
            };
            let left_label = format!(
                "{} · {}{}",
                player_names[player_idx], context.scores[player_idx], crawford
            );
            let right_label = if self.show_pip_counts {
                format!("{} pips · {} off", pips[player_idx], off[player_idx])
            } else {
                String::new()
            };
            let marker = if let Some(sprite) = marker_sprite {
                div()
                    .w(px(15.0))
                    .h(px(15.0))
                    .flex_shrink_0()
                    .child(img(sprite).size_full())
            } else {
                div()
                    .w(px(10.0))
                    .h(px(10.0))
                    .rounded_full()
                    .bg(dot_color)
                    .border_1()
                    .border_color(rgb(0xacb9a2))
                    .flex_shrink_0()
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .py(px(2.0))
                .text_xs()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(marker)
                        .child(left_label),
                )
                .child(right_label)
        };

        let board_canvas = canvas(
            move |bounds, _window, _cx| {
                let (lw, lh) = snapshot.geom.dimensions();
                let transform =
                    BoardTransform::fit(bounds, lw, lh, snapshot.flip_x, snapshot.flip_y);
                let dice = dice_placements(
                    &snapshot.geom,
                    &snapshot.dice_entries,
                    snapshot.side_by_owner,
                    snapshot.dice_split,
                );
                let cube_logical = {
                    let mut atlas = snapshot.atlas.lock().expect("atlas lock");
                    let bgblitz_cube_dims = atlas
                        .get_or_load_bgblitz(&snapshot.board_theme_id)
                        .and_then(|t| {
                            t.cube_sprite(
                                snapshot.cube_value,
                                snapshot.cube_owner,
                                snapshot.cube_pending,
                            )
                        })
                        .map(|(_, dims)| *dims);
                    Some(cube_rect(
                        &snapshot.geom,
                        snapshot.cube_owner,
                        bgblitz_cube_dims,
                    ))
                };
                let hitboxes = BoardHitboxes::compute(
                    &snapshot.geom,
                    &transform,
                    &snapshot.static_board,
                    &snapshot.source_points,
                    &dice,
                    cube_logical,
                );
                if let Ok(mut guard) = snapshot.last_hitboxes.lock() {
                    *guard = hitboxes.clone();
                }
                (snapshot, transform, dice, cube_logical)
            },
            move |_bounds, (snapshot, transform, dice, cube_logical), window, cx| {
                paint_board(&snapshot, &transform, &dice, cube_logical, window, cx);
            },
        )
        .absolute()
        .inset_0()
        .size_full();

        let mut canvas_wrapper = div()
            .id(self.id.clone())
            .relative()
            .flex_1()
            .w_full()
            .min_h(px(160.0))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .child(board_canvas);

        if let Some(outline_color) = self.review_outline {
            canvas_wrapper = canvas_wrapper
                .border_2()
                .border_color(outline_color)
                .rounded(px(4.0));
        }

        let mut root = div().flex().flex_col().size_full().gap(px(4.0));
        if self.show_player_bars {
            root = root.child(render_player_bar(top_player_idx));
        }
        root = root.child(canvas_wrapper.test_support());
        if self.show_player_bars {
            root = root.child(render_player_bar(bottom_player_idx));
        }
        root
    }
}

/// 8-layer `canvas()` paint pipeline for both built-in and BGBlitz boards.
fn paint_board(
    s: &PaintSnapshot,
    transform: &BoardTransform,
    dice: &[DiePlacement],
    cube_logical: Option<LogicalRect>,
    window: &mut Window,
    cx: &mut App,
) {
    let mut atlas = s.atlas.lock().expect("atlas lock");
    let scale_factor = window.scale_factor();

    // Layer 1 & 2: Board frame, surface/background, bar, off-tray, points, point numbers.
    match &s.geom {
        BoardGeometry::BgBlitz(metrics) => {
            if let Some(bg_theme) = atlas.get_or_load_bgblitz(&s.board_theme_id) {
                let bg_img = bg_theme.background_for_flip(s.flip_x < 0.0, s.flip_y < 0.0);
                let board_bounds = transform.board_bounds();
                let _ = window.paint_image(
                    board_bounds,
                    board_bounds,
                    Corners::all(px(5.0)),
                    bg_img,
                    0,
                    false,
                );
                if let Some(strips) = bg_theme.numbers.get(&s.number_orientation) {
                    let strip_rects = number_strip_rects(
                        metrics,
                        s.number_orientation,
                        [strips[0].1, strips[1].1],
                    );
                    for (idx, rect) in strip_rects.into_iter().enumerate() {
                        let b = transform.map_rect(rect);
                        let _ = window.paint_image(
                            b,
                            b,
                            Corners::default(),
                            Arc::clone(&strips[idx].0),
                            0,
                            false,
                        );
                    }
                }
            }
        }
        BoardGeometry::BuiltIn => {
            let pal = s.board_theme_builtin.palette();
            let frame_bounds = transform.map_rect(LogicalRect::new(1.0, 1.0, 678.0, 468.0));
            let frame_bg = if pal.inlay.is_some() {
                linear_gradient(
                    180.0,
                    linear_color_stop(pal.rail_highlight(), 0.0),
                    linear_color_stop(pal.rail_shadow(), 1.0),
                )
            } else {
                rgb(pal.frame).into()
            };
            window.paint_quad(quad(
                frame_bounds,
                Corners::all(transform.scale_len(14.0)),
                frame_bg,
                px(0.0),
                Hsla::transparent_black(),
                BorderStyle::Solid,
            ));

            if let Some(inlay_hex) = pal.inlay {
                let inlay_bounds = transform.map_rect(LogicalRect::new(5.0, 5.0, 670.0, 460.0));
                let mut inlay_color: Hsla = rgb(inlay_hex).into();
                inlay_color.a = 0.58;
                window.paint_quad(quad(
                    inlay_bounds,
                    Corners::all(transform.scale_len(10.0)),
                    Hsla::transparent_black(),
                    px(1.0),
                    inlay_color,
                    BorderStyle::Solid,
                ));
            }

            let field_bounds = transform.map_rect(NATIVE_FIELD);
            window.paint_quad(quad(
                field_bounds,
                Corners::all(transform.scale_len(3.0)),
                rgb(pal.felt),
                px(0.0),
                Hsla::transparent_black(),
                BorderStyle::Solid,
            ));

            if s.board_theme_builtin.surface_svg().is_some() {
                let pw = (f32::from(field_bounds.size.width) * scale_factor).ceil() as u32;
                let ph = (f32::from(field_bounds.size.height) * scale_factor).ceil() as u32;
                if let Some(surf) = atlas.builtin_surface(s.board_theme_builtin, pw, ph) {
                    let _ = window.paint_image(
                        field_bounds,
                        field_bounds,
                        Corners::all(transform.scale_len(3.0)),
                        surf,
                        0,
                        false,
                    );
                }
            }

            // Central bar and right bear-off tray.
            let bar_rect = LogicalRect::new(
                NATIVE_WIDTH / 2.0 - NATIVE_POINT_PITCH / 2.0,
                NATIVE_FIELD.y,
                NATIVE_POINT_PITCH,
                NATIVE_FIELD.height,
            );
            let bar_border = if pal.inlay.is_some() {
                pal.rail_shadow()
            } else {
                Hsla::transparent_black()
            };
            let bar_border_w = if pal.inlay.is_some() {
                transform.scale_len(1.5)
            } else {
                px(0.0)
            };
            for r in [bar_rect, NATIVE_OFF_TRAY] {
                window.paint_quad(quad(
                    transform.map_rect(r),
                    Corners::all(transform.scale_len(3.0)),
                    rgb(pal.bar),
                    bar_border_w,
                    bar_border,
                    BorderStyle::Solid,
                ));
            }

            // 24 alternating triangles and point numbers.
            let number_color: Hsla = rgb(pal.point_number).into();
            let font_size = transform.scale_len(12.0);
            for i in 0..12usize {
                let top_pt = 13 + i as i32;
                let bot_pt = 12 - i as i32;
                let center = BoardGeometry::native_point_x(i);
                let left = center - NATIVE_POINT_PITCH / 2.0;
                let right = center + NATIVE_POINT_PITCH / 2.0;

                let top_color: Hsla = rgb(if i % 2 == 1 {
                    pal.point_light
                } else {
                    pal.point_dark
                })
                .into();
                let bot_color: Hsla = rgb(if i % 2 == 1 {
                    pal.point_dark
                } else {
                    pal.point_light
                })
                .into();

                paint_triangle(
                    window,
                    transform.map_point(LogicalPoint::new(left, NATIVE_FIELD.y)),
                    transform.map_point(LogicalPoint::new(right, NATIVE_FIELD.y)),
                    transform.map_point(LogicalPoint::new(center, 214.0)),
                    top_color,
                );
                paint_triangle(
                    window,
                    transform.map_point(LogicalPoint::new(left, NATIVE_FIELD.bottom())),
                    transform.map_point(LogicalPoint::new(right, NATIVE_FIELD.bottom())),
                    transform.map_point(LogicalPoint::new(center, 256.0)),
                    bot_color,
                );

                paint_centered_text(
                    window,
                    cx,
                    &relative_point(top_pt, s.number_player).to_string(),
                    transform.map_point(LogicalPoint::new(center, 17.0)),
                    font_size,
                    FontWeight::NORMAL,
                    number_color,
                );
                paint_centered_text(
                    window,
                    cx,
                    &relative_point(bot_pt, s.number_player).to_string(),
                    transform.map_point(LogicalPoint::new(center, 453.0)),
                    font_size,
                    FontWeight::NORMAL,
                    number_color,
                );
            }
        }
    }

    // Layer 3: Static checkers on 24 points, 2 bars, and 2 bear-off trays + >5 badges.
    let mut stacks_to_draw: Vec<(i32, i32)> = POINT_COLUMNS
        .iter()
        .map(|pc| (pc.point, s.static_board[pc.point as usize]))
        .collect();
    stacks_to_draw.push((0, s.static_board[0]));
    stacks_to_draw.push((25, s.static_board[25]));
    if s.tray_counts[1] > 0 {
        stacks_to_draw.push((26, -s.tray_counts[1]));
    }
    if s.tray_counts[0] > 0 {
        stacks_to_draw.push((-1, s.tray_counts[0]));
    }

    for (pt, count) in stacks_to_draw {
        if count == 0 {
            continue;
        }
        let stack = point_stack_geometry(pt, count, &s.geom);
        let white = count > 0;
        let variant_base = if s.geom.is_bgblitz() {
            stack.column
        } else {
            pt.unsigned_abs() as usize
        };
        for (i, slot) in stack.checker_rects().into_iter().enumerate() {
            paint_checker_piece(
                window,
                &mut atlas,
                &s.checker_theme_id,
                s.board_theme_builtin,
                transform,
                slot,
                white,
                variant_base + i,
                1.0,
            );
        }
        if let Some(badge_center) = stack.badge_center() {
            paint_stack_badge(
                window,
                cx,
                &s.geom,
                s.board_theme_builtin,
                transform,
                badge_center,
                stack.width,
                stack.total,
                white,
            );
        }
    }

    // Layer 4: Interactive destination dots & built-in bear-off indicators.
    if !s.destinations.is_empty() || !s.geom.is_bgblitz() {
        let (_, board_h) = s.geom.dimensions();
        for (pt, rect) in interactive_point_rects(&s.geom) {
            if s.destinations.contains(&pt) {
                let bottom = pt == -1 || pt == 25 || (1..=12).contains(&pt);
                let dot_center = destination_dot_center(rect, bottom);
                let dot_r = board_h * 0.009;
                paint_circle(
                    window,
                    transform.map_point(dot_center),
                    transform.scale_len(dot_r),
                    rgb(0xf1d58d).into(),
                    Some((rgb(0x26352d).into(), transform.scale_len(1.2))),
                );
            }
        }
    }

    // Layer 5: Doubling cube (with hover lift + shadow + active press).
    if let Some(cube_rect_logical) = cube_logical {
        let lift_y = if s.cube_pressed {
            1.0
        } else if s.cube_hover {
            -3.0
        } else {
            0.0
        };
        let scale_mul = if s.cube_pressed {
            0.97
        } else if s.cube_hover {
            1.04
        } else {
            1.0
        };
        let mut bounds = transform.map_rect(cube_rect_logical);
        bounds = scale_bounds_about_center(bounds, scale_mul);
        bounds.origin.y += px(lift_y);

        if s.cube_hover && !s.cube_pressed {
            window.paint_drop_shadows(
                bounds,
                Corners::all(px(4.0)),
                &[BoxShadow {
                    color: rgba(0x00000073).into(),
                    offset: point(px(0.0), px(4.0)),
                    blur_radius: px(2.0),
                    spread_radius: px(0.0),
                    inset: false,
                }],
            );
        }

        let bgblitz_face = atlas
            .get_or_load_bgblitz(&s.cube_theme_id)
            .and_then(|t| t.cube_sprite(s.cube_value, s.cube_owner, s.cube_pending))
            .map(|(img, _)| Arc::clone(img));

        if let Some(face_img) = bgblitz_face {
            let _ = window.paint_image(bounds, bounds, Corners::default(), face_img, 0, false);
        } else {
            let cube_builtin =
                BuiltInThemeId::parse(&s.cube_theme_id).unwrap_or(BuiltInThemeId::Forest);
            if let Some(body) = atlas.builtin_cube(cube_builtin) {
                let _ = window.paint_image(bounds, bounds, Corners::default(), body, 0, false);
            }
            let numeral = if s.cube_value == 1 {
                "64".to_string()
            } else {
                s.cube_value.to_string()
            };
            let font_px = f32::from(bounds.size.height)
                * (62.0_f32.min(144.0 / numeral.len().max(1) as f32) / 100.0);
            paint_centered_text(
                window,
                cx,
                &numeral,
                bounds.center(),
                px(font_px),
                FontWeight::SEMIBOLD,
                rgb(cube_builtin.palette().cube_numeral).into(),
            );
        }
    }

    // Layer 6: Dice pair / opening split dice (with half-used double masking & hover lift).
    let lift_y = if s.dice_pressed {
        1.0
    } else if s.dice_hover {
        -3.0
    } else {
        0.0
    };
    let dice_scale_mul = if s.dice_pressed {
        0.97
    } else if s.dice_hover {
        1.04
    } else {
        1.0
    };
    for die in dice {
        let usage = s
            .dice_usage
            .get(die.index)
            .copied()
            .unwrap_or(DieUsage::Unused);
        let mut rect = die.rect;
        rect.x += s.dice_tumble_offset.0;
        rect.y += s.dice_tumble_offset.1;
        let mut bounds = transform.map_rect(rect);
        bounds = scale_bounds_about_center(bounds, dice_scale_mul);
        bounds.origin.y += px(lift_y);

        if s.dice_hover && !s.dice_pressed {
            window.paint_drop_shadows(
                bounds,
                Corners::all(px(5.0)),
                &[BoxShadow {
                    color: rgba(0x00000073).into(),
                    offset: point(px(0.0), px(4.0)),
                    blur_radius: px(2.0),
                    spread_radius: px(0.0),
                    inset: false,
                }],
            );
        }

        let face_idx = die.value.unwrap_or(1).clamp(1, 6) as usize - 1;
        let bgblitz_die = atlas.get_or_load_bgblitz(&s.dice_theme_id).map(|t| {
            let pair = if die.owner == 0 {
                &t.white_dice[face_idx]
            } else {
                &t.black_dice[face_idx]
            };
            (Arc::clone(&pair.0), Arc::clone(&pair.1))
        });

        if let Some((full_img, dim_img)) = bgblitz_die {
            match usage {
                DieUsage::Unused => {
                    let img_to_draw = if die.value.is_none() {
                        dim_img
                    } else {
                        full_img
                    };
                    let _ = window.paint_image(
                        bounds,
                        bounds,
                        Corners::default(),
                        img_to_draw,
                        0,
                        false,
                    );
                }
                DieUsage::Used => {
                    let _ =
                        window.paint_image(bounds, bounds, Corners::default(), dim_img, 0, false);
                }
                DieUsage::Half => {
                    // Exact half-die mask: left 50% at 0.28 alpha, right 50% at 1.0 alpha.
                    let half_w = bounds.size.width / 2.0;
                    let left_clip = Bounds {
                        origin: bounds.origin,
                        size: size(half_w, bounds.size.height),
                    };
                    let right_clip = Bounds {
                        origin: point(bounds.origin.x + half_w, bounds.origin.y),
                        size: size(bounds.size.width - half_w, bounds.size.height),
                    };
                    let _ = window.paint_image(
                        left_clip,
                        bounds,
                        Corners::default(),
                        dim_img,
                        0,
                        false,
                    );
                    let _ = window.paint_image(
                        right_clip,
                        bounds,
                        Corners::default(),
                        full_img,
                        0,
                        false,
                    );
                }
            }
        } else {
            let die_builtin =
                BuiltInThemeId::parse(&s.dice_theme_id).unwrap_or(BuiltInThemeId::Forest);
            let base_alpha = if die.value.is_none() {
                0.25
            } else {
                s.dice_opacity
            };
            paint_themed_die(
                window,
                bounds,
                die_builtin,
                die.owner == 0,
                die.value,
                usage,
                base_alpha,
                s.dice_hover,
            );
        }
    }

    // Layer 7: In-flight animated checkers (`PlayCheckerMotion`).
    for motion in &s.motions {
        let start = checker_center(&s.motion_before, motion.from, motion.player, &s.geom);
        let end = checker_center(&s.motion_after, motion.to, motion.player, &s.geom);
        let cx_pos = start.x + (end.x - start.x) * s.motion_progress;
        let cy_pos = start.y + (end.y - start.y) * s.motion_progress;
        let stack = point_stack_geometry(
            motion.from,
            if motion.player == 0 { 1 } else { -1 },
            &s.geom,
        );
        let slot = LogicalRect::new(
            cx_pos - stack.width / 2.0,
            cy_pos - stack.height / 2.0,
            stack.width,
            stack.height,
        );
        let src_count = if (0..=25).contains(&motion.from) {
            s.motion_before[motion.from as usize].unsigned_abs() as usize
        } else {
            1
        };
        let top_idx = src_count.min(5).saturating_sub(1);
        let variant = if s.geom.is_bgblitz() {
            stack.column
        } else {
            motion.from.unsigned_abs() as usize
        } + top_idx;

        paint_checker_piece(
            window,
            &mut atlas,
            &s.checker_theme_id,
            s.board_theme_builtin,
            transform,
            slot,
            motion.player == 0,
            variant,
            s.motion_opacity,
        );
    }

    // Layer 8: Move preview quadratic Bézier arrows (`MoveArrows.tsx`).
    if !s.arrows.is_empty() {
        let arrow_scale = s.geom.arrow_scale() * transform.scale;
        let dark: Hsla = rgb(0x10291c).into();
        let light: Hsla = rgb(0xfffde8).into();
        let green: Hsla = rgb(0x28be68).into();
        for arrow in &s.arrows {
            let p0 = transform.map_point(arrow.start);
            let ctrl = transform.map_point(arrow.control);
            let p1 = transform.map_point(arrow.end);
            let hl = transform.map_point(arrow.head_left);
            let hr = transform.map_point(arrow.head_right);

            for (color, width_factor) in [(dark, 10.0), (light, 7.0), (green, 4.0)] {
                let mut builder = PathBuilder::stroke(px(width_factor * arrow_scale)).with_style(
                    PathStyle::Stroke(StrokeOptions {
                        line_width: width_factor * arrow_scale,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        miter_limit: 4.0,
                    }),
                );
                builder.move_to(p0);
                builder.curve_to(p1, ctrl);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
                }
            }

            for (color, width_factor) in [(dark, 6.0), (light, 3.0), (green, 1.0)] {
                let mut head_stroke = PathBuilder::stroke(px(width_factor * arrow_scale))
                    .with_style(PathStyle::Stroke(StrokeOptions {
                        line_width: width_factor * arrow_scale,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        miter_limit: 4.0,
                    }));
                head_stroke.move_to(hl);
                head_stroke.line_to(p1);
                head_stroke.line_to(hr);
                head_stroke.close();
                if let Ok(path) = head_stroke.build() {
                    window.paint_path(path, color);
                }
            }
            paint_triangle(window, hl, p1, hr, green);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_checker_piece(
    window: &mut Window,
    atlas: &mut ThemeAtlasCache,
    checker_theme_id: &str,
    board_builtin: BuiltInThemeId,
    transform: &BoardTransform,
    slot: LogicalRect,
    white: bool,
    variant: usize,
    opacity: f32,
) {
    if let Some(bg_theme) = atlas.get_or_load_bgblitz(checker_theme_id) {
        let list = if white {
            &bg_theme.white_checkers
        } else {
            &bg_theme.black_checkers
        };
        if !list.is_empty() {
            let (img, (iw, ih)) = &list[variant % list.len()];
            let face = checker_face_rect(slot, *iw, *ih);
            let b = transform.map_rect(face);
            let _ = window.paint_image(b, b, Corners::default(), Arc::clone(img), 0, false);
            return;
        }
    }

    let checker_builtin = BuiltInThemeId::parse(checker_theme_id).unwrap_or(BuiltInThemeId::Forest);
    if let Some(pair) = atlas.builtin_checker_pair(checker_builtin, white) {
        let face = checker_face_rect(slot, 192.0, 192.0);
        let b = transform.map_rect(face);
        let _ = window.paint_image(
            b,
            b,
            Corners::default(),
            Arc::clone(&pair[variant % 2]),
            0,
            false,
        );
        return;
    }

    // Flat built-in vector checker (forest / walnut / slate).
    let pal = checker_builtin.palette();
    let board_pal = board_builtin.palette();
    let mut fill_col: Hsla = rgb(if white {
        pal.white_checker
    } else {
        pal.black_checker
    })
    .into();
    fill_col.a *= opacity;

    let mut stroke_col: Hsla = if let Some(inlay) = board_pal.inlay {
        crate::themes::srgb_mix(board_pal.black_checker, inlay, 0.48)
    } else if white {
        rgb(0xb4b4a0).into()
    } else {
        rgb(0x1c362b).into()
    };
    stroke_col.a *= opacity;

    let center = transform.map_point(slot.center());
    let radius = transform.scale_len((slot.height / 2.0 - 0.75).max(1.0));
    paint_circle(
        window,
        center,
        radius,
        fill_col,
        Some((stroke_col, transform.scale_len(1.5))),
    );

    let mut ring_col: Hsla = rgb(0x97ab7e).into();
    ring_col.a = 0.35 * opacity;
    paint_circle_stroke(
        window,
        center,
        transform.scale_len(slot.height * 0.34),
        ring_col,
        transform.scale_len(0.8),
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_stack_badge(
    window: &mut Window,
    cx: &mut App,
    geom: &BoardGeometry,
    board_builtin: BuiltInThemeId,
    transform: &BoardTransform,
    center: LogicalPoint,
    checker_width: f32,
    total: usize,
    white: bool,
) {
    let screen_center = transform.map_point(center);
    let label = total.to_string();
    match geom {
        BoardGeometry::BgBlitz(_) => {
            paint_circle(
                window,
                screen_center,
                transform.scale_len(checker_width * 0.2),
                rgb(0xfffbed).into(),
                Some((rgb(0x172b22).into(), transform.scale_len(2.0))),
            );
            paint_centered_text(
                window,
                cx,
                &label,
                screen_center,
                transform.scale_len(checker_width * 0.23),
                FontWeight::BOLD,
                rgb(0x172b22).into(),
            );
        }
        BoardGeometry::BuiltIn => {
            let pal = board_builtin.palette();
            let text_color: Hsla = if pal.inlay.is_some() {
                rgb(if white {
                    pal.black_checker
                } else {
                    pal.white_checker
                })
                .into()
            } else {
                rgb(if white { 0x3e5849 } else { 0xf4f3dc }).into()
            };
            paint_centered_text(
                window,
                cx,
                &label,
                screen_center,
                transform.scale_len(15.0),
                FontWeight::SEMIBOLD,
                text_color,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_themed_die(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    theme: BuiltInThemeId,
    light: bool,
    value: Option<u8>,
    usage: DieUsage,
    base_alpha: f32,
    hovered: bool,
) {
    let pal = theme.palette();
    let (edge_a, edge_b) = pal.die_edge_stops(light);
    let (face_a, face_b) = pal.die_face_stops(light);
    let ink: Hsla = rgb(if light { 0x19221e } else { 0xfff9e5 }).into();

    let draw_with_alpha = |window: &mut Window, alpha: f32| {
        let mut e0 = edge_a;
        let mut e1 = edge_b;
        let mut f0 = face_a;
        let mut f1 = face_b;
        let mut pip_col = ink;
        e0.a *= alpha;
        e1.a *= alpha;
        f0.a *= alpha;
        f1.a *= alpha;
        pip_col.a *= alpha;

        let radius = bounds.size.width * (10.0 / 64.0);
        window.paint_quad(quad(
            bounds,
            Corners::all(radius),
            linear_gradient(
                144.0,
                linear_color_stop(e0, 0.0),
                linear_color_stop(e1, 1.0),
            ),
            px(0.0),
            Hsla::transparent_black(),
            BorderStyle::Solid,
        ));

        let inset = bounds.size.width * (2.5 / 64.0);
        let face_bounds = Bounds {
            origin: point(bounds.origin.x + inset, bounds.origin.y + inset),
            size: size(
                bounds.size.width - inset * 2.0,
                bounds.size.height - inset * 2.0,
            ),
        };
        window.paint_quad(quad(
            face_bounds,
            Corners::all(face_bounds.size.width * 0.28),
            linear_gradient(
                156.0,
                linear_color_stop(f0, 0.0),
                linear_color_stop(f1, 1.0),
            ),
            px(0.0),
            Hsla::transparent_black(),
            BorderStyle::Solid,
        ));

        let pips: &[usize] = match value {
            Some(1) => &[4],
            Some(2) => &[2, 6],
            Some(3) => &[2, 4, 6],
            Some(4) => &[0, 2, 6, 8],
            Some(5) => &[0, 2, 4, 6, 8],
            Some(6) => &[0, 2, 3, 5, 6, 8],
            _ => &[],
        };
        let pip_r = bounds.size.width * (7.1 / 64.0);
        for &cell in pips {
            let cx = f32::from(bounds.origin.x)
                + f32::from(bounds.size.width) * ((16.0 + (cell % 3) as f32 * 16.0) / 64.0);
            let cy = f32::from(bounds.origin.y)
                + f32::from(bounds.size.height) * ((16.0 + (cell / 3) as f32 * 16.0) / 64.0);
            paint_circle(window, point(px(cx), px(cy)), pip_r, pip_col, None);
        }
        if value.is_none() {
            let dash_w = bounds.size.width * (14.0 / 64.0);
            let dash_h = bounds.size.height * (3.0 / 64.0);
            let dash = Bounds {
                origin: point(
                    bounds.center().x - dash_w / 2.0,
                    bounds.center().y - dash_h / 2.0,
                ),
                size: size(dash_w, dash_h),
            };
            window.paint_quad(fill(dash, pip_col));
        }
    };

    match usage {
        DieUsage::Unused => draw_with_alpha(window, base_alpha),
        DieUsage::Used => {
            let used_alpha = if hovered { 0.55 } else { 0.28 };
            draw_with_alpha(window, base_alpha * used_alpha);
        }
        DieUsage::Half => {
            let half_w = bounds.size.width / 2.0;
            let left_mask = gpui_kit::ContentMask {
                bounds: Bounds {
                    origin: bounds.origin,
                    size: size(half_w, bounds.size.height),
                },
            };
            let right_mask = gpui_kit::ContentMask {
                bounds: Bounds {
                    origin: point(bounds.origin.x + half_w, bounds.origin.y),
                    size: size(bounds.size.width - half_w, bounds.size.height),
                },
            };
            window.with_content_mask(Some(left_mask), |window| {
                draw_with_alpha(window, base_alpha * 0.28);
            });
            window.with_content_mask(Some(right_mask), |window| {
                draw_with_alpha(window, base_alpha);
            });
        }
    }
}

fn paint_triangle(
    window: &mut Window,
    a: Point<Pixels>,
    b: Point<Pixels>,
    c: Point<Pixels>,
    color: Hsla,
) {
    let mut builder = PathBuilder::fill();
    builder.move_to(a);
    builder.line_to(b);
    builder.line_to(c);
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn paint_circle(
    window: &mut Window,
    center: Point<Pixels>,
    radius: Pixels,
    fill_color: Hsla,
    stroke: Option<(Hsla, Pixels)>,
) {
    let d = radius * 2.0;
    let bounds = Bounds {
        origin: point(center.x - radius, center.y - radius),
        size: size(d, d),
    };
    let (border_color, border_width) = stroke.unwrap_or((Hsla::transparent_black(), px(0.0)));
    window.paint_quad(quad(
        bounds,
        Corners::all(radius),
        fill_color,
        border_width,
        border_color,
        BorderStyle::Solid,
    ));
}

fn paint_circle_stroke(
    window: &mut Window,
    center: Point<Pixels>,
    radius: Pixels,
    color: Hsla,
    width: Pixels,
) {
    paint_circle(
        window,
        center,
        radius,
        Hsla::transparent_black(),
        Some((color, width)),
    );
}

fn paint_centered_text(
    window: &mut Window,
    cx: &mut App,
    text: &str,
    center: Point<Pixels>,
    font_size: Pixels,
    weight: FontWeight,
    color: Hsla,
) {
    if text.is_empty() || f32::from(font_size) < 3.0 {
        return;
    }
    let shared: SharedString = text.to_string().into();
    let font = Font {
        weight,
        ..Default::default()
    };
    let run = TextRun {
        len: shared.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(shared, font_size, &[run], None);
    let line_h = font_size * 1.15;
    let origin = point(center.x - shaped.width / 2.0, center.y - line_h / 2.0);
    let _ = shaped.paint(origin, line_h, TextAlign::Left, None, window, cx);
}

fn scale_bounds_about_center(bounds: Bounds<Pixels>, factor: f32) -> Bounds<Pixels> {
    if (factor - 1.0).abs() < 1e-4 {
        return bounds;
    }
    let c = bounds.center();
    let nw = bounds.size.width * factor;
    let nh = bounds.size.height * factor;
    Bounds {
        origin: point(c.x - nw / 2.0, c.y - nh / 2.0),
        size: size(nw, nh),
    }
}

/// Fixed legal preview position mirrored 1:1 from `BoardSettingsSection.tsx`
/// (checkers on points, both bars, and both bear-off trays).
const GALLERY_PREVIEW_BOARD: [i32; 26] = [
    -1, -1, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -4, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 1, 1,
];

/// Paints a full 1:1 miniature built-in board preview matching the browser
/// `BoardSettingsSection.tsx` gallery tiles (`<Board position={previewPosition} />`):
/// frame, rail gradient & inlay, felt + `resvg` surface texture, bar, bear-off
/// tray, 24 alternating points, all preview checkers (points, bars & trays),
/// doubling cube, and themed dice.
fn paint_builtin_gallery_preview(
    theme: BuiltInThemeId,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    if w <= 1.0 || h <= 1.0 {
        return;
    }

    let pal = theme.palette();
    let assets = builtin_gallery_assets(theme);
    let sx = w / NATIVE_WIDTH;
    let sy = h / NATIVE_HEIGHT;

    let map_pt = |x: f32, y: f32| point(bounds.origin.x + px(x * sx), bounds.origin.y + px(y * sy));
    let map_rect = |r: LogicalRect| Bounds {
        origin: map_pt(r.x, r.y),
        size: size(px(r.width * sx), px(r.height * sy)),
    };

    // Frame + optional inlaid rail gradient.
    let frame_bg = if pal.inlay.is_some() {
        linear_gradient(
            180.0,
            linear_color_stop(pal.rail_highlight(), 0.0),
            linear_color_stop(pal.rail_shadow(), 1.0),
        )
    } else {
        rgb(pal.frame).into()
    };
    window.paint_quad(quad(
        bounds,
        Corners::all(px(4.0)),
        frame_bg,
        px(0.0),
        Hsla::transparent_black(),
        BorderStyle::Solid,
    ));

    if let Some(inlay_hex) = pal.inlay {
        let inset = px(1.5);
        let inlay_bounds = Bounds {
            origin: point(bounds.origin.x + inset, bounds.origin.y + inset),
            size: size(
                (bounds.size.width - inset * 2.0).max(px(0.0)),
                (bounds.size.height - inset * 2.0).max(px(0.0)),
            ),
        };
        let mut inlay_color: Hsla = rgb(inlay_hex).into();
        inlay_color.a = 0.58;
        window.paint_quad(quad(
            inlay_bounds,
            Corners::all(px(3.0)),
            Hsla::transparent_black(),
            px(0.75),
            inlay_color,
            BorderStyle::Solid,
        ));
    }

    // Playing field + SVG surface texture (for the 6 textured built-in themes).
    let field_bounds = map_rect(NATIVE_FIELD);
    window.paint_quad(quad(
        field_bounds,
        Corners::all(px(1.5)),
        rgb(pal.felt),
        px(0.0),
        Hsla::transparent_black(),
        BorderStyle::Solid,
    ));
    if let Some(surf) = assets.surface {
        let _ = window.paint_image(
            field_bounds,
            field_bounds,
            Corners::all(px(1.5)),
            surf,
            0,
            false,
        );
    }

    // Central bar and right bear-off tray.
    let bar_rect = LogicalRect::new(
        NATIVE_WIDTH / 2.0 - NATIVE_POINT_PITCH / 2.0,
        NATIVE_FIELD.y,
        NATIVE_POINT_PITCH,
        NATIVE_FIELD.height,
    );
    let bar_border = if pal.inlay.is_some() {
        pal.rail_shadow()
    } else {
        Hsla::transparent_black()
    };
    let bar_border_w = if pal.inlay.is_some() {
        px(0.75)
    } else {
        px(0.0)
    };
    for r in [bar_rect, NATIVE_OFF_TRAY] {
        window.paint_quad(quad(
            map_rect(r),
            Corners::all(px(1.0)),
            rgb(pal.bar),
            bar_border_w,
            bar_border,
            BorderStyle::Solid,
        ));
    }

    // 24 alternating triangles.
    for i in 0..12usize {
        let center = BoardGeometry::native_point_x(i);
        let left = center - NATIVE_POINT_PITCH / 2.0;
        let right = center + NATIVE_POINT_PITCH / 2.0;

        let top_color: Hsla = rgb(if i % 2 == 1 {
            pal.point_light
        } else {
            pal.point_dark
        })
        .into();
        let bot_color: Hsla = rgb(if i % 2 == 1 {
            pal.point_dark
        } else {
            pal.point_light
        })
        .into();

        paint_triangle(
            window,
            map_pt(left, NATIVE_FIELD.y),
            map_pt(right, NATIVE_FIELD.y),
            map_pt(center, 214.0),
            top_color,
        );
        paint_triangle(
            window,
            map_pt(left, NATIVE_FIELD.bottom()),
            map_pt(right, NATIVE_FIELD.bottom()),
            map_pt(center, 256.0),
            bot_color,
        );
    }

    // Full preview position checkers on points, bars, and bear-off trays.
    let off = borne_off_counts(&GALLERY_PREVIEW_BOARD);
    let mut stacks: Vec<(i32, i32)> = POINT_COLUMNS
        .iter()
        .map(|pc| (pc.point, GALLERY_PREVIEW_BOARD[pc.point as usize]))
        .collect();
    stacks.push((0, GALLERY_PREVIEW_BOARD[0]));
    stacks.push((25, GALLERY_PREVIEW_BOARD[25]));
    if off[1] > 0 {
        stacks.push((26, -off[1]));
    }
    if off[0] > 0 {
        stacks.push((-1, off[0]));
    }

    for (pt, count) in stacks {
        if count == 0 {
            continue;
        }
        let stack = point_stack_geometry(pt, count, &BoardGeometry::BuiltIn);
        let white = count > 0;
        let sprite = if white {
            assets.white_checker.as_ref()
        } else {
            assets.black_checker.as_ref()
        };
        for slot in stack.checker_rects() {
            let b = map_rect(slot);
            if let Some(img_sprite) = sprite {
                let _ =
                    window.paint_image(b, b, Corners::default(), Arc::clone(img_sprite), 0, false);
            } else {
                let fill_col: Hsla = rgb(if white {
                    pal.white_checker
                } else {
                    pal.black_checker
                })
                .into();
                let stroke_col: Hsla = if white {
                    rgb(0xb4b4a0).into()
                } else {
                    rgb(0x1c362b).into()
                };
                let radius = (b.size.height.min(b.size.width) / 2.0 - px(0.25)).max(px(1.0));
                paint_circle(
                    window,
                    b.center(),
                    radius,
                    fill_col,
                    Some((stroke_col, px(0.6))),
                );
            }
        }
    }

    // Centered left-rail doubling cube at exact built-in board coordinates.
    if let Some(cube_img) = assets.cube {
        let cube_logical = cube_rect(&BoardGeometry::BuiltIn, None, None);
        let cb = map_rect(cube_logical);
        let _ = window.paint_image(cb, cb, Corners::all(px(1.5)), cube_img, 0, false);
    }

    // Preview dice `[3, 2]` in exact built-in board coordinates.
    let dice = dice_placements(
        &BoardGeometry::BuiltIn,
        &[(0, Some(3)), (0, Some(2))],
        false,
        false,
    );
    for die in dice {
        let db = map_rect(die.rect);
        paint_themed_die(
            window,
            db,
            theme,
            true,
            die.value,
            DieUsage::Unused,
            1.0,
            false,
        );
    }
}

/// Lightweight Settings → Board gallery thumbnail tile (`BoardSettingsSection.tsx`).
///
/// Renders either a fast BGBlitz composite (`~320×216` BGRA thumbnail + 2 corner
/// checkers at ~10% width) or a 1:1 miniature built-in board (`previewPosition`
/// with frame, inlay, surface texture, bar, bear-off tray, 24 points, checkers,
/// cube, and dice) with selection outline, badge, and theme name.
pub struct BoardGalleryTile {
    pub theme_id: String,
    pub label: SharedString,
    pub selected: bool,
    pub bgblitz_entry: Option<BgBlitzThemeEntry>,
    pub accent: Hsla,
    pub border: Hsla,
    pub surface_selected: Hsla,
}

impl BoardGalleryTile {
    pub fn render_element(self, id: impl Into<ElementId>) -> impl IntoElement {
        self.render_element_with_badge(id, None)
    }

    pub fn render_element_with_badge(
        self,
        id: impl Into<ElementId>,
        selected_badge: Option<(SharedString, Hsla)>,
    ) -> impl IntoElement {
        let mut thumb = if let Some(entry) = self.bgblitz_entry {
            div()
                .relative()
                .w_full()
                .h(px(78.0))
                .rounded(px(4.0))
                .overflow_hidden()
                .child(img(entry.thumbnail_bg).size_full())
                .child(
                    div()
                        .absolute()
                        .bottom(px(6.0))
                        .left(px(14.0))
                        .w(px(13.0))
                        .h(px(13.0))
                        .child(img(entry.thumbnail_white_checker).size_full()),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(6.0))
                        .right(px(14.0))
                        .w(px(13.0))
                        .h(px(13.0))
                        .child(img(entry.thumbnail_black_checker).size_full()),
                )
        } else {
            let theme = BuiltInThemeId::parse(&self.theme_id).unwrap_or(BuiltInThemeId::Forest);
            div()
                .relative()
                .w_full()
                .h(px(78.0))
                .rounded(px(4.0))
                .overflow_hidden()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            paint_builtin_gallery_preview(theme, bounds, window);
                        },
                    )
                    .size_full(),
                )
        };

        if self.selected
            && let Some((badge_text, badge_fg)) = selected_badge
        {
            thumb = thumb.child(
                div()
                    .absolute()
                    .top(px(4.0))
                    .left(px(4.0))
                    .px(px(5.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(self.accent)
                    .text_color(badge_fg)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("✓ {badge_text}")),
            );
        }

        let mut card = div()
            .id(id)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(7.0))
            .rounded(px(6.0));
        if self.selected {
            card = card
                .border_2()
                .border_color(self.accent)
                .bg(self.surface_selected);
        } else {
            card = card.border_1().border_color(self.border);
        }
        let mut label_el = div().text_xs().child(self.label);
        if self.selected {
            label_el = label_el.font_weight(FontWeight::SEMIBOLD);
        }
        card.child(thumb).child(label_el).test_support()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{PlayFrame, checker_frame};
    use crate::geometry::{BearoffDirection, project_move};
    use gpui_kit::{AppContext, TestAppContext, WindowOptions, test::TestWindowExt};
    use std::sync::atomic::{AtomicI32, Ordering};

    #[gpui_kit::test]
    async fn board_view_renders_and_dispatches_click_to_move_and_dice_cube_events(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
        });

        let atlas = Arc::new(Mutex::new(ThemeAtlasCache::default()));
        let clicked_point = Arc::new(AtomicI32::new(-99));
        let dice_clicks = Arc::new(AtomicI32::new(0));
        let cube_clicks = Arc::new(AtomicI32::new(0));

        let cp = Arc::clone(&clicked_point);
        let dc = Arc::clone(&dice_clicks);
        let cc = Arc::clone(&cube_clicks);

        let (window, board_entity) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    let view = cx.new(|cx| {
                        let mut b = BoardView::new("test-board", Arc::clone(&atlas));
                        b.set_theme_config(
                            BoardThemeConfig {
                                board_theme: "terra-rossa".into(),
                                checker_theme: Some("ion".into()),
                                cube_theme: Some("slate".into()),
                                dice_theme: Some("match-board".into()),
                            },
                            cx,
                        );
                        b.set_play_orientation(
                            Some(PlayOrientation {
                                human: 0,
                                direction: BearoffDirection::Left,
                            }),
                            cx,
                        );
                        let pos = BoardPosition {
                            dice: vec![3, 1],
                            ..Default::default()
                        };
                        let preview = project_move(&pos.board, 0, &[3, 1], "8/5 6/5");
                        b.set_position(pos, cx);
                        b.set_preview(Some(preview), BoardViewMode::Arrows, cx);
                        b.set_interaction(
                            Some(BoardInteractionState {
                                enabled_points: vec![6, 8, 13],
                                source_points: vec![6, 8, 13],
                                destination_points: vec![5],
                                hovered_source: Some(6),
                                dice: Some(BoardDiceInteraction {
                                    can_swap: true,
                                    used: [DieUsage::Half, DieUsage::Unused],
                                    ..Default::default()
                                }),
                                can_double: true,
                                cube_interactive: true,
                            }),
                            cx,
                        );
                        b
                    });

                    cx.subscribe(
                        &view,
                        move |_entity, event: &BoardEvent, _cx| match *event {
                            BoardEvent::PointClicked(pt) => {
                                cp.store(pt, Ordering::SeqCst);
                            }
                            BoardEvent::DiceClicked => {
                                dc.fetch_add(1, Ordering::SeqCst);
                            }
                            BoardEvent::CubeClicked => {
                                cc.fetch_add(1, Ordering::SeqCst);
                            }
                            BoardEvent::SourceHovered(_) => {}
                        },
                    )
                    .detach();

                    view
                })
            })
            .expect("open test board window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("test-board").visible());

            let hitboxes = board_entity.read(cx).last_hitboxes();
            assert_eq!(hitboxes.point_targets.len(), 28);
            assert!(hitboxes.dice_pair_bounds.is_some());
            assert!(hitboxes.cube_bounds.is_some());

            // Click point 6 via its prepaint screen-space hitbox center.
            let board_origin = window.find("test-board").bounds().origin;
            let p6_bounds = hitboxes
                .point_targets
                .iter()
                .find(|(pt, _)| *pt == 6)
                .unwrap()
                .1;
            let p6_local = p6_bounds.center() - board_origin;
            window.click_at("test-board", p6_local, cx);

            // Click dice pair center.
            let dice_local = hitboxes.dice_pair_bounds.unwrap().center() - board_origin;
            window.click_at("test-board", dice_local, cx);

            // Click cube center.
            let cube_local = hitboxes.cube_bounds.unwrap().center() - board_origin;
            window.click_at("test-board", cube_local, cx);
        })
        .expect("dispatch click events");

        // GPUI flushes Effect::Emit when update_window returns.
        assert_eq!(clicked_point.load(Ordering::SeqCst), 6);
        assert_eq!(dice_clicks.load(Ordering::SeqCst), 1);
        assert_eq!(cube_clicks.load(Ordering::SeqCst), 1);

        cx.update_window(window, |_, window, cx| {
            // Exercise checker flight animation frame rendering + cancellation.
            board_entity.update(cx, |view, cx| {
                let b = initial_board();
                view.enqueue_animation(
                    [PlayFrame::Checker(checker_frame(&b, 6, 3, 0, vec![3, 1]))],
                    cx,
                );
                assert!(view.is_animating());
            });
            window.render_frame(cx);
            board_entity.update(cx, |view, cx| {
                view.cancel_animation(cx);
                assert!(!view.is_animating());
            });
            window.render_frame(cx);
        })
        .expect("render and cancel animation");
    }

    #[gpui_kit::test]
    async fn board_gallery_tile_and_after_preview_and_bgblitz_eviction_render_cleanly(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
        });

        let repo_themes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let atlas = Arc::new(Mutex::new(ThemeAtlasCache::new(Some(repo_themes), 1)));
        let bgblitz_entry = {
            let mut guard = atlas.lock().unwrap();
            let _ = guard.scan_bgblitz_dir();
            guard.bgblitz_entries().first().cloned().cloned()
        };

        struct GalleryHost {
            board: gpui_kit::Entity<BoardView>,
            entry: Option<BgBlitzThemeEntry>,
        }
        impl Render for GalleryHost {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                let tile_builtin = BoardGalleryTile {
                    theme_id: "forest".into(),
                    label: "Forest".into(),
                    selected: true,
                    bgblitz_entry: None,
                    accent: rgb(0x315bc5).into(),
                    border: rgb(0xdce2ea).into(),
                    surface_selected: rgb(0xeaf0ff).into(),
                }
                .render_element("tile-forest");

                let tile_textured = BoardGalleryTile {
                    theme_id: "viridian".into(),
                    label: "Viridian".into(),
                    selected: false,
                    bgblitz_entry: None,
                    accent: rgb(0x315bc5).into(),
                    border: rgb(0xdce2ea).into(),
                    surface_selected: rgb(0xeaf0ff).into(),
                }
                .render_element("tile-viridian");

                let mut row = div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(tile_builtin)
                    .child(tile_textured);
                if let Some(entry) = self.entry.clone() {
                    let tile_bgblitz = BoardGalleryTile {
                        theme_id: entry.id.clone(),
                        label: entry.name.clone().into(),
                        selected: false,
                        bgblitz_entry: Some(entry),
                        accent: rgb(0x315bc5).into(),
                        border: rgb(0xdce2ea).into(),
                        surface_selected: rgb(0xeaf0ff).into(),
                    }
                    .render_element("tile-bgblitz");
                    row = row.child(tile_bgblitz);
                }

                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(row)
                    .child(div().flex_1().child(self.board.clone()))
            }
        }

        let atlas_for_view = Arc::clone(&atlas);
        let first_bg_id = bgblitz_entry.as_ref().map(|e| e.id.clone());
        let (window, host) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    let board = cx.new(|cx| {
                        let mut b = BoardView::new("preview-board", Arc::clone(&atlas_for_view));
                        if let Some(id) = first_bg_id.clone() {
                            b.set_theme_config(
                                BoardThemeConfig {
                                    board_theme: id,
                                    ..Default::default()
                                },
                                cx,
                            );
                        }
                        let pos = BoardPosition {
                            dice: vec![3, 2],
                            ..Default::default()
                        };
                        let preview = project_move(&pos.board, 0, &[3, 2], "13/10 13/11");
                        b.set_position(pos, cx);
                        b.set_preview(Some(preview), BoardViewMode::After, cx);
                        b.set_cube_offer_pending(true, cx);
                        b.set_show_both_dice_colors(true, cx);
                        b
                    });
                    cx.new(|_| GalleryHost {
                        board,
                        entry: bgblitz_entry,
                    })
                })
            })
            .expect("open gallery host window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("tile-forest").visible());
            assert!(window.find("tile-viridian").visible());
            assert!(window.find("preview-board").visible());

            // Verify After mode cleared dice hitboxes and flush_evictions drops cleanly.
            let board_ent = host.read(cx).board.clone();
            assert!(
                board_ent
                    .read(cx)
                    .last_hitboxes()
                    .dice_pair_bounds
                    .is_none()
            );

            let mut guard = atlas.lock().unwrap();
            let second_id = guard.bgblitz_entries().get(1).map(|e| e.id.clone());
            if let Some(second) = second_id {
                assert!(guard.get_or_load_bgblitz(&second).is_some());
                let dropped = guard.flush_evictions(Some(window), cx);
                assert!(
                    dropped > 0,
                    "LRU eviction must release GPU images via drop_image"
                );
            }
        })
        .expect("update gallery host window");
    }
}
