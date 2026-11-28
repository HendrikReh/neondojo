//! Built-in board themes (9 surfaces, 6 embedded SVGs, 24 WebP checkers, 9 WebP
//! cubes), direct Rust BGBlitz ZIP theme loader with full `prepare_themes.py`
//! security parity, Rec.709 luminance light/dark assignment, and GPUI
//! `RenderImage` texture cache (`RGBA → BGRA` swap, quantized `resvg` surface
//! cache, `~320×216` gallery thumbnails, and LRU eviction via `cx.drop_image`).

use crate::geometry::{BgBlitzMetrics, BoardGeometry, NumberOrientation};
use anyhow::{Context, Result, anyhow, bail, ensure};
use gpui_kit::{App, Hsla, RenderImage, Window, rgb};
use image::{Frame, RgbaImage, imageops};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::{HashMap, VecDeque, hash_map::Entry};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use zip::ZipArchive;

/// Special `dice_theme` value meaning "follow the active board theme"
/// (`shared/configuration-options.json::default_dice_theme`).
pub const MATCH_BOARD_DICE: &str = "match-board";

// Security limits mirrored 1:1 from `tools/prepare_themes.py`.
pub const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: usize = 2_000;
pub const MAX_XML_BYTES: usize = 1024 * 1024;
pub const MAX_IMAGE_DIM: u32 = 8_192;
pub const MAX_COORD: i32 = 8_192;
pub const MAX_CHECKER_PIXELS: u64 = 1_024 * 1_024;
pub const MAX_CHECKER_VARIANTS: usize = 32;

/// Gallery thumbnail target width in physical pixels (`~320×216` for 1.48 ratio).
pub const GALLERY_THUMB_MAX_WIDTH: u32 = 320;
/// Rasterized surface texture dimensions for built-in gallery previews
/// (matches `NATIVE_FIELD` 534×414 ratio at crisp Retina resolution).
const BUILTIN_GALLERY_SURFACE_W: u32 = 320;
const BUILTIN_GALLERY_SURFACE_H: u32 = 248;

/// Cached GPU textures for rendering a full built-in board preview in
/// `BoardGalleryTile` without disturbing `ThemeAtlasCache` LRU slots.
#[derive(Clone)]
pub struct BuiltInGalleryAssets {
    pub surface: Option<Arc<RenderImage>>,
    pub white_checker: Option<Arc<RenderImage>>,
    pub black_checker: Option<Arc<RenderImage>>,
    pub cube: Option<Arc<RenderImage>>,
}

static BUILTIN_GALLERY_CACHE: LazyLock<Mutex<HashMap<BuiltInThemeId, BuiltInGalleryAssets>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns cached gallery preview textures for a built-in board theme,
/// decoding embedded WebP sprites and rasterizing any SVG surface once.
pub fn builtin_gallery_assets(theme: BuiltInThemeId) -> BuiltInGalleryAssets {
    let mut cache = BUILTIN_GALLERY_CACHE
        .lock()
        .expect("builtin gallery cache lock");
    cache
        .entry(theme)
        .or_insert_with(|| {
            let surface = theme.surface_svg().and_then(|svg| {
                rasterize_surface_svg(svg, BUILTIN_GALLERY_SURFACE_W, BUILTIN_GALLERY_SURFACE_H)
                    .ok()
            });
            let white_checker = theme.checker_webps(true).and_then(|[b0, _]| {
                image::load_from_memory(b0)
                    .ok()
                    .map(|img| render_image_from_rgba(img.to_rgba8()))
            });
            let black_checker = theme.checker_webps(false).and_then(|[b0, _]| {
                image::load_from_memory(b0)
                    .ok()
                    .map(|img| render_image_from_rgba(img.to_rgba8()))
            });
            let cube = image::load_from_memory(theme.cube_webp())
                .ok()
                .map(|img| render_image_from_rgba(img.to_rgba8()));
            BuiltInGalleryAssets {
                surface,
                white_checker,
                black_checker,
                cube,
            }
        })
        .clone()
}
/// Quantization bucket for rasterized `resvg` surface textures.
pub const SURFACE_RASTER_BUCKET_PX: u32 = 64;
/// Default max resident full-resolution BGBlitz themes in GPU atlas memory.
pub const DEFAULT_MAX_RESIDENT_BGBLITZ: usize = 3;

// Embedded built-in SVG surfaces (6 textured finishes).
const SURFACE_MOONSTONE_SVG: &[u8] = include_bytes!("../assets/surfaces/moonstone.svg");
const SURFACE_VIRIDIAN_SVG: &[u8] = include_bytes!("../assets/surfaces/viridian.svg");
const SURFACE_TERRA_ROSSA_SVG: &[u8] = include_bytes!("../assets/surfaces/terra-rossa.svg");
const SURFACE_ION_SVG: &[u8] = include_bytes!("../assets/surfaces/ion.svg");
const SURFACE_PELAGIC_SVG: &[u8] = include_bytes!("../assets/surfaces/pelagic.svg");
const SURFACE_CARBON_SVG: &[u8] = include_bytes!("../assets/surfaces/carbon.svg");

// Embedded built-in 192×192 WebP checkers (24 files).
const CHECKER_MOONSTONE_W0: &[u8] = include_bytes!("../assets/checkers/moonstone-white-0.webp");
const CHECKER_MOONSTONE_W1: &[u8] = include_bytes!("../assets/checkers/moonstone-white-1.webp");
const CHECKER_MOONSTONE_B0: &[u8] = include_bytes!("../assets/checkers/moonstone-black-0.webp");
const CHECKER_MOONSTONE_B1: &[u8] = include_bytes!("../assets/checkers/moonstone-black-1.webp");
const CHECKER_VIRIDIAN_W0: &[u8] = include_bytes!("../assets/checkers/viridian-white-0.webp");
const CHECKER_VIRIDIAN_W1: &[u8] = include_bytes!("../assets/checkers/viridian-white-1.webp");
const CHECKER_VIRIDIAN_B0: &[u8] = include_bytes!("../assets/checkers/viridian-black-0.webp");
const CHECKER_VIRIDIAN_B1: &[u8] = include_bytes!("../assets/checkers/viridian-black-1.webp");
const CHECKER_TERRA_W0: &[u8] = include_bytes!("../assets/checkers/terra-rossa-white-0.webp");
const CHECKER_TERRA_W1: &[u8] = include_bytes!("../assets/checkers/terra-rossa-white-1.webp");
const CHECKER_TERRA_B0: &[u8] = include_bytes!("../assets/checkers/terra-rossa-black-0.webp");
const CHECKER_TERRA_B1: &[u8] = include_bytes!("../assets/checkers/terra-rossa-black-1.webp");
const CHECKER_ION_W0: &[u8] = include_bytes!("../assets/checkers/ion-white-0.webp");
const CHECKER_ION_W1: &[u8] = include_bytes!("../assets/checkers/ion-white-1.webp");
const CHECKER_ION_B0: &[u8] = include_bytes!("../assets/checkers/ion-black-0.webp");
const CHECKER_ION_B1: &[u8] = include_bytes!("../assets/checkers/ion-black-1.webp");
const CHECKER_PELAGIC_W0: &[u8] = include_bytes!("../assets/checkers/pelagic-white-0.webp");
const CHECKER_PELAGIC_W1: &[u8] = include_bytes!("../assets/checkers/pelagic-white-1.webp");
const CHECKER_PELAGIC_B0: &[u8] = include_bytes!("../assets/checkers/pelagic-black-0.webp");
const CHECKER_PELAGIC_B1: &[u8] = include_bytes!("../assets/checkers/pelagic-black-1.webp");
const CHECKER_CARBON_W0: &[u8] = include_bytes!("../assets/checkers/carbon-white-0.webp");
const CHECKER_CARBON_W1: &[u8] = include_bytes!("../assets/checkers/carbon-white-1.webp");
const CHECKER_CARBON_B0: &[u8] = include_bytes!("../assets/checkers/carbon-black-0.webp");
const CHECKER_CARBON_B1: &[u8] = include_bytes!("../assets/checkers/carbon-black-1.webp");

// Embedded built-in 192×192 WebP cubes (9 files).
const CUBE_FOREST: &[u8] = include_bytes!("../assets/cubes/forest.webp");
const CUBE_WALNUT: &[u8] = include_bytes!("../assets/cubes/walnut.webp");
const CUBE_SLATE: &[u8] = include_bytes!("../assets/cubes/slate.webp");
const CUBE_MOONSTONE: &[u8] = include_bytes!("../assets/cubes/moonstone.webp");
const CUBE_VIRIDIAN: &[u8] = include_bytes!("../assets/cubes/viridian.webp");
const CUBE_TERRA_ROSSA: &[u8] = include_bytes!("../assets/cubes/terra-rossa.webp");
const CUBE_ION: &[u8] = include_bytes!("../assets/cubes/ion.webp");
const CUBE_PELAGIC: &[u8] = include_bytes!("../assets/cubes/pelagic.webp");
const CUBE_CARBON: &[u8] = include_bytes!("../assets/cubes/carbon.webp");

/// Canonical identifiers for the 9 built-in board themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltInThemeId {
    Forest,
    Walnut,
    Slate,
    Moonstone,
    Viridian,
    TerraRossa,
    Ion,
    Pelagic,
    Carbon,
}

impl BuiltInThemeId {
    pub const ALL: [Self; 9] = [
        Self::Forest,
        Self::Walnut,
        Self::Slate,
        Self::Moonstone,
        Self::Viridian,
        Self::TerraRossa,
        Self::Ion,
        Self::Pelagic,
        Self::Carbon,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Walnut => "walnut",
            Self::Slate => "slate",
            Self::Moonstone => "moonstone",
            Self::Viridian => "viridian",
            Self::TerraRossa => "terra-rossa",
            Self::Ion => "ion",
            Self::Pelagic => "pelagic",
            Self::Carbon => "carbon",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Forest => "Forest",
            Self::Walnut => "Walnut",
            Self::Slate => "Slate",
            Self::Moonstone => "Moonstone",
            Self::Viridian => "Viridian",
            Self::TerraRossa => "Terra Rossa",
            Self::Ion => "Ion",
            Self::Pelagic => "Pelagic",
            Self::Carbon => "Carbon",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "forest" => Some(Self::Forest),
            "walnut" => Some(Self::Walnut),
            "slate" => Some(Self::Slate),
            "moonstone" => Some(Self::Moonstone),
            "viridian" => Some(Self::Viridian),
            "terra-rossa" => Some(Self::TerraRossa),
            "ion" => Some(Self::Ion),
            "pelagic" => Some(Self::Pelagic),
            "carbon" => Some(Self::Carbon),
            _ => None,
        }
    }

    pub const fn surface_svg(self) -> Option<&'static [u8]> {
        match self {
            Self::Forest | Self::Walnut | Self::Slate => None,
            Self::Moonstone => Some(SURFACE_MOONSTONE_SVG),
            Self::Viridian => Some(SURFACE_VIRIDIAN_SVG),
            Self::TerraRossa => Some(SURFACE_TERRA_ROSSA_SVG),
            Self::Ion => Some(SURFACE_ION_SVG),
            Self::Pelagic => Some(SURFACE_PELAGIC_SVG),
            Self::Carbon => Some(SURFACE_CARBON_SVG),
        }
    }

    pub const fn checker_webps(self, white: bool) -> Option<[&'static [u8]; 2]> {
        match (self, white) {
            (Self::Forest | Self::Walnut | Self::Slate, _) => None,
            (Self::Moonstone, true) => Some([CHECKER_MOONSTONE_W0, CHECKER_MOONSTONE_W1]),
            (Self::Moonstone, false) => Some([CHECKER_MOONSTONE_B0, CHECKER_MOONSTONE_B1]),
            (Self::Viridian, true) => Some([CHECKER_VIRIDIAN_W0, CHECKER_VIRIDIAN_W1]),
            (Self::Viridian, false) => Some([CHECKER_VIRIDIAN_B0, CHECKER_VIRIDIAN_B1]),
            (Self::TerraRossa, true) => Some([CHECKER_TERRA_W0, CHECKER_TERRA_W1]),
            (Self::TerraRossa, false) => Some([CHECKER_TERRA_B0, CHECKER_TERRA_B1]),
            (Self::Ion, true) => Some([CHECKER_ION_W0, CHECKER_ION_W1]),
            (Self::Ion, false) => Some([CHECKER_ION_B0, CHECKER_ION_B1]),
            (Self::Pelagic, true) => Some([CHECKER_PELAGIC_W0, CHECKER_PELAGIC_W1]),
            (Self::Pelagic, false) => Some([CHECKER_PELAGIC_B0, CHECKER_PELAGIC_B1]),
            (Self::Carbon, true) => Some([CHECKER_CARBON_W0, CHECKER_CARBON_W1]),
            (Self::Carbon, false) => Some([CHECKER_CARBON_B0, CHECKER_CARBON_B1]),
        }
    }

    pub const fn cube_webp(self) -> &'static [u8] {
        match self {
            Self::Forest => CUBE_FOREST,
            Self::Walnut => CUBE_WALNUT,
            Self::Slate => CUBE_SLATE,
            Self::Moonstone => CUBE_MOONSTONE,
            Self::Viridian => CUBE_VIRIDIAN,
            Self::TerraRossa => CUBE_TERRA_ROSSA,
            Self::Ion => CUBE_ION,
            Self::Pelagic => CUBE_PELAGIC,
            Self::Carbon => CUBE_CARBON,
        }
    }

    /// Exact color specification matching `board.css`, `board-finishes.css`,
    /// and `ThemedDie.tsx`.
    pub const fn palette(self) -> BuiltInBoardPalette {
        match self {
            Self::Forest => BuiltInBoardPalette {
                frame: 0x234b3d,
                felt: 0xe3dec7,
                bar: 0x234b3d,
                point_light: 0xc0c9a5,
                point_dark: 0x607e63,
                white_checker: 0xf8f4e2,
                black_checker: 0x284e3e,
                inlay: None,
                point_number: 0xd9e0c9,
                cube_numeral: 0x342b24,
                die_light: 0xf7f4e7,
                die_dark: 0x205b48,
            },
            Self::Walnut => BuiltInBoardPalette {
                frame: 0x573d2f,
                felt: 0xe3cda5,
                bar: 0x573d2f,
                point_light: 0xd4af78,
                point_dark: 0x955e40,
                white_checker: 0xfff2da,
                black_checker: 0x593d30,
                inlay: None,
                point_number: 0xd9e0c9,
                cube_numeral: 0x342b24,
                die_light: 0xfff2d9,
                die_dark: 0x603323,
            },
            Self::Slate => BuiltInBoardPalette {
                frame: 0x354855,
                felt: 0xd2dde0,
                bar: 0x354855,
                point_light: 0xb0c2c7,
                point_dark: 0x6a8b9a,
                white_checker: 0xf1f8f8,
                black_checker: 0x344c5c,
                inlay: None,
                point_number: 0xd9e0c9,
                cube_numeral: 0xf5f4eb,
                die_light: 0xedf9fc,
                die_dark: 0x273747,
            },
            Self::Moonstone => BuiltInBoardPalette {
                frame: 0x394858,
                felt: 0xadbdc7,
                bar: 0x475666,
                point_light: 0xdce7eb,
                point_dark: 0x718499,
                white_checker: 0xe6eff7,
                black_checker: 0x28374b,
                inlay: Some(0xcfdae2),
                point_number: 0xf1f1e8,
                cube_numeral: 0xf5f4eb,
                die_light: 0xfaf8ff,
                die_dark: 0x354e91,
            },
            Self::Viridian => BuiltInBoardPalette {
                frame: 0x283e34,
                felt: 0x39725e,
                bar: 0x354b3c,
                point_light: 0xa9ca9e,
                point_dark: 0x1e4939,
                white_checker: 0xfff1ce,
                black_checker: 0x183e32,
                inlay: Some(0xe0ba78),
                point_number: 0xf1f1e8,
                cube_numeral: 0x342b24,
                die_light: 0xf5f8dc,
                die_dark: 0x008345,
            },
            Self::TerraRossa => BuiltInBoardPalette {
                frame: 0x603d33,
                felt: 0xdfb88a,
                bar: 0x674538,
                point_light: 0xf5ddae,
                point_dark: 0xa5573e,
                white_checker: 0xf5dbb7,
                black_checker: 0x643727,
                inlay: Some(0xf1b785),
                point_number: 0xf1f1e8,
                cube_numeral: 0x342b24,
                die_light: 0xfff1cf,
                die_dark: 0xba101c,
            },
            Self::Ion => BuiltInBoardPalette {
                frame: 0x1d233c,
                felt: 0x292d55,
                bar: 0x252844,
                point_light: 0x65b9c9,
                point_dark: 0x927ac2,
                white_checker: 0xf5c6dc,
                black_checker: 0x355983,
                inlay: Some(0x9ccedc),
                point_number: 0xf1f1e8,
                cube_numeral: 0xf5f4eb,
                die_light: 0xf5e6fa,
                die_dark: 0x504197,
            },
            Self::Pelagic => BuiltInBoardPalette {
                frame: 0x163948,
                felt: 0x34677b,
                bar: 0x204252,
                point_light: 0x9ccfc8,
                point_dark: 0x1c435d,
                white_checker: 0xf3eedf,
                black_checker: 0x233b5c,
                inlay: Some(0xa6c8c5),
                point_number: 0xf1f1e8,
                cube_numeral: 0xf5f4eb,
                die_light: 0xe3f6f4,
                die_dark: 0x075b69,
            },
            Self::Carbon => BuiltInBoardPalette {
                frame: 0x26292c,
                felt: 0x34393c,
                bar: 0x323538,
                point_light: 0xb1b6b6,
                point_dark: 0xc57154,
                white_checker: 0xf4ccb0,
                black_checker: 0x212529,
                inlay: Some(0xd59072),
                point_number: 0xf1f1e8,
                cube_numeral: 0xf5f4eb,
                die_light: 0xf3f0e9,
                die_dark: 0x222229,
            },
        }
    }
}

/// RGB hex palette for a built-in board theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltInBoardPalette {
    pub frame: u32,
    pub felt: u32,
    pub bar: u32,
    pub point_light: u32,
    pub point_dark: u32,
    pub white_checker: u32,
    pub black_checker: u32,
    pub inlay: Option<u32>,
    pub point_number: u32,
    pub cube_numeral: u32,
    pub die_light: u32,
    pub die_dark: u32,
}

impl BuiltInBoardPalette {
    pub fn rail_highlight(self) -> Hsla {
        match self.inlay {
            Some(inlay) => srgb_mix(self.frame, inlay, 0.23),
            None => rgb(self.frame).into(),
        }
    }

    pub fn rail_shadow(self) -> Hsla {
        match self.inlay {
            Some(_) => srgb_mix(self.frame, 0x000000, 0.32),
            None => rgb(self.frame).into(),
        }
    }

    pub fn die_edge_stops(self, light: bool) -> (Hsla, Hsla) {
        let base = if light { self.die_light } else { self.die_dark };
        (
            srgb_mix(base, 0xffffff, 0.52),
            srgb_mix(base, 0x000000, 0.44),
        )
    }

    pub fn die_face_stops(self, light: bool) -> (Hsla, Hsla) {
        let base = if light { self.die_light } else { self.die_dark };
        if light {
            (
                srgb_mix(base, 0xffffff, 0.16),
                srgb_mix(base, 0x000000, 0.12),
            )
        } else {
            (
                srgb_mix(base, 0x000000, 0.16),
                srgb_mix(base, 0xffffff, 0.12),
            )
        }
    }
}

/// Linear sRGB `color-mix(in srgb, base, other ratio)`.
pub fn srgb_mix(base: u32, other: u32, ratio: f32) -> Hsla {
    let r = ratio.clamp(0.0, 1.0);
    let mix_ch = |a: u32, b: u32| -> u32 {
        (((a as f32) * (1.0 - r) + (b as f32) * r).round() as u32).min(255)
    };
    let red = mix_ch((base >> 16) & 0xff, (other >> 16) & 0xff);
    let green = mix_ch((base >> 8) & 0xff, (other >> 8) & 0xff);
    let blue = mix_ch(base & 0xff, other & 0xff);
    rgb((red << 16) | (green << 8) | blue).into()
}

/// Independent surface / checker / cube / dice theme configuration matching
/// `shared/configuration-options.json` & `BoardSettingsSection.tsx`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardThemeConfig {
    pub board_theme: String,
    pub checker_theme: Option<String>,
    pub cube_theme: Option<String>,
    pub dice_theme: Option<String>,
}

impl Default for BoardThemeConfig {
    fn default() -> Self {
        Self {
            board_theme: BuiltInThemeId::Forest.as_str().to_string(),
            checker_theme: None,
            cube_theme: None,
            dice_theme: Some(MATCH_BOARD_DICE.to_string()),
        }
    }
}

impl BoardThemeConfig {
    pub fn resolved_checker_id(&self) -> &str {
        self.checker_theme
            .as_deref()
            .unwrap_or(self.board_theme.as_str())
    }

    pub fn resolved_cube_id(&self) -> &str {
        self.cube_theme
            .as_deref()
            .unwrap_or(self.board_theme.as_str())
    }

    pub fn resolved_dice_id(&self) -> &str {
        match self.dice_theme.as_deref() {
            Some(MATCH_BOARD_DICE) => self.board_theme.as_str(),
            Some(explicit) if !explicit.is_empty() => explicit,
            _ => self.resolved_checker_id(),
        }
    }
}

/// Swaps RGBA byte order to GPUI Metal's required BGRA byte order in place.
pub fn rgba_to_bgra(pixels: &mut [u8]) {
    let (chunks, _) = pixels.as_chunks_mut::<4>();
    for px in chunks {
        px.swap(0, 2);
    }
}

/// Builds a GPUI `RenderImage` from straight RGBA pixels by performing the
/// required `RGBA → BGRA` channel swap.
pub fn render_image_from_rgba(mut img: RgbaImage) -> Arc<RenderImage> {
    rgba_to_bgra(img.as_mut());
    Arc::new(RenderImage::new([Frame::new(img)]))
}

/// Builds a GPUI `RenderImage` with every pixel's alpha multiplied by `factor`
/// (used for `0.28`-opacity used dice and half-used double dice).
pub fn render_image_with_alpha(source: &RgbaImage, factor: f32) -> Arc<RenderImage> {
    let mut img = source.clone();
    let f = factor.clamp(0.0, 1.0);
    for px in img.pixels_mut() {
        px.0[3] = (f32::from(px.0[3]) * f).round() as u8;
    }
    render_image_from_rgba(img)
}

/// Flips an RGBA image horizontally and/or vertically and uploads as a BGRA
/// `RenderImage`.
pub fn render_image_flipped(source: &RgbaImage, flip_x: bool, flip_y: bool) -> Arc<RenderImage> {
    let mut img = source.clone();
    if flip_x {
        imageops::flip_horizontal_in_place(&mut img);
    }
    if flip_y {
        imageops::flip_vertical_in_place(&mut img);
    }
    render_image_from_rgba(img)
}

/// Quantizes a pixel dimension up to the nearest `SURFACE_RASTER_BUCKET_PX`
/// bucket (`64..=4096`) to keep the `resvg` cache bounded during window resize.
pub fn quantize_surface_dim(pixels: u32) -> u32 {
    let b = SURFACE_RASTER_BUCKET_PX;
    pixels.max(b).div_ceil(b).saturating_mul(b).clamp(b, 4096)
}

/// Rasterizes one of the 6 built-in full-color surface SVGs via `resvg` into a
/// BGRA GPUI `RenderImage` at `(width, height)` pixels.
pub fn rasterize_surface_svg(
    svg_bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<Arc<RenderImage>> {
    let w = width.clamp(1, 4096);
    let h = height.clamp(1, 4096);
    let tree = resvg::usvg::Tree::from_data(svg_bytes, &resvg::usvg::Options::default())
        .context("parse built-in surface SVG")?;
    let svg_size = tree.size();
    let sx = w as f32 / svg_size.width().max(1.0);
    let sy = h as f32 / svg_size.height().max(1.0);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow!("failed to allocate {w}x{h} pixmap for surface SVG"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    // tiny_skia outputs premultiplied RGBA; convert to straight RGBA then BGRA.
    let mut rgba = pixmap.take();
    let (chunks, _) = rgba.as_chunks_mut::<4>();
    for px in chunks {
        let a = px[3];
        if a > 0 && a < 255 {
            let inv = 255.0 / f32::from(a);
            px[0] = (f32::from(px[0]) * inv).round() as u8;
            px[1] = (f32::from(px[1]) * inv).round() as u8;
            px[2] = (f32::from(px[2]) * inv).round() as u8;
        }
        px.swap(0, 2); // RGBA -> BGRA
    }
    let img = RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| anyhow!("surface SVG buffer size mismatch"))?;
    Ok(Arc::new(RenderImage::new([Frame::new(img)])))
}

/// Mean Rec.709 relative luminance (`0.0..=255.0`) of opaque pixels (`alpha >=
/// 128`), matching `tools/prepare_themes.py::png_luminance`.
pub fn mean_opaque_luminance(img: &RgbaImage) -> Result<f64> {
    let (w, h) = img.dimensions();
    ensure!(
        u64::from(w) * u64::from(h) <= MAX_CHECKER_PIXELS,
        "Checker artwork has more than 1024 x 1024 pixels"
    );
    let mut total = 0.0_f64;
    let mut weight = 0u64;
    for px in img.pixels() {
        let [r, g, b, a] = px.0;
        if a >= 128 {
            total += 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b);
            weight += 1;
        }
    }
    ensure!(weight > 0, "Checker artwork has no opaque pixels");
    Ok(total / weight as f64)
}

/// Lightweight metadata + gallery thumbnail for a discovered BGBlitz ZIP theme.
#[derive(Clone)]
pub struct BgBlitzThemeEntry {
    pub id: String,
    pub name: String,
    pub zip_path: PathBuf,
    pub metrics: BgBlitzMetrics,
    pub thumbnail_bg: Arc<RenderImage>,
    pub thumbnail_white_checker: Arc<RenderImage>,
    pub thumbnail_black_checker: Arc<RenderImage>,
}

/// Fully decoded BGBlitz theme held in CPU RGBA memory before/during GPU upload.
#[derive(Clone)]
pub struct DecodedBgBlitzTheme {
    pub id: String,
    pub name: String,
    pub metrics: BgBlitzMetrics,
    pub background: RgbaImage,
    pub white_checkers: Vec<RgbaImage>,
    pub black_checkers: Vec<RgbaImage>,
    pub white_dice: [RgbaImage; 6],
    pub black_dice: [RgbaImage; 6],
    pub cubes: HashMap<String, RgbaImage>,
    pub numbers: HashMap<NumberOrientation, [RgbaImage; 2]>,
}

impl DecodedBgBlitzTheme {
    /// Generates a downscaled `~320×216` BGRA gallery thumbnail without keeping
    /// the full `1490×988`/`2062×1477` background texture in VRAM.
    pub fn build_gallery_entry(&self, zip_path: PathBuf) -> BgBlitzThemeEntry {
        let (bw, bh) = self.background.dimensions();
        let tw = GALLERY_THUMB_MAX_WIDTH.min(bw).max(1);
        let th = ((u64::from(bh) * u64::from(tw)) / u64::from(bw.max(1))) as u32;
        let thumb_bg = imageops::resize(
            &self.background,
            tw,
            th.max(1),
            imageops::FilterType::Triangle,
        );
        let thumb_w = imageops::resize(
            &self.white_checkers[0],
            48,
            48,
            imageops::FilterType::Triangle,
        );
        let thumb_b = imageops::resize(
            &self.black_checkers[0],
            48,
            48,
            imageops::FilterType::Triangle,
        );
        BgBlitzThemeEntry {
            id: self.id.clone(),
            name: self.name.clone(),
            zip_path,
            metrics: self.metrics.clone(),
            thumbnail_bg: render_image_from_rgba(thumb_bg),
            thumbnail_white_checker: render_image_from_rgba(thumb_w),
            thumbnail_black_checker: render_image_from_rgba(thumb_b),
        }
    }
}

/// GPU sprite paired with its intrinsic pixel dimensions `(width, height)`.
pub type SizedSprite = (Arc<RenderImage>, (f32, f32));

/// GPU-ready BGRA `RenderImage` bundle for an active BGBlitz theme.
pub struct BgBlitzGpuTheme {
    pub id: String,
    pub name: String,
    pub metrics: BgBlitzMetrics,
    pub background_raw: RgbaImage,
    pub backgrounds: HashMap<(bool, bool), Arc<RenderImage>>,
    pub white_checkers: Vec<SizedSprite>,
    pub black_checkers: Vec<SizedSprite>,
    pub white_dice: [(Arc<RenderImage>, Arc<RenderImage>); 6],
    pub black_dice: [(Arc<RenderImage>, Arc<RenderImage>); 6],
    pub cubes: HashMap<String, SizedSprite>,
    pub numbers: HashMap<NumberOrientation, [SizedSprite; 2]>,
}

impl BgBlitzGpuTheme {
    pub fn from_decoded(decoded: DecodedBgBlitzTheme) -> Self {
        let mut backgrounds = HashMap::new();
        backgrounds.insert(
            (false, false),
            render_image_from_rgba(decoded.background.clone()),
        );

        let white_checkers = decoded
            .white_checkers
            .into_iter()
            .map(|img| {
                let dims = (img.width() as f32, img.height() as f32);
                (render_image_from_rgba(img), dims)
            })
            .collect();
        let black_checkers = decoded
            .black_checkers
            .into_iter()
            .map(|img| {
                let dims = (img.width() as f32, img.height() as f32);
                (render_image_from_rgba(img), dims)
            })
            .collect();
        let white_dice = decoded.white_dice.map(|img| {
            let dimmed = render_image_with_alpha(&img, 0.28);
            (render_image_from_rgba(img), dimmed)
        });
        let black_dice = decoded.black_dice.map(|img| {
            let dimmed = render_image_with_alpha(&img, 0.28);
            (render_image_from_rgba(img), dimmed)
        });
        let cubes = decoded
            .cubes
            .into_iter()
            .map(|(k, img)| {
                let dims = (img.width() as f32, img.height() as f32);
                (k, (render_image_from_rgba(img), dims))
            })
            .collect();
        let numbers = decoded
            .numbers
            .into_iter()
            .map(|(orient, pair)| {
                let gpu_pair = pair.map(|img| {
                    let dims = (img.width() as f32, img.height() as f32);
                    (render_image_from_rgba(img), dims)
                });
                (orient, gpu_pair)
            })
            .collect();

        Self {
            id: decoded.id,
            name: decoded.name,
            metrics: decoded.metrics,
            background_raw: decoded.background,
            backgrounds,
            white_checkers,
            black_checkers,
            white_dice,
            black_dice,
            cubes,
            numbers,
        }
    }

    /// Returns the background `RenderImage` matching `(flip_x, flip_y)`,
    /// creating and caching the flipped texture lazily on first request.
    pub fn background_for_flip(&mut self, flip_x: bool, flip_y: bool) -> Arc<RenderImage> {
        Arc::clone(
            self.backgrounds
                .entry((flip_x, flip_y))
                .or_insert_with(|| render_image_flipped(&self.background_raw, flip_x, flip_y)),
        )
    }

    /// Selects the cube face sprite matching `piece-artwork.ts::cubeImage`.
    pub fn cube_sprite(
        &self,
        value: i32,
        owner: Option<u8>,
        pending: bool,
    ) -> Option<&(Arc<RenderImage>, (f32, f32))> {
        let face = if value == 1 {
            "64".to_string()
        } else {
            value.to_string()
        };
        if !pending
            && owner.is_none()
            && value == 1
            && let Some(right) = self.cubes.get("64_r")
        {
            return Some(right);
        }
        self.cubes.get(&face)
    }

    /// Collects every GPU `Arc<RenderImage>` owned by this theme so
    /// `cx.drop_image` can release them cleanly on LRU eviction.
    pub fn all_render_images(&self) -> Vec<Arc<RenderImage>> {
        let mut out = Vec::new();
        out.extend(self.backgrounds.values().cloned());
        out.extend(self.white_checkers.iter().map(|(img, _)| Arc::clone(img)));
        out.extend(self.black_checkers.iter().map(|(img, _)| Arc::clone(img)));
        for (full, dim) in &self.white_dice {
            out.push(Arc::clone(full));
            out.push(Arc::clone(dim));
        }
        for (full, dim) in &self.black_dice {
            out.push(Arc::clone(full));
            out.push(Arc::clone(dim));
        }
        out.extend(self.cubes.values().map(|(img, _)| Arc::clone(img)));
        for pair in self.numbers.values() {
            for (img, _) in pair {
                out.push(Arc::clone(img));
            }
        }
        out
    }
}

/// Parses and validates a single BGBlitz `.zip` theme directly in Rust with
/// full `tools/prepare_themes.py` security and format parity.
pub fn load_bgblitz_zip(path: &Path) -> Result<DecodedBgBlitzTheme> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(meta.len() <= MAX_ARCHIVE_BYTES, "ZIP exceeds 32 MiB");

    let slug = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    ensure!(
        !slug.is_empty()
            && slug
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Theme ZIP name must contain only letters, digits, - or _"
    );

    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("invalid ZIP archive")?;

    let mut entries: HashMap<String, String> = HashMap::new();
    let mut total_expanded: u64 = 0;
    for idx in 0..archive.len() {
        let entry = archive.by_index(idx).context("read ZIP entry header")?;
        let name = entry.name().to_string();
        validate_archive_rel_path(&name)?;
        total_expanded = total_expanded.saturating_add(entry.size());
        ensure!(
            total_expanded <= MAX_EXPANDED_BYTES,
            "Expanded ZIP exceeds theme size limits"
        );
        if !entry.is_dir() {
            let key = name.to_ascii_lowercase();
            ensure!(
                entries.insert(key, name).is_none(),
                "Ambiguous archive filename"
            );
            ensure!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Expanded ZIP exceeds theme size limits"
            );
        }
    }

    // Locate the single <theme> XML descriptor (ignoring outer <themes> index).
    let mut theme_defs: Vec<(String, ParsedThemeXml)> = Vec::new();
    let mut xml_names: Vec<String> = entries
        .values()
        .filter(|name| name.to_ascii_lowercase().ends_with(".xml"))
        .cloned()
        .collect();
    xml_names.sort();

    for xml_name in xml_names {
        let data = read_zip_entry_capped(&mut archive, &xml_name, MAX_XML_BYTES)?;
        if let Some(parsed) = parse_theme_xml(&data)? {
            theme_defs.push((xml_name, parsed));
        }
    }
    ensure!(
        theme_defs.len() == 1,
        "ZIP must contain exactly one theme definition"
    );
    let (def_name, xml) = theme_defs.pop().unwrap();
    let base_dir = def_name.rsplit_once('/').map_or("", |(dir, _)| dir);

    ensure!(
        xml.numbers_type == "twoImage"
            && xml.dice_type == "sixImage"
            && xml.cube_type == "sixImage",
        "This adapter requires two-image numbers and separate dice/cube images"
    );

    let mut load_img =
        |rel_name: &str, require_png: bool, optional: bool| -> Result<Option<RgbaImage>> {
            validate_archive_rel_path(rel_name)?;
            let requested = if base_dir.is_empty() {
                rel_name.to_string()
            } else {
                format!("{base_dir}/{rel_name}")
            };
            let Some(actual) = entries.get(&requested.to_ascii_lowercase()) else {
                if optional {
                    return Ok(None);
                }
                bail!("Missing image: {requested}");
            };
            let ext = Path::new(actual)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            ensure!(
                matches!(ext.as_str(), "png" | "jpg" | "jpeg"),
                "Unsupported image extension: {actual}"
            );
            let bytes = read_zip_entry_capped(&mut archive, actual, MAX_EXPANDED_BYTES as usize)?;
            if require_png {
                ensure!(
                    bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                    "Checker artwork must be PNG"
                );
            } else {
                ensure!(
                    bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(b"\xff\xd8"),
                    "Only PNG and JPEG theme artwork is supported"
                );
            }
            let decoded = image::load_from_memory(&bytes)
                .with_context(|| format!("decode image {actual}"))?
                .to_rgba8();
            let (w, h) = decoded.dimensions();
            ensure!(
                w > 0 && w <= MAX_IMAGE_DIM && h > 0 && h <= MAX_IMAGE_DIM,
                "Image dimensions outside 1..=8192"
            );
            Ok(Some(decoded))
        };

    let background = load_img(&xml.background_name, false, false)?
        .ok_or_else(|| anyhow!("Missing background image"))?;
    let bg_w = background.width() as f32;
    let bg_h = background.height() as f32;

    ensure!(
        xml.columns.len() == 15
            && xml
                .columns
                .iter()
                .all(|&c| c.is_finite() && c >= 0.0 && c < bg_w)
            && xml.columns.windows(2).all(|w| w[0] <= w[1]),
        "Expected 15 ordered checker column coordinates within the board"
    );
    let columns: [f32; 15] = xml
        .columns
        .try_into()
        .map_err(|_| anyhow!("Expected 15 checker columns"))?;

    let mut load_checker_variants = |pattern: &str| -> Result<Vec<RgbaImage>> {
        let cap = xml.checker_num_images.clamp(1, MAX_CHECKER_VARIANTS);
        let mut candidates = Vec::with_capacity(cap);
        candidates.push(pattern.replace('?', ""));
        for i in 2..=cap {
            let name = pattern.replace('?', &i.to_string());
            if !candidates.contains(&name) {
                candidates.push(name);
            }
        }
        let mut found = Vec::new();
        for name in candidates {
            if let Some(img) = load_img(&name, true, true)? {
                ensure!(
                    u64::from(img.width()) * u64::from(img.height()) <= MAX_CHECKER_PIXELS,
                    "Checker artwork has more than 1024 x 1024 pixels"
                );
                found.push(img);
            }
        }
        ensure!(!found.is_empty(), "Missing checker artwork");
        let first_dims = found[0].dimensions();
        ensure!(
            found.iter().all(|img| img.dimensions() == first_dims),
            "Checker variants must share dimensions"
        );
        Ok(found)
    };

    let red_checkers = load_checker_variants(&xml.checker_red)?;
    let green_checkers = load_checker_variants(&xml.checker_green)?;

    let red_lum = mean_opaque_luminance(&red_checkers[0])?;
    let green_lum = mean_opaque_luminance(&green_checkers[0])?;
    let green_is_white = green_lum > red_lum;

    let (white_checkers, black_checkers, white_dice_pat, black_dice_pat) = if green_is_white {
        (
            green_checkers,
            red_checkers,
            xml.dice_green.as_str(),
            xml.dice_red.as_str(),
        )
    } else {
        (
            red_checkers,
            green_checkers,
            xml.dice_red.as_str(),
            xml.dice_green.as_str(),
        )
    };

    let mut load_dice_set = |pattern: &str| -> Result<[RgbaImage; 6]> {
        let mut faces = Vec::with_capacity(6);
        for pip in 1..=6 {
            let img = load_img(&pattern.replace('?', &pip.to_string()), false, false)?
                .ok_or_else(|| anyhow!("Missing die face {pip}"))?;
            faces.push(img);
        }
        faces
            .try_into()
            .map_err(|_| anyhow!("Expected 6 die faces"))
    };
    let white_dice = load_dice_set(white_dice_pat)?;
    let black_dice = load_dice_set(black_dice_pat)?;

    let mut cubes = HashMap::new();
    for val in [2, 4, 8, 16, 32, 64] {
        let key = val.to_string();
        let img = load_img(&xml.cube_name.replace('?', &key), false, false)?
            .ok_or_else(|| anyhow!("Missing cube image {key}"))?;
        cubes.insert(key, img);
    }
    for side in ["64_l", "64_r"] {
        if let Some(img) = load_img(&xml.cube_name.replace('?', side), false, true)? {
            cubes.insert(side.to_string(), img);
        }
    }

    let mut numbers = HashMap::new();
    for orient in NumberOrientation::ALL {
        let pattern = xml
            .numbers
            .get(&orient)
            .ok_or_else(|| anyhow!("Missing number orientation {}", orient.as_str()))?;
        let top = load_img(&pattern.replace('?', "1"), false, false)?
            .ok_or_else(|| anyhow!("Missing number strip 1 for {}", orient.as_str()))?;
        let bottom = load_img(&pattern.replace('?', "2"), false, false)?
            .ok_or_else(|| anyhow!("Missing number strip 2 for {}", orient.as_str()))?;
        numbers.insert(orient, [top, bottom]);
    }

    let metrics = BgBlitzMetrics {
        bg_width: bg_w,
        bg_height: bg_h,
        columns,
        checker_offset_x: xml.checker_offset_x as f32,
        checker_offset_y: xml.checker_offset_y as f32,
        white_checker_size: (
            white_checkers[0].width() as f32,
            white_checkers[0].height() as f32,
        ),
        black_checker_size: (
            black_checkers[0].width() as f32,
            black_checkers[0].height() as f32,
        ),
        white_die_size: (white_dice[0].width() as f32, white_dice[0].height() as f32),
        black_die_size: (black_dice[0].width() as f32, black_dice[0].height() as f32),
        dice_left_x: xml.dice_left_x as f32,
        dice_right_x: xml.dice_right_x as f32,
        cube_left_x: xml.cube_left_x as f32,
        cube_right_x: xml.cube_right_x as f32,
        cube_offset_y: xml.cube_offset_y as f32,
        cube_centered_y: xml.cube_centered_y as f32,
        numbers_x: xml.numbers_x as f32,
        numbers_y: [xml.numbers_y[0] as f32, xml.numbers_y[1] as f32],
    };

    Ok(DecodedBgBlitzTheme {
        id: format!("bgblitz:{slug}"),
        name: xml.display_name,
        metrics,
        background,
        white_checkers,
        black_checkers,
        white_dice,
        black_dice,
        cubes,
        numbers,
    })
}

/// Scans `dir` for `*.zip` BGBlitz themes (rejecting case-insensitive stem
/// collisions) and returns lightweight gallery entries sorted by slug.
pub fn discover_bgblitz_themes(dir: &Path) -> Result<Vec<BgBlitzThemeEntry>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut zips: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read themes directory {}", dir.display()))?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        })
        .collect();
    zips.sort();

    let mut seen_stems = HashMap::<String, PathBuf>::new();
    for z in &zips {
        let stem = z
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        ensure!(
            seen_stems.insert(stem, z.clone()).is_none(),
            "Theme ZIP names must be distinct ignoring letter case"
        );
    }

    let mut entries = Vec::with_capacity(zips.len());
    for zip_path in zips {
        let decoded = load_bgblitz_zip(&zip_path)
            .with_context(|| format!("load theme archive {}", zip_path.display()))?;
        entries.push(decoded.build_gallery_entry(zip_path));
    }
    Ok(entries)
}

fn validate_archive_rel_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && !path.starts_with('/') && !path.contains('\\'),
        "Unsafe archive path: {path}"
    );
    ensure!(
        !path.split('/').any(|part| part == ".."),
        "Unsafe archive path: {path}"
    );
    Ok(())
}

fn read_zip_entry_capped(
    archive: &mut ZipArchive<File>,
    name: &str,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let entry = archive
        .by_name(name)
        .with_context(|| format!("missing ZIP entry {name}"))?;
    let mut buf = Vec::new();
    entry
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut buf)
        .with_context(|| format!("read ZIP entry {name}"))?;
    ensure!(
        buf.len() <= max_bytes,
        "ZIP entry exceeds size limit: {name}"
    );
    Ok(buf)
}

struct ParsedThemeXml {
    display_name: String,
    background_name: String,
    numbers_type: String,
    numbers_x: i32,
    numbers_y: [i32; 2],
    numbers: HashMap<NumberOrientation, String>,
    checker_red: String,
    checker_green: String,
    checker_offset_x: i32,
    checker_offset_y: i32,
    checker_num_images: usize,
    columns: Vec<f32>,
    dice_type: String,
    dice_red: String,
    dice_green: String,
    dice_left_x: i32,
    dice_right_x: i32,
    cube_type: String,
    cube_name: String,
    cube_left_x: i32,
    cube_right_x: i32,
    cube_offset_y: i32,
    cube_centered_y: i32,
}

fn parse_theme_xml(data: &[u8]) -> Result<Option<ParsedThemeXml>> {
    ensure!(data.len() <= MAX_XML_BYTES, "Unsafe or oversized XML");
    let text = std::str::from_utf8(data).context("XML is not valid UTF-8")?;
    let upper = text.to_ascii_uppercase();
    ensure!(
        !upper.contains("<!DOCTYPE") && !upper.contains("<!ENTITY"),
        "Unsafe or oversized XML"
    );

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut root_seen = false;
    let mut is_theme = false;
    let mut in_checker = false;
    let mut checker_text = String::new();

    let mut display_name = None;
    let mut bg_seen = false;
    let mut background_name = None;
    let mut numbers_type = String::new();
    let mut numbers_x = 0;
    let mut numbers_y = [0, 0];
    let mut numbers = HashMap::new();

    let mut checker_seen = false;
    let mut checker_red = None;
    let mut checker_green = None;
    let mut checker_offset_x = 0;
    let mut checker_offset_y = 0;
    let mut checker_num_images = 1usize;

    let mut dice_seen = false;
    let mut dice_type = String::new();
    let mut dice_red = None;
    let mut dice_green = None;
    let mut dice_left_x = 0;
    let mut dice_right_x = 0;

    let mut cube_seen = false;
    let mut cube_type = String::new();
    let mut cube_name = None;
    let mut cube_left_x = 0;
    let mut cube_right_x = 0;
    let mut cube_offset_y = 0;
    let mut cube_centered_y = 0;

    let parse_attrs = |e: &quick_xml::events::BytesStart<'_>| -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        for attr in e.attributes() {
            let attr = attr.context("malformed XML attribute")?;
            let key = std::str::from_utf8(attr.key.as_ref())?.to_string();
            let val = attr
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                .into_owned();
            map.insert(key, val);
        }
        Ok(map)
    };

    let parse_coord = |attrs: &HashMap<String, String>, key: &str, default: i32| -> Result<i32> {
        let val = match attrs.get(key) {
            Some(raw) => raw
                .trim()
                .parse::<i32>()
                .with_context(|| format!("invalid integer attribute {key}={raw}"))?,
            None => default,
        };
        ensure!(val.abs() <= MAX_COORD, "Theme coordinate exceeds limits");
        Ok(val)
    };

    loop {
        match reader.read_event().context("parse theme XML")? {
            Event::DocType(_) => bail!("Unsafe or oversized XML"),
            Event::Start(e) | Event::Empty(e) => {
                let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                let attrs = parse_attrs(&e)?;
                if !root_seen {
                    root_seen = true;
                    if tag != "theme" {
                        return Ok(None);
                    }
                    is_theme = true;
                    display_name = attrs.get("displayName").cloned();
                    continue;
                }
                match tag.as_str() {
                    "background" => {
                        bg_seen = true;
                        background_name = attrs.get("name").cloned();
                        numbers_type = attrs.get("numbersType").cloned().unwrap_or_default();
                        numbers_x = parse_coord(&attrs, "numbersXoff", 0)?;
                        numbers_y = [
                            parse_coord(&attrs, "numbersYoff", 0)?,
                            parse_coord(&attrs, "numbersYoff2", 0)?,
                        ];
                    }
                    "numbers" => {
                        if let (Some(kind), Some(name)) = (attrs.get("type"), attrs.get("name"))
                            && let Some(orient) = NumberOrientation::from_tag(kind)
                        {
                            numbers.insert(orient, name.clone());
                        }
                    }
                    "checker" => {
                        checker_seen = true;
                        checker_red = attrs.get("red").cloned();
                        checker_green = attrs.get("green").cloned();
                        checker_offset_x = parse_coord(&attrs, "offsetX", 0)?;
                        checker_offset_y = parse_coord(&attrs, "offsetY", 0)?;
                        let num = parse_coord(&attrs, "numImages", 1)?;
                        checker_num_images = num.max(1) as usize;
                        in_checker = true;
                    }
                    "dice" => {
                        dice_seen = true;
                        dice_type = attrs.get("type").cloned().unwrap_or_default();
                        dice_red = attrs.get("red").cloned();
                        dice_green = attrs.get("green").cloned();
                        dice_left_x = parse_coord(&attrs, "leftX", 0)?;
                        dice_right_x = parse_coord(&attrs, "rightX", 0)?;
                    }
                    "cube" => {
                        cube_seen = true;
                        cube_type = attrs.get("type").cloned().unwrap_or_default();
                        cube_name = attrs.get("name").cloned();
                        cube_left_x = parse_coord(&attrs, "leftX", 0)?;
                        cube_right_x = parse_coord(&attrs, "rightX", 0)?;
                        cube_offset_y = parse_coord(&attrs, "offsetY", 0)?;
                        cube_centered_y = parse_coord(&attrs, "centeredCubeYOffset", 0)?;
                    }
                    _ => {}
                }
            }
            Event::Text(e) if in_checker => {
                checker_text
                    .push_str(std::str::from_utf8(e.as_ref()).context("decode checker columns")?);
            }
            Event::End(e) => {
                if e.name().as_ref() == b"checker" {
                    in_checker = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !is_theme {
        return Ok(None);
    }
    ensure!(bg_seen, "Missing background definition");
    ensure!(checker_seen, "Missing checker definition");
    ensure!(dice_seen, "Missing dice definition");
    ensure!(cube_seen, "Missing cube definition");

    let columns: Vec<f32> = checker_text
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<f32>()
                .with_context(|| format!("invalid checker column coordinate '{s}'"))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Some(ParsedThemeXml {
        display_name: display_name.ok_or_else(|| anyhow!("Missing displayName attribute"))?,
        background_name: background_name.ok_or_else(|| anyhow!("Missing background name"))?,
        numbers_type,
        numbers_x,
        numbers_y,
        numbers,
        checker_red: checker_red.ok_or_else(|| anyhow!("Missing checker red attribute"))?,
        checker_green: checker_green.ok_or_else(|| anyhow!("Missing checker green attribute"))?,
        checker_offset_x,
        checker_offset_y,
        checker_num_images,
        columns,
        dice_type,
        dice_red: dice_red.ok_or_else(|| anyhow!("Missing dice red attribute"))?,
        dice_green: dice_green.ok_or_else(|| anyhow!("Missing dice green attribute"))?,
        dice_left_x,
        dice_right_x,
        cube_type,
        cube_name: cube_name.ok_or_else(|| anyhow!("Missing cube name attribute"))?,
        cube_left_x,
        cube_right_x,
        cube_offset_y,
        cube_centered_y,
    }))
}

/// GPU texture & sprite cache managing built-in WebPs, quantized `resvg`
/// surface textures, BGBlitz gallery thumbnails, and LRU-evicted full BGBlitz
/// themes (`cx.drop_image`).
pub struct ThemeAtlasCache {
    themes_dir: Option<PathBuf>,
    max_resident_bgblitz: usize,
    /// Built-in checker sprites keyed by `(theme, is_white)`.
    builtin_checkers: HashMap<(BuiltInThemeId, bool), [Arc<RenderImage>; 2]>,
    /// Built-in cube bodies keyed by `theme`.
    builtin_cubes: HashMap<BuiltInThemeId, Arc<RenderImage>>,
    /// Quantized `resvg` surface textures keyed by `(theme, bucket_w, bucket_h)`.
    surface_rasters: HashMap<(BuiltInThemeId, u32, u32), Arc<RenderImage>>,
    surface_lru: VecDeque<(BuiltInThemeId, u32, u32)>,
    /// Discovered BGBlitz ZIP index (thumbnails + paths).
    bgblitz_catalog: HashMap<String, BgBlitzThemeEntry>,
    bgblitz_catalog_order: Vec<String>,
    /// Full-resolution GPU-resident BGBlitz themes + LRU queue.
    bgblitz_gpu: HashMap<String, BgBlitzGpuTheme>,
    bgblitz_lru: VecDeque<String>,
    /// Evicted `RenderImage`s waiting to be dropped via `cx.drop_image`.
    pending_drops: Vec<Arc<RenderImage>>,
}

impl Default for ThemeAtlasCache {
    fn default() -> Self {
        Self::new(None, DEFAULT_MAX_RESIDENT_BGBLITZ)
    }
}

impl ThemeAtlasCache {
    pub fn new(themes_dir: Option<PathBuf>, max_resident_bgblitz: usize) -> Self {
        Self {
            themes_dir,
            max_resident_bgblitz: max_resident_bgblitz.max(1),
            builtin_checkers: HashMap::new(),
            builtin_cubes: HashMap::new(),
            surface_rasters: HashMap::new(),
            surface_lru: VecDeque::new(),
            bgblitz_catalog: HashMap::new(),
            bgblitz_catalog_order: Vec::new(),
            bgblitz_gpu: HashMap::new(),
            bgblitz_lru: VecDeque::new(),
            pending_drops: Vec::new(),
        }
    }

    /// Scans `self.themes_dir` (if configured) and populates gallery thumbnails.
    pub fn scan_bgblitz_dir(&mut self) -> Result<usize> {
        let Some(dir) = self.themes_dir.clone() else {
            return Ok(0);
        };
        let entries = discover_bgblitz_themes(&dir)?;
        self.set_bgblitz_entries(entries);
        Ok(self.bgblitz_catalog_order.len())
    }

    /// Replaces the discovered BGBlitz gallery catalog (e.g. from a background scan).
    pub fn set_bgblitz_entries(&mut self, entries: Vec<BgBlitzThemeEntry>) {
        self.bgblitz_catalog.clear();
        self.bgblitz_catalog_order.clear();
        for entry in entries {
            self.bgblitz_catalog_order.push(entry.id.clone());
            self.bgblitz_catalog.insert(entry.id.clone(), entry);
        }
    }

    pub fn themes_dir(&self) -> Option<&Path> {
        self.themes_dir.as_deref()
    }

    /// Registers an already-decoded BGBlitz theme directly (e.g. loaded off the
    /// UI thread via `cx.background_executor()`).
    pub fn insert_bgblitz_decoded(&mut self, decoded: DecodedBgBlitzTheme, zip_path: PathBuf) {
        let id = decoded.id.clone();
        if !self.bgblitz_catalog.contains_key(&id) {
            self.bgblitz_catalog_order.push(id.clone());
            self.bgblitz_catalog
                .insert(id.clone(), decoded.build_gallery_entry(zip_path));
        }
        let gpu = BgBlitzGpuTheme::from_decoded(decoded);
        if let Some(old) = self.bgblitz_gpu.insert(id.clone(), gpu) {
            self.pending_drops.extend(old.all_render_images());
        }
        self.touch_bgblitz_lru(&id);
    }

    /// Returns all discovered BGBlitz gallery entries in canonical order.
    pub fn bgblitz_entries(&self) -> Vec<&BgBlitzThemeEntry> {
        self.bgblitz_catalog_order
            .iter()
            .filter_map(|id| self.bgblitz_catalog.get(id))
            .collect()
    }

    pub fn bgblitz_entry(&self, id: &str) -> Option<&BgBlitzThemeEntry> {
        self.bgblitz_catalog.get(id)
    }

    pub fn resident_bgblitz_count(&self) -> usize {
        self.bgblitz_gpu.len()
    }

    /// Ensures the requested BGBlitz theme is loaded in GPU memory (synchronously
    /// from its ZIP if cataloged), returning `None` if unavailable.
    pub fn get_or_load_bgblitz(&mut self, id: &str) -> Option<&mut BgBlitzGpuTheme> {
        if !id.starts_with("bgblitz:") {
            return None;
        }
        if !self.bgblitz_gpu.contains_key(id) {
            let zip_path = self.bgblitz_catalog.get(id)?.zip_path.clone();
            match load_bgblitz_zip(&zip_path) {
                Ok(decoded) => {
                    let gpu = BgBlitzGpuTheme::from_decoded(decoded);
                    self.bgblitz_gpu.insert(id.to_string(), gpu);
                }
                Err(err) => {
                    tracing::warn!("failed to load BGBlitz theme {id}: {err:#}");
                    return None;
                }
            }
        }
        self.touch_bgblitz_lru(id);
        self.bgblitz_gpu.get_mut(id)
    }

    /// Returns the `BoardGeometry` for a theme ID (falling back to `BuiltIn` if
    /// an archive theme is unavailable).
    pub fn geometry_for(&mut self, board_theme_id: &str) -> BoardGeometry {
        if let Some(gpu) = self.get_or_load_bgblitz(board_theme_id) {
            BoardGeometry::BgBlitz(gpu.metrics.clone())
        } else {
            BoardGeometry::BuiltIn
        }
    }

    /// Returns the 2 WebP checker sprite variants for a textured built-in theme,
    /// decoding lazily once.
    pub fn builtin_checker_pair(
        &mut self,
        theme: BuiltInThemeId,
        white: bool,
    ) -> Option<&[Arc<RenderImage>; 2]> {
        if let Entry::Vacant(slot) = self.builtin_checkers.entry((theme, white)) {
            let [b0, b1] = theme.checker_webps(white)?;
            let img0 = image::load_from_memory(b0).ok()?.to_rgba8();
            let img1 = image::load_from_memory(b1).ok()?.to_rgba8();
            slot.insert([render_image_from_rgba(img0), render_image_from_rgba(img1)]);
        }
        self.builtin_checkers.get(&(theme, white))
    }

    /// Returns the 192×192 WebP cube body for any built-in theme.
    pub fn builtin_cube(&mut self, theme: BuiltInThemeId) -> Option<Arc<RenderImage>> {
        if let Entry::Vacant(slot) = self.builtin_cubes.entry(theme) {
            let img = image::load_from_memory(theme.cube_webp()).ok()?.to_rgba8();
            slot.insert(render_image_from_rgba(img));
        }
        self.builtin_cubes.get(&theme).cloned()
    }

    /// Returns a quantized `resvg`-rasterized surface `RenderImage` for a
    /// textured built-in theme, caching up to 8 `(theme, bucket_w, bucket_h)`
    /// entries and queuing older ones for `cx.drop_image`.
    pub fn builtin_surface(
        &mut self,
        theme: BuiltInThemeId,
        pixel_width: u32,
        pixel_height: u32,
    ) -> Option<Arc<RenderImage>> {
        let svg = theme.surface_svg()?;
        let key = (
            theme,
            quantize_surface_dim(pixel_width),
            quantize_surface_dim(pixel_height),
        );
        if let Entry::Vacant(slot) = self.surface_rasters.entry(key) {
            let rendered = rasterize_surface_svg(svg, key.1, key.2).ok()?;
            slot.insert(rendered);
            self.surface_lru.push_back(key);
            while self.surface_lru.len() > 8 {
                if let Some(evicted) = self.surface_lru.pop_front()
                    && evicted != key
                    && let Some(img) = self.surface_rasters.remove(&evicted)
                {
                    self.pending_drops.push(img);
                }
            }
        }
        self.surface_rasters.get(&key).cloned()
    }

    /// Evicts any queued GPU textures via GPUI's `cx.drop_image(img, window)` so
    /// switching themes or resizing never leaks Metal atlas pages.
    pub fn flush_evictions(&mut self, mut window: Option<&mut Window>, cx: &mut App) -> usize {
        let count = self.pending_drops.len();
        for img in self.pending_drops.drain(..) {
            cx.drop_image(img, window.as_deref_mut());
        }
        count
    }

    /// Number of evicted textures awaiting `flush_evictions`.
    pub fn pending_drop_count(&self) -> usize {
        self.pending_drops.len()
    }

    fn touch_bgblitz_lru(&mut self, id: &str) {
        self.bgblitz_lru.retain(|existing| existing != id);
        self.bgblitz_lru.push_back(id.to_string());
        while self.bgblitz_lru.len() > self.max_resident_bgblitz {
            if let Some(evicted_id) = self.bgblitz_lru.pop_front()
                && evicted_id != id
                && let Some(old) = self.bgblitz_gpu.remove(&evicted_id)
            {
                self.pending_drops.extend(old.all_render_images());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn rgba_to_bgra_swaps_red_and_blue_preserving_green_and_alpha() {
        let mut buf = vec![0x11, 0x22, 0x33, 0xFF, 0xAA, 0xBB, 0xCC, 0x40];
        rgba_to_bgra(&mut buf);
        assert_eq!(buf, vec![0x33, 0x22, 0x11, 0xFF, 0xCC, 0xBB, 0xAA, 0x40]);
    }

    #[test]
    fn all_nine_builtin_themes_produce_cached_gallery_assets() {
        for theme in BuiltInThemeId::ALL {
            let assets = builtin_gallery_assets(theme);
            assert!(assets.cube.is_some(), "gallery cube missing for {theme:?}");
            if theme.surface_svg().is_some() {
                assert!(
                    assets.surface.is_some(),
                    "gallery surface missing for {theme:?}"
                );
                assert!(
                    assets.white_checker.is_some(),
                    "gallery white checker missing for {theme:?}"
                );
                assert!(
                    assets.black_checker.is_some(),
                    "gallery black checker missing for {theme:?}"
                );
            } else {
                assert!(assets.surface.is_none());
                assert!(assets.white_checker.is_none());
                assert!(assets.black_checker.is_none());
            }
        }
    }

    #[test]
    fn all_nine_builtin_themes_decode_surfaces_checkers_and_cubes() {
        let mut cache = ThemeAtlasCache::default();
        for theme in BuiltInThemeId::ALL {
            assert_eq!(BuiltInThemeId::parse(theme.as_str()), Some(theme));
            assert!(
                cache.builtin_cube(theme).is_some(),
                "cube missing for {theme:?}"
            );
            if theme.surface_svg().is_some() {
                let surf = cache
                    .builtin_surface(theme, 534, 414)
                    .expect("surface svg rasterizes");
                assert!(surf.as_bytes(0).is_some());
                assert!(cache.builtin_checker_pair(theme, true).is_some());
                assert!(cache.builtin_checker_pair(theme, false).is_some());
            } else {
                assert!(cache.builtin_checker_pair(theme, true).is_none());
            }
        }
    }

    #[test]
    fn dice_theme_resolution_handles_match_board_follow_checkers_and_explicit() {
        let cfg = BoardThemeConfig {
            board_theme: "terra-rossa".into(),
            checker_theme: Some("ion".into()),
            cube_theme: None,
            dice_theme: Some(MATCH_BOARD_DICE.into()),
        };
        assert_eq!(cfg.resolved_checker_id(), "ion");
        assert_eq!(cfg.resolved_cube_id(), "terra-rossa");
        assert_eq!(cfg.resolved_dice_id(), "terra-rossa");

        let follow_checkers = BoardThemeConfig {
            dice_theme: None,
            ..cfg.clone()
        };
        assert_eq!(follow_checkers.resolved_dice_id(), "ion");

        let explicit = BoardThemeConfig {
            dice_theme: Some("forest".into()),
            ..cfg
        };
        assert_eq!(explicit.resolved_dice_id(), "forest");
    }

    #[test]
    fn bgblitz_zip_loader_rejects_traversal_doctype_and_oversized_coords() {
        let dir = tempfile::tempdir().unwrap();

        // 1. Path traversal inside ZIP.
        let bad_path = dir.path().join("traversal.zip");
        {
            let file = File::create(&bad_path).unwrap();
            let mut z = zip::ZipWriter::new(file);
            z.start_file("../evil.xml", SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"<theme/>").unwrap();
            z.finish().unwrap();
        }
        assert!(load_bgblitz_zip(&bad_path).is_err());

        // 2. XML DOCTYPE / ENTITY rejection.
        let xxe_path = dir.path().join("xxe.zip");
        {
            let file = File::create(&xxe_path).unwrap();
            let mut z = zip::ZipWriter::new(file);
            z.start_file("theme.xml", SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"<!DOCTYPE theme [<!ENTITY x \"a\">]><theme displayName=\"X\"/>")
                .unwrap();
            z.finish().unwrap();
        }
        assert!(load_bgblitz_zip(&xxe_path).is_err());

        // 3. Invalid filename slug.
        let bad_slug = dir.path().join("bad name!.zip");
        File::create(&bad_slug).unwrap();
        assert!(load_bgblitz_zip(&bad_slug).is_err());
    }

    #[test]
    fn loads_all_present_local_bgblitz_zips_with_luminance_and_lru_eviction() {
        let repo_themes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let zips: Vec<PathBuf> = std::fs::read_dir(&repo_themes)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "zip"))
            .collect();
        if zips.is_empty() {
            // Fresh checkout without untracked BGBlitz archives (mirrors CI).
            return;
        }

        let mut cache = ThemeAtlasCache::new(Some(repo_themes), 2);
        let discovered = cache
            .scan_bgblitz_dir()
            .expect("discover local BGBlitz ZIPs");
        assert_eq!(discovered, zips.len());

        let ids: Vec<String> = cache
            .bgblitz_entries()
            .iter()
            .map(|e| e.id.clone())
            .collect();
        for id in &ids {
            let gpu = cache
                .get_or_load_bgblitz(id)
                .unwrap_or_else(|| panic!("theme {id} should load"));
            assert_eq!(gpu.numbers.len(), 4);
            assert!(gpu.cubes.contains_key("64"));
            assert!(!gpu.white_checkers.is_empty());
            assert!(!gpu.black_checkers.is_empty());
            if id == "bgblitz:cl_green" {
                assert_eq!(gpu.metrics.bg_width, 2062.0);
                assert_eq!(gpu.metrics.bg_height, 1477.0);
                assert_eq!(gpu.metrics.numbers_y, [682.0, 764.0]);
            }
        }
        if ids.len() > 2 {
            assert_eq!(cache.resident_bgblitz_count(), 2);
            assert!(cache.pending_drop_count() > 0);
        }
    }
}
