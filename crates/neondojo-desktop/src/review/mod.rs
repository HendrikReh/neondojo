//! Shared backgammon review primitives sitting strictly below all workspaces.
//!
//! Consumed by Analysis, Playing review, Saved Positions, and Study without any
//! cross-workspace dependencies.

pub mod cube_details;
pub mod move_explanation;
pub mod move_history_row;
pub mod preview_controls;
pub mod severity;
