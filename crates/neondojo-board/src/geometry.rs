//! Pure backgammon board geometry, coordinate transforms, move-preview
//! projection, arrow geometry, and prepaint hitboxes.
//!
//! Mirrors `web/src/shared/board/{themes.ts, boardMath.ts, move-preview.ts,
//! play-state.ts, MoveArrows.tsx, BoardPlayTargets.tsx}` while delegating
//! backgammon legality and GNUbg Match ID decoding to `neondojo-rules`.

use gpui_kit::{Bounds, Pixels, Point, point, px, size};
use neondojo_rules::backgammon::{
    board as rules_board, checker_paths, expand_joined_play,
    ids::{Field, MatchId},
    notation as rules_notation,
};
use serde::{Deserialize, Serialize};

/// Native built-in SVG viewBox width (logical units).
pub const NATIVE_WIDTH: f32 = 680.0;
/// Native built-in SVG viewBox height (logical units).
pub const NATIVE_HEIGHT: f32 = 470.0;
/// Scale factor inherited from BGBlitz X22 (`84px` checkers on `90px` pitch on a `1490px` board).
pub const NATIVE_SCALE: f32 = 680.0 / 1490.0;
/// Built-in checker diameter in logical units (`~38.3356`).
pub const NATIVE_CHECKER_SIZE: f32 = 84.0 * NATIVE_SCALE;
/// Built-in point pitch in logical units (`~41.0738`).
pub const NATIVE_POINT_PITCH: f32 = 90.0 * NATIVE_SCALE;
/// Playing field width: 12 points plus the central bar.
pub const NATIVE_FIELD_WIDTH: f32 = 13.0 * NATIVE_POINT_PITCH;

/// Playing field rectangle on built-in boards (`x ≈ 73.02, y = 28, w ≈ 533.96, h = 414`).
pub const NATIVE_FIELD: LogicalRect = LogicalRect {
    x: (NATIVE_WIDTH - NATIVE_FIELD_WIDTH) / 2.0,
    y: 28.0,
    width: NATIVE_FIELD_WIDTH,
    height: 414.0,
};

/// Bear-off tray rectangle on the right rail of built-in boards.
pub const NATIVE_OFF_TRAY: LogicalRect = LogicalRect {
    x: NATIVE_FIELD.x + NATIVE_FIELD.width + (NATIVE_FIELD.x - NATIVE_POINT_PITCH) / 2.0,
    y: NATIVE_FIELD.y,
    width: NATIVE_POINT_PITCH,
    height: NATIVE_FIELD.height,
};

/// Logical 2D point in theme coordinates (origin top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    pub x: f32,
    pub y: f32,
}

impl LogicalPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Logical 2D rectangle in theme coordinates (origin top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LogicalRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(self) -> LogicalPoint {
        LogicalPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }
}

/// Bear-off direction preference for live Play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BearoffDirection {
    #[default]
    Right,
    Left,
}

/// Live-play orientation: which player sits at the bottom and which side holds
/// their home board / bear-off tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayOrientation {
    /// `0` = White, `1` = Black.
    pub human: u8,
    pub direction: BearoffDirection,
}

/// Returns `(flip_x, flip_y)` axis signs (`1.0` or `-1.0`) matching
/// `web/src/shared/board/play-state.ts::boardFlips`.
pub const fn board_flips(human: u8, direction: BearoffDirection) -> (f32, f32) {
    let flip_x = match direction {
        BearoffDirection::Left => -1.0,
        BearoffDirection::Right => 1.0,
    };
    let flip_y = if human == 1 { -1.0 } else { 1.0 };
    (flip_x, flip_y)
}

/// Converts an absolute point (`1..=24`) to mover-relative numbering for `player`.
pub const fn relative_point(point: i32, player: u8) -> i32 {
    if player == 0 { point } else { 25 - point }
}

/// BGBlitz point-number strip orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumberOrientation {
    /// Right-to-left, White (player 0) down.
    Rld,
    /// Right-to-left, Black (player 1) up.
    Rlu,
    /// Left-to-right, White (player 0) down.
    Lrd,
    /// Left-to-right, Black (player 1) up.
    Lru,
}

impl NumberOrientation {
    pub const ALL: [Self; 4] = [Self::Lrd, Self::Lru, Self::Rld, Self::Rlu];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rld => "RLD",
            Self::Rlu => "RLU",
            Self::Lrd => "LRD",
            Self::Lru => "LRU",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "RLD" => Some(Self::Rld),
            "RLU" => Some(Self::Rlu),
            "LRD" => Some(Self::Lrd),
            "LRU" => Some(Self::Lru),
            _ => None,
        }
    }

    /// Computes the strip orientation for `(flip_x, number_player)` matching
    /// `Board.tsx` line 269: `${flipX === -1 ? 'LR' : 'RL'}${numberPlayer === 0 ? 'D' : 'U'}`.
    pub const fn from_orientation(flip_x: f32, number_player: u8) -> Self {
        match (flip_x < 0.0, number_player == 0) {
            (true, true) => Self::Lrd,
            (true, false) => Self::Lru,
            (false, true) => Self::Rld,
            (false, false) => Self::Rlu,
        }
    }

    pub const fn is_left_to_right(self) -> bool {
        matches!(self, Self::Lrd | Self::Lru)
    }
}

/// Maps logical board coordinates into screen-space GPUI `Pixels` while
/// preserving exact theme aspect ratio and applying `(flip_x, flip_y)` without
/// double-flipping upright text or sprites.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardTransform {
    pub origin: Point<Pixels>,
    pub scale: f32,
    pub flip_x: f32,
    pub flip_y: f32,
    pub logical_width: f32,
    pub logical_height: f32,
}

impl BoardTransform {
    /// Fits `(logical_width, logical_height)` uniformly inside `available`,
    /// centered along whichever axis has spare space.
    pub fn fit(
        available: Bounds<Pixels>,
        logical_width: f32,
        logical_height: f32,
        flip_x: f32,
        flip_y: f32,
    ) -> Self {
        let avail_w = f32::from(available.size.width).max(1.0);
        let avail_h = f32::from(available.size.height).max(1.0);
        let lw = logical_width.max(1.0);
        let lh = logical_height.max(1.0);
        let scale = (avail_w / lw).min(avail_h / lh);
        let drawn_w = lw * scale;
        let drawn_h = lh * scale;
        let origin = point(
            px(f32::from(available.origin.x) + (avail_w - drawn_w) / 2.0),
            px(f32::from(available.origin.y) + (avail_h - drawn_h) / 2.0),
        );
        Self {
            origin,
            scale,
            flip_x: if flip_x < 0.0 { -1.0 } else { 1.0 },
            flip_y: if flip_y < 0.0 { -1.0 } else { 1.0 },
            logical_width: lw,
            logical_height: lh,
        }
    }

    /// Screen-space bounds of the entire fitted board.
    pub fn board_bounds(&self) -> Bounds<Pixels> {
        Bounds {
            origin: self.origin,
            size: size(
                px(self.logical_width * self.scale),
                px(self.logical_height * self.scale),
            ),
        }
    }

    /// Maps a logical point to screen space, applying `(flip_x, flip_y)` around
    /// the logical board center.
    pub fn map_point(&self, pt: LogicalPoint) -> Point<Pixels> {
        let lx = if self.flip_x < 0.0 {
            self.logical_width - pt.x
        } else {
            pt.x
        };
        let ly = if self.flip_y < 0.0 {
            self.logical_height - pt.y
        } else {
            pt.y
        };
        point(
            px(f32::from(self.origin.x) + lx * self.scale),
            px(f32::from(self.origin.y) + ly * self.scale),
        )
    }

    /// Maps a logical rectangle to a normalized screen-space `Bounds<Pixels>`
    /// with non-negative width and height regardless of `flip_x`/`flip_y`.
    pub fn map_rect(&self, rect: LogicalRect) -> Bounds<Pixels> {
        let p0 = self.map_point(LogicalPoint::new(rect.x, rect.y));
        let p1 = self.map_point(LogicalPoint::new(rect.right(), rect.bottom()));
        let min_x = f32::from(p0.x).min(f32::from(p1.x));
        let min_y = f32::from(p0.y).min(f32::from(p1.y));
        let max_x = f32::from(p0.x).max(f32::from(p1.x));
        let max_y = f32::from(p0.y).max(f32::from(p1.y));
        Bounds {
            origin: point(px(min_x), px(min_y)),
            size: size(px(max_x - min_x), px(max_y - min_y)),
        }
    }

    /// Inverse mapping from a screen-space pointer location to logical board
    /// coordinates.
    pub fn screen_to_logical(&self, screen: Point<Pixels>) -> LogicalPoint {
        let raw_x = (f32::from(screen.x) - f32::from(self.origin.x)) / self.scale.max(1e-6);
        let raw_y = (f32::from(screen.y) - f32::from(self.origin.y)) / self.scale.max(1e-6);
        LogicalPoint::new(
            if self.flip_x < 0.0 {
                self.logical_width - raw_x
            } else {
                raw_x
            },
            if self.flip_y < 0.0 {
                self.logical_height - raw_y
            } else {
                raw_y
            },
        )
    }

    /// Scales a logical length into screen `Pixels`.
    pub fn scale_len(&self, logical: f32) -> Pixels {
        px(logical * self.scale)
    }
}

/// Numeric layout descriptor extracted from a BGBlitz XML theme.
#[derive(Clone, Debug, PartialEq)]
pub struct BgBlitzMetrics {
    pub bg_width: f32,
    pub bg_height: f32,
    pub columns: [f32; 15],
    pub checker_offset_x: f32,
    pub checker_offset_y: f32,
    pub white_checker_size: (f32, f32),
    pub black_checker_size: (f32, f32),
    pub white_die_size: (f32, f32),
    pub black_die_size: (f32, f32),
    pub dice_left_x: f32,
    pub dice_right_x: f32,
    pub cube_left_x: f32,
    pub cube_right_x: f32,
    pub cube_offset_y: f32,
    pub cube_centered_y: f32,
    pub numbers_x: f32,
    pub numbers_y: [f32; 2],
}

/// Active board layout coordinate space (built-in `680×470` vs BGBlitz XML).
#[derive(Clone, Debug, PartialEq)]
pub enum BoardGeometry {
    BuiltIn,
    BgBlitz(BgBlitzMetrics),
}

impl BoardGeometry {
    pub fn dimensions(&self) -> (f32, f32) {
        match self {
            Self::BuiltIn => (NATIVE_WIDTH, NATIVE_HEIGHT),
            Self::BgBlitz(m) => (m.bg_width, m.bg_height),
        }
    }

    pub fn aspect_ratio(&self) -> f32 {
        let (w, h) = self.dimensions();
        w / h.max(1.0)
    }

    pub fn is_bgblitz(&self) -> bool {
        matches!(self, Self::BgBlitz(_))
    }

    /// Reference horizontal scale relative to built-in `680px` width (used for
    /// arrow stroke thicknesses and arrowhead sizes).
    pub fn arrow_scale(&self) -> f32 {
        self.dimensions().0 / NATIVE_WIDTH
    }

    /// Center X coordinate of built-in point index `0..12` (left-to-right,
    /// skipping the central bar between indices 5 and 6).
    pub fn native_point_x(index: usize) -> f32 {
        let bar_skip = if index >= 6 { 1.0 } else { 0.0 };
        NATIVE_FIELD.x + (index as f32 + 0.5 + bar_skip) * NATIVE_POINT_PITCH
    }

    /// Sum of the two outer playing point centers (`columns[1]` and
    /// `columns[13]`) used to mirror BGBlitz point-number strips cleanly despite
    /// uneven transparent strip margins (`BgBlitzBoard.tsx:132`).
    pub fn outer_point_center_sum(&self) -> Option<f32> {
        match self {
            Self::BuiltIn => None,
            Self::BgBlitz(m) => Some(
                m.columns[1] + m.columns[13] + 2.0 * m.checker_offset_x + m.white_checker_size.0,
            ),
        }
    }
}

/// Mapping of a backgammon point (`1..=24`) to its BGBlitz column index
/// (`1..=6` left half, `8..=13` right half) and vertical side (`bottom`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointColumn {
    pub point: i32,
    pub column: usize,
    pub bottom: bool,
}

/// All 24 board points in canonical rendering order (top 13..=24, bottom 12..=1).
pub const POINT_COLUMNS: [PointColumn; 24] = [
    PointColumn {
        point: 13,
        column: 1,
        bottom: false,
    },
    PointColumn {
        point: 14,
        column: 2,
        bottom: false,
    },
    PointColumn {
        point: 15,
        column: 3,
        bottom: false,
    },
    PointColumn {
        point: 16,
        column: 4,
        bottom: false,
    },
    PointColumn {
        point: 17,
        column: 5,
        bottom: false,
    },
    PointColumn {
        point: 18,
        column: 6,
        bottom: false,
    },
    PointColumn {
        point: 19,
        column: 8,
        bottom: false,
    },
    PointColumn {
        point: 20,
        column: 9,
        bottom: false,
    },
    PointColumn {
        point: 21,
        column: 10,
        bottom: false,
    },
    PointColumn {
        point: 22,
        column: 11,
        bottom: false,
    },
    PointColumn {
        point: 23,
        column: 12,
        bottom: false,
    },
    PointColumn {
        point: 24,
        column: 13,
        bottom: false,
    },
    PointColumn {
        point: 12,
        column: 1,
        bottom: true,
    },
    PointColumn {
        point: 11,
        column: 2,
        bottom: true,
    },
    PointColumn {
        point: 10,
        column: 3,
        bottom: true,
    },
    PointColumn {
        point: 9,
        column: 4,
        bottom: true,
    },
    PointColumn {
        point: 8,
        column: 5,
        bottom: true,
    },
    PointColumn {
        point: 7,
        column: 6,
        bottom: true,
    },
    PointColumn {
        point: 6,
        column: 8,
        bottom: true,
    },
    PointColumn {
        point: 5,
        column: 9,
        bottom: true,
    },
    PointColumn {
        point: 4,
        column: 10,
        bottom: true,
    },
    PointColumn {
        point: 3,
        column: 11,
        bottom: true,
    },
    PointColumn {
        point: 2,
        column: 12,
        bottom: true,
    },
    PointColumn {
        point: 1,
        column: 13,
        bottom: true,
    },
];

/// Stack placement for a single point, bar, or bear-off tray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub step: f32,
    pub visible: usize,
    pub total: usize,
    pub bottom: bool,
    pub column: usize,
}

impl StackGeometry {
    /// Returns the logical bounding rectangle of each visible checker (`0..visible`)
    /// stacked edge-to-edge (`step == height`, zero gap).
    pub fn checker_rects(&self) -> Vec<LogicalRect> {
        let dir = if self.bottom { -1.0 } else { 1.0 };
        (0..self.visible)
            .map(|i| {
                LogicalRect::new(
                    self.x,
                    self.y + dir * (i as f32) * self.step,
                    self.width,
                    self.height,
                )
            })
            .collect()
    }

    /// Returns the logical center of the 5th checker when `total > 5`, where
    /// the count badge (`6..15`) is drawn.
    pub fn badge_center(&self) -> Option<LogicalPoint> {
        if self.total <= 5 {
            return None;
        }
        let dir = if self.bottom { -1.0 } else { 1.0 };
        Some(LogicalPoint::new(
            self.x + self.width / 2.0,
            self.y + dir * 4.0 * self.step + self.height / 2.0,
        ))
    }

    /// Tight bounding box covering all visible checkers in this stack (used for
    /// hover targets matching `.play-checker-hover`).
    pub fn stack_bounds(&self) -> Option<LogicalRect> {
        if self.visible == 0 {
            return None;
        }
        let span = ((self.visible - 1) as f32) * self.step;
        let top_y = if self.bottom { self.y - span } else { self.y };
        Some(LogicalRect::new(
            self.x,
            top_y,
            self.width,
            self.height + span,
        ))
    }
}

/// Computes checker stack geometry for any point (`1..=24`), bar (`0` Black,
/// `25` White), or off-tray (`-1` White, `26` Black).
pub fn point_stack_geometry(point: i32, count: i32, geom: &BoardGeometry) -> StackGeometry {
    let mapping = POINT_COLUMNS.iter().find(|entry| entry.point == point);
    let bottom = mapping
        .map(|m| m.bottom)
        .unwrap_or(point == -1 || point == 25);
    let off = point == -1 || point == 26;
    let column = mapping
        .map(|m| m.column)
        .unwrap_or(if off { 14 } else { 7 });
    let total = count.unsigned_abs() as usize;
    let visible = total.min(5);

    match geom {
        BoardGeometry::BgBlitz(m) => {
            let (width, height) = if count < 0 {
                m.black_checker_size
            } else {
                m.white_checker_size
            };
            let inset = m.bg_width * 0.028 + m.checker_offset_y;
            let y = if bottom {
                m.bg_height - inset - height
            } else {
                inset
            };
            StackGeometry {
                x: m.columns[column] + m.checker_offset_x,
                y,
                width,
                height,
                step: height,
                visible,
                total,
                bottom,
                column,
            }
        }
        BoardGeometry::BuiltIn => {
            let center_x = if off {
                NATIVE_OFF_TRAY.x + NATIVE_OFF_TRAY.width / 2.0
            } else if column == 7 {
                NATIVE_WIDTH / 2.0
            } else {
                let index = column - 1 - usize::from(column > 7);
                BoardGeometry::native_point_x(index)
            };
            let y = if bottom {
                NATIVE_FIELD.y + NATIVE_FIELD.height - NATIVE_CHECKER_SIZE
            } else {
                NATIVE_FIELD.y
            };
            StackGeometry {
                x: center_x - NATIVE_CHECKER_SIZE / 2.0,
                y,
                width: NATIVE_CHECKER_SIZE,
                height: NATIVE_CHECKER_SIZE,
                step: NATIVE_CHECKER_SIZE,
                visible,
                total,
                bottom,
                column,
            }
        }
    }
}

/// Computes the exact slot rectangle for a checker sprite inside a board slot,
/// preserving the checker artwork's aspect ratio and centering horizontally
/// (`CheckerPiece.tsx`).
pub fn checker_face_rect(slot: LogicalRect, image_w: f32, image_h: f32) -> LogicalRect {
    let face_width = if image_h > 0.0 {
        slot.height * (image_w / image_h)
    } else {
        slot.width
    };
    LogicalRect::new(
        slot.x + (slot.width - face_width) / 2.0,
        slot.y,
        face_width,
        slot.height,
    )
}

/// Borne-off checker counts `[white_off, black_off]`.
pub fn borne_off_counts(board: &[i32; 26]) -> [i32; 2] {
    let mut white_on_board = 0;
    let mut black_on_board = 0;
    for &count in board {
        if count > 0 {
            white_on_board += count;
        } else if count < 0 {
            black_on_board -= count;
        }
    }
    [(15 - white_on_board).max(0), (15 - black_on_board).max(0)]
}

/// Pip counts `[white_pips, black_pips]` delegating directly to `neondojo-rules`.
pub fn pip_counts(board: &[i32; 26]) -> [i32; 2] {
    rules_board::pip_counts(board)
}

/// Decoded match context from a GNUbg 72-bit Match ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedMatchContext {
    pub scores: [i32; 2],
    pub length: i32,
    pub jacoby: bool,
    pub cube: i32,
    pub owner: Option<u8>,
    pub crawford: bool,
    pub doubled: bool,
}

impl Default for DecodedMatchContext {
    fn default() -> Self {
        Self {
            scores: [0, 0],
            length: 0,
            jacoby: true,
            cube: 1,
            owner: None,
            crawford: false,
            doubled: false,
        }
    }
}

/// Decodes a GNUbg Match ID using `neondojo_rules::backgammon::ids::MatchId`.
/// Falls back to a neutral centered-1-cube context if `match_id` is empty or
/// malformed (matching tests that pass `""`).
pub fn decode_match_context(match_id: &str) -> DecodedMatchContext {
    let Some(id) = MatchId::decode(match_id) else {
        return DecodedMatchContext::default();
    };
    let owner_bits = id.get(Field::CUBE_OWNER);
    let owner = match owner_bits {
        0 => Some(0),
        1 => Some(1),
        _ => None,
    };
    let log2 = id.get(Field::CUBE_LOG2).clamp(0, 15);
    DecodedMatchContext {
        scores: [id.get(Field::SCORE_0), id.get(Field::SCORE_1)],
        length: id.get(Field::LENGTH),
        jacoby: id.get(Field::NO_JACOBY) == 0,
        cube: 1 << log2,
        owner,
        crawford: id.get(Field::CRAWFORD) != 0,
        doubled: id.get(Field::DOUBLED) != 0,
    }
}

/// A single in-flight checker movement (`from`/`to` use absolute board
/// coordinates: `1..=24` points, `0` Black bar, `25` White bar, `-1` White off,
/// `26` Black off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckerMotion {
    pub from: i32,
    pub to: i32,
    pub player: u8,
}

/// Applies absolute checker steps `(from, to)` for `player` (`0` White, `1`
/// Black), sending hit blots to the opponent bar (`0` or `25`).
pub fn apply_absolute_steps(board: &[i32; 26], steps: &[(i32, i32)], player: u8) -> [i32; 26] {
    let mut out = *board;
    let sign = if player == 0 { 1 } else { -1 };
    let opp_bar = if player == 0 { 0usize } else { 25usize };
    for &(from, to) in steps {
        if (0..=25).contains(&from) {
            out[from as usize] -= sign;
        }
        if !(0..=25).contains(&to) {
            continue;
        }
        let to_idx = to as usize;
        if out[to_idx] == -sign {
            out[to_idx] = 0;
            out[opp_bar] -= sign;
        }
        out[to_idx] += sign;
    }
    out
}

/// Removes departing in-flight checkers from `before` so static stacks do not
/// duplicate moving pieces during flight (`play-animation.ts::underlayBoard`).
pub fn underlay_board(before: &[i32; 26], motions: &[CheckerMotion]) -> [i32; 26] {
    let mut board = *before;
    for motion in motions {
        if (0..=25).contains(&motion.from) {
            board[motion.from as usize] -= if motion.player == 0 { 1 } else { -1 };
        }
    }
    board
}

/// Computes borne-off tray counts adjusted for checkers currently departing the
/// off-tray (e.g. during an undo reversal animation).
pub fn tray_counts_for_frame(displayed_board: &[i32; 26], motions: &[CheckerMotion]) -> [i32; 2] {
    let mut counts = borne_off_counts(displayed_board);
    for motion in motions {
        if motion.from == -1 || motion.from == 26 {
            let idx = usize::from(motion.player != 0);
            counts[idx] = (counts[idx] - 1).max(0);
        }
    }
    counts
}

/// Center of the top visible checker (at most the 5th) on `point` (`1..=24`,
/// bar `0`/`25`, or off-tray `-1`/`26`) in logical coordinates.
pub fn checker_center(
    board: &[i32; 26],
    point: i32,
    player: u8,
    geom: &BoardGeometry,
) -> LogicalPoint {
    let off = point == -1 || point == 26;
    let count = if off {
        borne_off_counts(board)[usize::from(player != 0)]
    } else if (0..=25).contains(&point) {
        board[point as usize].abs()
    } else {
        1
    };
    let signed = (if player == 0 { 1 } else { -1 }) * count.max(1);
    let stack = point_stack_geometry(point, signed, geom);
    let dir = if stack.bottom { -1.0 } else { 1.0 };
    LogicalPoint::new(
        stack.x + stack.width / 2.0,
        stack.y
            + stack.height / 2.0
            + dir * ((stack.visible.saturating_sub(1)) as f32) * stack.step,
    )
}

/// Preview display mode for Analysis / Study / Play history review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoardViewMode {
    #[default]
    Before,
    Arrows,
    After,
}

/// One expanded single-die step in absolute coordinates (`25`/`0` bar, `-1`/`26`
/// off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovePreviewStep {
    pub from: i32,
    pub to: i32,
    pub die: i32,
    pub hit: bool,
}

/// Result of projecting a candidate or played checker move notation onto a
/// position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MovePreview {
    Ok {
        board: [i32; 26],
        steps: Vec<MovePreviewStep>,
        forced: bool,
    },
    Unavailable(String),
}

impl MovePreview {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }

    pub fn steps(&self) -> Option<&[MovePreviewStep]> {
        match self {
            Self::Ok { steps, .. } => Some(steps),
            Self::Unavailable(_) => None,
        }
    }

    pub fn after_board(&self) -> Option<&[i32; 26]> {
        match self {
            Self::Ok { board, .. } => Some(board),
            Self::Unavailable(_) => None,
        }
    }
}

/// Projects a legal checker move notation (e.g. `"24/20 13/8"`, `"bar/22* 6/4(2)"`,
/// `"13/6"`, `"Cannot move"`) using `neondojo-rules` without mutating the source
/// position.
pub fn project_move(board: &[i32; 26], player: u8, dice: &[u8], notation: &str) -> MovePreview {
    if player > 1 || rules_board::validate(board).is_err() {
        return MovePreview::Unavailable("The position cannot be projected safely.".into());
    }
    if dice.len() != 2 || dice.iter().any(|&d| !(1..=6).contains(&d)) {
        return MovePreview::Unavailable("A checker preview requires two valid dice.".into());
    }
    let dice_i32 = [i32::from(dice[0]), i32::from(dice[1])];
    let trimmed = notation.trim();
    if trimmed.is_empty() {
        return MovePreview::Unavailable("No move notation supplied.".into());
    }

    let Some(runs) = rules_notation::parse_steps(trimmed) else {
        return MovePreview::Unavailable("This notation is not a supported checker move.".into());
    };

    if runs.is_empty() {
        // "Cannot move" / "dance": verify it is genuinely a forced pass.
        return match checker_paths(board, i32::from(player), dice_i32) {
            Ok(paths) if paths.is_empty() => MovePreview::Ok {
                board: *board,
                steps: Vec::new(),
                forced: true,
            },
            _ => MovePreview::Unavailable("A checker can move; this is not a forced pass.".into()),
        };
    }

    let mut after_board = *board;
    let expanded = match expand_joined_play(&mut after_board, i32::from(player), dice_i32, &runs) {
        Ok(steps) => steps,
        Err(_) => {
            return MovePreview::Unavailable(
                "The notation does not describe a complete legal roll.".into(),
            );
        }
    };

    // Reconstruct per-step hit flags and exact die used (handling bear-off overshoots).
    let mut sim_local = rules_board::local_board(board, i32::from(player));
    let mut avail_dice: Vec<i32> = if dice_i32[0] == dice_i32[1] {
        vec![dice_i32[0]; 4]
    } else {
        let mut d = vec![dice_i32[0], dice_i32[1]];
        d.sort_unstable();
        d
    };

    let mut preview_steps = Vec::with_capacity(expanded.len());
    for (rel_from, rel_to) in expanded {
        let die = if rel_to > 0 {
            let needed = (rel_from - rel_to) as i32;
            if let Some(pos) = avail_dice.iter().position(|&d| d == needed) {
                avail_dice.remove(pos)
            } else {
                needed
            }
        } else {
            let exact = rel_from as i32;
            if let Some(pos) = avail_dice.iter().position(|&d| d == exact) {
                avail_dice.remove(pos)
            } else if let Some(pos) = avail_dice.iter().position(|&d| d > exact) {
                avail_dice.remove(pos)
            } else {
                exact
            }
        };

        let hit = rel_to > 0 && sim_local[rel_to] == -1;
        sim_local[rel_from] -= 1;
        if rel_to > 0 {
            if hit {
                sim_local[rel_to] = 0;
                sim_local[0] -= 1;
            }
            sim_local[rel_to] += 1;
        }

        let abs_from = if rel_from == 25 {
            if player == 0 { 25 } else { 0 }
        } else if player == 0 {
            rel_from as i32
        } else {
            25 - rel_from as i32
        };
        let abs_to = if rel_to == 0 {
            if player == 0 { -1 } else { 26 }
        } else if player == 0 {
            rel_to as i32
        } else {
            25 - rel_to as i32
        };

        preview_steps.push(MovePreviewStep {
            from: abs_from,
            to: abs_to,
            die,
            hit,
        });
    }

    MovePreview::Ok {
        board: after_board,
        steps: preview_steps,
        forced: false,
    }
}

/// Quadratic Bézier shaft + arrowhead triangle in logical coordinates for one
/// move preview step (`MoveArrows.tsx`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowGeometry {
    pub start: LogicalPoint,
    pub control: LogicalPoint,
    pub end: LogicalPoint,
    pub head_left: LogicalPoint,
    pub head_right: LogicalPoint,
    pub step: MovePreviewStep,
}

/// Computes quadratic Bézier curve and arrowhead points matching
/// `web/src/shared/board/MoveArrows.tsx`.
pub fn move_arrow_geometry(
    original: &[i32; 26],
    player: u8,
    steps: &[MovePreviewStep],
    geom: &BoardGeometry,
) -> Vec<ArrowGeometry> {
    let mut board = *original;
    let sign = if player == 0 { 1 } else { -1 };
    let opp_bar = if player == 0 { 0usize } else { 25usize };
    let (_, height) = geom.dimensions();
    let scale = geom.arrow_scale();

    steps
        .iter()
        .enumerate()
        .map(|(index, &step)| {
            let start = checker_center(&board, step.from, player, geom);
            if (0..=25).contains(&step.from) {
                board[step.from as usize] -= sign;
            }
            if (1..=24).contains(&step.to) {
                let idx = step.to as usize;
                if step.hit {
                    board[idx] = 0;
                    board[opp_bar] -= sign;
                }
                board[idx] += sign;
            }
            let end = checker_center(&board, step.to, player, geom);

            let same_half = (start.y < height / 2.0) == (end.y < height / 2.0);
            let bend = (18.0 + ((index % 3) as f32) * 8.0) * scale;
            let control = LogicalPoint::new(
                (start.x + end.x) / 2.0
                    + if same_half {
                        0.0
                    } else if index % 2 == 1 {
                        bend
                    } else {
                        -bend
                    },
                (start.y + end.y) / 2.0
                    + if same_half {
                        if start.y < height / 2.0 { bend } else { -bend }
                    } else {
                        0.0
                    },
            );
            let dx = end.x - control.x;
            let dy = end.y - control.y;
            let len = dx.hypot(dy).max(1e-6);
            let ux = dx / len;
            let uy = dy / len;
            let back_x = end.x - ux * 15.0 * scale;
            let back_y = end.y - uy * 15.0 * scale;
            let wing = 7.0 * scale;

            ArrowGeometry {
                start,
                control,
                end,
                head_left: LogicalPoint::new(back_x - uy * wing, back_y + ux * wing),
                head_right: LogicalPoint::new(back_x + uy * wing, back_y - ux * wing),
                step,
            }
        })
        .collect()
}

/// Computes the doubling cube rectangle in logical board coordinates
/// (`Board.tsx:382`, `BgBlitzBoard.tsx:138-153`).
pub fn cube_rect(
    geom: &BoardGeometry,
    owner: Option<u8>,
    bgblitz_cube_size: Option<(f32, f32)>,
) -> LogicalRect {
    match geom {
        BoardGeometry::BuiltIn => {
            let y = match owner {
                None => 220.0,
                Some(0) => 362.0,
                Some(_) => 78.0,
            };
            LogicalRect::new(325.0, y, 30.0, 30.0)
        }
        BoardGeometry::BgBlitz(m) => {
            let (cube_w, cube_h) = bgblitz_cube_size.unwrap_or((64.0, 64.0));
            let bar_x = m.columns[7] + m.white_checker_size.0 / 2.0;
            let configured_x = if owner == Some(0) {
                m.cube_right_x
            } else {
                m.cube_left_x
            };
            let cube_x = if owner.is_none() || configured_x == 0.0 {
                bar_x - cube_w / 2.0
            } else {
                configured_x
            };
            let cube_center_y = m.bg_height / 2.0
                + match owner {
                    None => m.cube_centered_y,
                    Some(0) => -m.cube_offset_y,
                    Some(_) => m.cube_offset_y,
                };
            LogicalRect::new(cube_x, cube_center_y - cube_h / 2.0, cube_w, cube_h)
        }
    }
}

/// Logical placement of a single die on the board.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiePlacement {
    pub index: usize,
    pub owner: u8,
    pub value: Option<u8>,
    pub rect: LogicalRect,
}

/// Computes logical rectangles for all visible dice (`PlayBoardDice.tsx`).
pub fn dice_placements(
    geom: &BoardGeometry,
    entries: &[(u8, Option<u8>)],
    side_by_owner: bool,
    split: bool,
) -> Vec<DiePlacement> {
    entries
        .iter()
        .enumerate()
        .map(|(index, &(owner, value))| {
            let (width, height, start_x, step_x, y) = match geom {
                BoardGeometry::BuiltIn => {
                    let left = owner == 1 && (side_by_owner || split);
                    (
                        33.0,
                        33.0,
                        if left { 185.0 } else { 420.0 },
                        if split { 0.0 } else { 45.0 },
                        219.0,
                    )
                }
                BoardGeometry::BgBlitz(m) => {
                    let (dw, dh) = if owner == 0 {
                        m.white_die_size
                    } else {
                        m.black_die_size
                    };
                    let left = owner == 1;
                    (
                        dw,
                        dh,
                        if left { m.dice_left_x } else { m.dice_right_x },
                        if split { 0.0 } else { dw * 1.25 },
                        (m.bg_height - dh) / 2.0,
                    )
                }
            };
            let x = start_x + (index as f32) * step_x;
            DiePlacement {
                index,
                owner,
                value,
                rect: LogicalRect::new(x, y, width, height),
            }
        })
        .collect()
}

/// Computes the two logical rectangles for BGBlitz point-number strips,
/// honoring both top/bottom rails (standard themes) and center-band numbers
/// (`cl_green`: `numbers_y = [682, 764]`) as well as `LR*` margin mirroring.
pub fn number_strip_rects(
    m: &BgBlitzMetrics,
    orientation: NumberOrientation,
    strip_sizes: [(f32, f32); 2],
) -> [LogicalRect; 2] {
    let outer_sum =
        m.columns[1] + m.columns[13] + 2.0 * m.checker_offset_x + m.white_checker_size.0;
    std::array::from_fn(|i| {
        let (sw, sh) = strip_sizes[i];
        let x = if orientation.is_left_to_right() {
            outer_sum - m.numbers_x - sw
        } else {
            m.numbers_x
        };
        LogicalRect::new(x, m.numbers_y[i], sw, sh)
    })
}

/// Interactive half-board point/bar/tray target rectangle in logical space
/// (`BoardPlayTargets.tsx`).
pub fn interactive_point_rects(geom: &BoardGeometry) -> Vec<(i32, LogicalRect)> {
    let (inset, height) = match geom {
        BoardGeometry::BuiltIn => (NATIVE_FIELD.y, NATIVE_HEIGHT),
        BoardGeometry::BgBlitz(m) => (m.bg_width * 0.028 + m.checker_offset_y, m.bg_height),
    };
    let top_y = (inset - 4.0).max(0.0);
    let bottom_y = height / 2.0 + 6.0;
    let target_h = (height / 2.0 - inset - 6.0).max(1.0);

    let mut rects = Vec::with_capacity(28);
    for pc in POINT_COLUMNS {
        let (x, w) = match geom {
            BoardGeometry::BuiltIn => {
                let idx = pc.column - 1 - usize::from(pc.column > 7);
                (
                    BoardGeometry::native_point_x(idx) - NATIVE_POINT_PITCH / 2.0,
                    NATIVE_POINT_PITCH,
                )
            }
            BoardGeometry::BgBlitz(m) => (
                m.columns[pc.column] + m.checker_offset_x - 2.0,
                m.white_checker_size.0 + 4.0,
            ),
        };
        rects.push((
            pc.point,
            LogicalRect::new(x, if pc.bottom { bottom_y } else { top_y }, w, target_h),
        ));
    }

    let (bar_x, bar_w) = match geom {
        BoardGeometry::BuiltIn => (
            NATIVE_WIDTH / 2.0 - NATIVE_POINT_PITCH / 2.0,
            NATIVE_POINT_PITCH,
        ),
        BoardGeometry::BgBlitz(m) => (
            m.columns[7] + m.checker_offset_x - 2.0,
            m.white_checker_size.0 + 4.0,
        ),
    };
    rects.push((0, LogicalRect::new(bar_x, top_y, bar_w, target_h)));
    rects.push((25, LogicalRect::new(bar_x, bottom_y, bar_w, target_h)));

    let (off_x, off_w) = match geom {
        BoardGeometry::BuiltIn => (NATIVE_OFF_TRAY.x, NATIVE_OFF_TRAY.width),
        BoardGeometry::BgBlitz(m) => (
            m.columns[14] + m.checker_offset_x - 2.0,
            m.white_checker_size.0 + 4.0,
        ),
    };
    rects.push((26, LogicalRect::new(off_x, top_y, off_w, target_h)));
    rects.push((-1, LogicalRect::new(off_x, bottom_y, off_w, target_h)));

    rects
}

/// Destination indicator dot center in logical coordinates (`BoardPlayTargets.tsx:118`).
pub fn destination_dot_center(rect: LogicalRect, bottom: bool) -> LogicalPoint {
    LogicalPoint::new(
        rect.x + rect.width / 2.0,
        rect.y + rect.height * if bottom { 0.45 } else { 0.55 },
    )
}

/// Screen-space prepaint hitboxes computed once per frame in `canvas()` prepaint
/// and queried during mouse move/click handling and headless tests.
#[derive(Clone, Debug, Default)]
pub struct BoardHitboxes {
    pub board_bounds: Bounds<Pixels>,
    pub point_targets: Vec<(i32, Bounds<Pixels>)>,
    pub source_hover_targets: Vec<(i32, Bounds<Pixels>)>,
    pub dice_pair_bounds: Option<Bounds<Pixels>>,
    pub cube_bounds: Option<Bounds<Pixels>>,
}

impl BoardHitboxes {
    /// Builds all screen-space hitboxes for the current frame.
    pub fn compute(
        geom: &BoardGeometry,
        transform: &BoardTransform,
        board: &[i32; 26],
        source_points: &[i32],
        dice_placements: &[DiePlacement],
        cube_logical: Option<LogicalRect>,
    ) -> Self {
        let point_targets = interactive_point_rects(geom)
            .into_iter()
            .map(|(pt, rect)| (pt, transform.map_rect(rect)))
            .collect();

        let source_hover_targets = source_points
            .iter()
            .filter_map(|&pt| {
                let count = if (0..=25).contains(&pt) {
                    board[pt as usize]
                } else {
                    0
                };
                let stack = point_stack_geometry(pt, count, geom);
                stack
                    .stack_bounds()
                    .map(|rect| (pt, transform.map_rect(rect)))
            })
            .collect();

        let dice_pair_bounds = if dice_placements.is_empty() {
            None
        } else {
            let left = dice_placements
                .iter()
                .map(|d| d.rect.x)
                .fold(f32::INFINITY, f32::min);
            let top = dice_placements
                .iter()
                .map(|d| d.rect.y)
                .fold(f32::INFINITY, f32::min);
            let right = dice_placements
                .iter()
                .map(|d| d.rect.right())
                .fold(f32::NEG_INFINITY, f32::max);
            let bottom = dice_placements
                .iter()
                .map(|d| d.rect.bottom())
                .fold(f32::NEG_INFINITY, f32::max);
            let union = LogicalRect::new(left, top, right - left, bottom - top + 8.0);
            Some(transform.map_rect(union))
        };

        let cube_bounds = cube_logical.map(|rect| transform.map_rect(rect));

        Self {
            board_bounds: transform.board_bounds(),
            point_targets,
            source_hover_targets,
            dice_pair_bounds,
            cube_bounds,
        }
    }

    pub fn hit_test_cube(&self, pos: Point<Pixels>) -> bool {
        self.cube_bounds.is_some_and(|b| b.contains(&pos))
    }

    pub fn hit_test_dice(&self, pos: Point<Pixels>) -> bool {
        self.dice_pair_bounds.is_some_and(|b| b.contains(&pos))
    }

    pub fn hit_test_source_hover(&self, pos: Point<Pixels>) -> Option<i32> {
        self.source_hover_targets
            .iter()
            .find(|(_, bounds)| bounds.contains(&pos))
            .map(|&(pt, _)| pt)
    }

    pub fn hit_test_point(&self, pos: Point<Pixels>) -> Option<i32> {
        self.point_targets
            .iter()
            .find(|(_, bounds)| bounds.contains(&pos))
            .map(|&(pt, _)| pt)
    }
}

/// Computes the target analysis board pane width using the same responsive
/// formula as `useAnalysisBoardResize.tsx` (`clamp(300 + w*0.22 + max(0, w-2560)*0.23, 320, 1680)`).
pub fn default_analysis_board_width(viewport_width: f32, max_available: f32) -> f32 {
    let (min_w, max_w) = neondojo_rules::options::analysis_board_width_bounds();
    let cap = max_available.min(max_w as f32).max(min_w as f32);
    let extra = (viewport_width - 2560.0).max(0.0);
    let raw = 300.0 + viewport_width * 0.22 + extra * 0.23;
    raw.clamp(min_w as f32, cap).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cl_green_metrics() -> BgBlitzMetrics {
        BgBlitzMetrics {
            bg_width: 2062.0,
            bg_height: 1477.0,
            columns: [
                59.0, 226.0, 349.0, 473.0, 596.0, 717.0, 840.0, 966.0, 1090.0, 1214.0, 1340.0,
                1462.0, 1585.0, 1711.0, 1876.0,
            ],
            checker_offset_x: 0.0,
            checker_offset_y: 15.0,
            white_checker_size: (120.0, 120.0),
            black_checker_size: (120.0, 120.0),
            white_die_size: (99.0, 99.0),
            black_die_size: (99.0, 99.0),
            dice_left_x: 452.0,
            dice_right_x: 1320.0,
            cube_left_x: 57.0,
            cube_right_x: 1872.0,
            cube_offset_y: -87.0,
            cube_centered_y: -4.0,
            numbers_x: 257.0,
            numbers_y: [682.0, 764.0],
        }
    }

    #[test]
    fn built_in_proportions_match_x22_and_zero_gap_five_checker_stacks() {
        let geom = BoardGeometry::BuiltIn;
        let p13 = point_stack_geometry(13, 5, &geom);
        let p14 = point_stack_geometry(14, 1, &geom);
        assert!(((p13.width / (p14.x - p13.x)) - (84.0 / 90.0)).abs() < 1e-6);
        assert!((p13.step - p13.height).abs() < 1e-6);

        // Verify <0.01px edge-to-edge gap across all 4 orientations and 3 desktop viewports.
        for &(vw, vh) in &[(1800.0, 1169.0), (2560.0, 1440.0), (3840.0, 2160.0)] {
            for human in [0u8, 1] {
                for dir in [BearoffDirection::Right, BearoffDirection::Left] {
                    let (fx, fy) = board_flips(human, dir);
                    let transform = BoardTransform::fit(
                        Bounds {
                            origin: point(px(0.0), px(0.0)),
                            size: size(px(vw), px(vh)),
                        },
                        NATIVE_WIDTH,
                        NATIVE_HEIGHT,
                        fx,
                        fy,
                    );
                    for pt in [13, 6, 25, 0, -1, 26] {
                        let count = if matches!(pt, 0 | 26) { -8 } else { 8 };
                        let stack = point_stack_geometry(pt, count, &geom);
                        assert_eq!(stack.visible, 5);
                        assert_eq!(stack.total, 8);
                        assert!(stack.badge_center().is_some());
                        let mut screen_boxes: Vec<(f32, f32)> = stack
                            .checker_rects()
                            .into_iter()
                            .map(|r| {
                                let b = transform.map_rect(r);
                                (
                                    f32::from(b.origin.y),
                                    f32::from(b.origin.y) + f32::from(b.size.height),
                                )
                            })
                            .collect();
                        screen_boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
                        for pair in screen_boxes.windows(2) {
                            let gap = (pair[1].0 - pair[0].1).abs();
                            assert!(gap < 0.01, "checker stack gap {gap} >= 0.01px on pt {pt}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn four_orientations_preserve_roundtrip_inverse_and_number_orientation() {
        let orientations = [
            (
                0u8,
                BearoffDirection::Right,
                (1.0, 1.0),
                NumberOrientation::Rld,
            ),
            (
                0u8,
                BearoffDirection::Left,
                (-1.0, 1.0),
                NumberOrientation::Lrd,
            ),
            (
                1u8,
                BearoffDirection::Right,
                (1.0, -1.0),
                NumberOrientation::Rlu,
            ),
            (
                1u8,
                BearoffDirection::Left,
                (-1.0, -1.0),
                NumberOrientation::Lru,
            ),
        ];
        for (human, dir, expected_flips, expected_num) in orientations {
            let (fx, fy) = board_flips(human, dir);
            assert_eq!((fx, fy), expected_flips);
            assert_eq!(NumberOrientation::from_orientation(fx, human), expected_num);

            let transform = BoardTransform::fit(
                Bounds {
                    origin: point(px(40.0), px(20.0)),
                    size: size(px(1000.0), px(700.0)),
                },
                NATIVE_WIDTH,
                NATIVE_HEIGHT,
                fx,
                fy,
            );
            let sample = LogicalPoint::new(150.0, 310.0);
            let screen = transform.map_point(sample);
            let back = transform.screen_to_logical(screen);
            assert!((back.x - sample.x).abs() < 1e-3);
            assert!((back.y - sample.y).abs() < 1e-3);
        }
    }

    #[test]
    fn cl_green_aspect_center_band_numbers_and_outer_rail_cube() {
        let m = cl_green_metrics();
        let geom = BoardGeometry::BgBlitz(m.clone());
        assert!((geom.aspect_ratio() - (2062.0 / 1477.0)).abs() < 1e-4);

        let rld = number_strip_rects(&m, NumberOrientation::Rld, [(1534.0, 25.0), (1529.0, 25.0)]);
        assert!((rld[0].x - 257.0).abs() < 1e-3);
        assert!((rld[0].y - 682.0).abs() < 1e-3);
        assert!((rld[1].y - 764.0).abs() < 1e-3);
        // Center-band numbers straddle the board midline (1477 / 2 = 738.5).
        assert!(rld[0].bottom() < m.bg_height / 2.0);
        assert!(rld[1].y > m.bg_height / 2.0);

        let lrd = number_strip_rects(&m, NumberOrientation::Lrd, [(1534.0, 25.0), (1529.0, 25.0)]);
        assert!(lrd[0].x > 0.0 && lrd[0].right() < m.bg_width);

        // Cube sits on outer left/right rails when owned, centered on bar when unowned.
        let white_cube = cube_rect(&geom, Some(0), Some((128.0, 128.0)));
        let black_cube = cube_rect(&geom, Some(1), Some((128.0, 128.0)));
        let centered_cube = cube_rect(&geom, None, Some((128.0, 128.0)));
        assert!((white_cube.x - 1872.0).abs() < 1e-3);
        assert!((black_cube.x - 57.0).abs() < 1e-3);
        assert!((centered_cube.center().x - (966.0 + 60.0)).abs() < 1e-3);
    }

    #[test]
    fn underlay_board_and_tray_counts_hide_departing_checkers() {
        let mut board = [0i32; 26];
        board[6] = 2;
        board[4] = -1;
        // White bears off 13 checkers already; now undoing from off-tray (-1 -> 6)
        let motions = vec![CheckerMotion {
            from: -1,
            to: 6,
            player: 0,
        }];
        let tray = tray_counts_for_frame(&board, &motions);
        assert_eq!(tray[0], 12); // 15 - 2 - 1 departing off-tray
        assert_eq!(tray[1], 14);

        let hit_motions = vec![
            CheckerMotion {
                from: 6,
                to: 4,
                player: 0,
            },
            CheckerMotion {
                from: 4,
                to: 0,
                player: 1,
            },
        ];
        let under = underlay_board(&board, &hit_motions);
        assert_eq!(under[6], 1);
        assert_eq!(under[4], 0);
    }

    #[test]
    fn project_move_and_arrows_cover_joined_runs_hits_bar_bearoff_and_forced_pass() {
        let mut board = [0i32; 26];
        board[13] = 2;
        board[7] = -1;
        board[1] = -1;
        let preview = project_move(&board, 0, &[6, 1], "13/7*/6");
        match preview {
            MovePreview::Ok {
                board: after,
                steps,
                forced,
            } => {
                assert!(!forced);
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].from, 13);
                assert_eq!(steps[0].to, 7);
                assert!(steps[0].hit);
                assert_eq!(steps[1].from, 7);
                assert_eq!(steps[1].to, 6);
                assert_eq!(after[0], -1);
                assert_eq!(after[6], 1);
                let arrows = move_arrow_geometry(&board, 0, &steps, &BoardGeometry::BuiltIn);
                assert_eq!(arrows.len(), 2);
                assert!(arrows[0].start.x > 0.0 && arrows[0].end.y < NATIVE_HEIGHT);
            }
            MovePreview::Unavailable(err) => panic!("unexpected failure: {err}"),
        }

        // Forced pass when white is closed out on the bar.
        let mut closed = [0i32; 26];
        closed[25] = 1;
        for slot in &mut closed[19..=24] {
            *slot = -2;
        }
        let pass = project_move(&closed, 0, &[6, 5], "Cannot move");
        assert!(matches!(pass, MovePreview::Ok { forced: true, .. }));
    }

    #[test]
    fn match_context_decoding_matches_reference_ids() {
        let m1 = decode_match_context("QYkqASAAIAAE");
        assert_eq!(m1.length, 9);
        assert!(!m1.jacoby);
        assert_eq!(m1.scores, [2, 4]);
        assert_eq!(m1.cube, 2);
        assert_eq!(m1.owner, Some(0));
        assert!(!m1.crawford);

        let money = decode_match_context("cIkFAAAAAAAA");
        assert_eq!(money.length, 0);
        assert!(money.jacoby);
    }

    #[test]
    fn hitboxes_hit_points_sources_dice_union_and_cube() {
        let geom = BoardGeometry::BuiltIn;
        let transform = BoardTransform::fit(
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(680.0), px(470.0)),
            },
            NATIVE_WIDTH,
            NATIVE_HEIGHT,
            1.0,
            1.0,
        );
        let board = rules_board::initial_board();
        let dice = dice_placements(&geom, &[(0, Some(3)), (0, Some(1))], true, false);
        let cube = cube_rect(&geom, None, None);
        let hitboxes =
            BoardHitboxes::compute(&geom, &transform, &board, &[6, 13], &dice, Some(cube));
        assert_eq!(hitboxes.point_targets.len(), 28);
        assert_eq!(hitboxes.source_hover_targets.len(), 2);

        // Clicking the gap between the two dice hits the dice pair union rect.
        let gap_x = (dice[0].rect.right() + dice[1].rect.x) / 2.0;
        let gap_y = dice[0].rect.center().y;
        assert!(hitboxes.hit_test_dice(transform.map_point(LogicalPoint::new(gap_x, gap_y))));
        assert!(hitboxes.hit_test_cube(transform.map_point(cube.center())));

        let p6_center = checker_center(&board, 6, 0, &geom);
        let p6_screen = transform.map_point(p6_center);
        assert_eq!(hitboxes.hit_test_source_hover(p6_screen), Some(6));
        assert_eq!(hitboxes.hit_test_point(p6_screen), Some(6));
    }
}
