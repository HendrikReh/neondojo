//! NeonDojo GPU backgammon board renderer, BGBlitz theme loader, animations, and
//! audio engine built on GPUI (`gpui-kit`) and `neondojo-rules`.

pub mod animation;
pub mod audio;
pub mod geometry;
pub mod themes;
pub mod view;

pub use animation::{
    ActiveVisual, BoardAnimationState, CheckerFrame, DiceFrame, DieUsage, PlayFrame, checker_frame,
    cubic_bezier_checker, reverse_frame,
};
pub use audio::{DEFAULT_PLAY_SOUND_VOLUME, DecodedClip, PlayAudioEngine, PlaySoundKind};
pub use geometry::{
    ArrowGeometry, BearoffDirection, BgBlitzMetrics, BoardGeometry, BoardHitboxes, BoardTransform,
    BoardViewMode, CheckerMotion, DecodedMatchContext, DiePlacement, LogicalPoint, LogicalRect,
    MovePreview, MovePreviewStep, NumberOrientation, PlayOrientation, StackGeometry,
    apply_absolute_steps, board_flips, borne_off_counts, checker_center, cube_rect,
    decode_match_context, default_analysis_board_width, dice_placements, move_arrow_geometry,
    pip_counts, point_stack_geometry, project_move, relative_point, tray_counts_for_frame,
    underlay_board,
};
pub use themes::{
    BgBlitzGpuTheme, BgBlitzThemeEntry, BoardThemeConfig, BuiltInBoardPalette, BuiltInThemeId,
    DecodedBgBlitzTheme, MATCH_BOARD_DICE, ThemeAtlasCache, discover_bgblitz_themes,
    load_bgblitz_zip, mean_opaque_luminance, quantize_surface_dim, rasterize_surface_svg,
    rgba_to_bgra,
};
pub use view::{
    BoardDiceInteraction, BoardEvent, BoardGalleryTile, BoardInteractionState, BoardPosition,
    BoardView,
};
