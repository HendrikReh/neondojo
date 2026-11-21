//! Strict standard-backgammon imports. The board is absolute: White moves
//! 24 -> 1, positive checkers; Black moves 1 -> 24, negative checkers.
//! Index 0 is Black's bar, and index 25 is White's bar.

mod bgf;
pub(crate) mod ids;
pub(crate) mod mat;
pub(crate) mod match_context;
pub(crate) mod ogxm;
pub(crate) mod sgf;
mod xg;
pub use bgf::bgf_to_sgf;
pub use ogxm::ogxm_to_sgf;
pub use sgf::parse_sgf;
pub(crate) use xg::xg_beaver_rule;
pub use xg::{XgImport, convert_xg, xg_to_sgf};

/// Canonical SGF of a MAT transcript and the irregularities its import resolved.
#[derive(Debug, Clone)]
pub struct MatImport {
    pub sgf: String,
    pub warnings: Vec<crate::model::ImportWarning>,
}

#[derive(Debug, Clone)]
pub struct ImportedMatch {
    pub players: [String; 2],
    pub match_length: i32,
    pub positions: Vec<ImportedPosition>,
    pub games: Vec<crate::model::MatchGame>,
}

#[derive(Debug, Clone)]
pub struct ImportedPosition {
    pub game_no: i32,
    pub ply: i32,
    pub player: i32,
    pub dice: [i32; 2],
    pub board: Vec<i32>,
    pub position_id: String,
    pub match_id: String,
    pub notation: String,
}

#[cfg(test)]
mod tests;
