//! Strict standard-backgammon imports, replayed with the rules and boards of
//! [`crate::backgammon`]. The same module owns the XG writer, so both
//! directions share one reading of the binary format.

mod bgf;
pub(crate) mod mat;
pub(crate) mod match_context;
pub(crate) mod ogxm;
pub(crate) mod sgf;
mod xg;
mod xg_export;
pub use bgf::bgf_to_sgf;
pub use ogxm::ogxm_to_sgf;
pub use sgf::parse_sgf;
pub(crate) use xg::xg_beaver_rule;
pub use xg::{XgImport, convert_xg, xg_to_sgf};
pub use xg_export::{XgExportMetadata, sgf_to_xg};

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
