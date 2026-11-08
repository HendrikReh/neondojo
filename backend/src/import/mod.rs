//! Strict standard-backgammon imports. The board is absolute: White moves
//! 24 -> 1, positive checkers; Black moves 1 -> 24, negative checkers.
//! Index 0 is Black's bar, and index 25 is White's bar.

mod ids;
pub(crate) mod sgf;
mod xg;
pub use sgf::parse_sgf;
pub use xg::xg_to_sgf;

#[derive(Debug, Clone)]
pub struct ImportedMatch {
    pub players: [String; 2],
    pub match_length: i32,
    pub positions: Vec<ImportedPosition>,
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
