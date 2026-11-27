//! SGF: the text format (`parser`), the validated replay of a match
//! (`replay`) and the foreign evaluations a conversion keeps (`retained`).

pub(crate) mod coord;
mod parser;
mod replay;
mod retained;
pub(crate) use parser::{
    MAX_SGF_BYTES, Node, escape_value, one, parse_trees, serialize_trees, write_property,
};
pub use replay::parse_sgf;
pub(crate) use replay::{
    GameContext, SourceMove, parse_importable_sgf_with_context, parse_native_sgf_source_moves,
    parse_native_sgf_with_context, parse_sgf_events, parse_sgf_source_moves,
    parse_sgf_with_context, play, replay_trees, setup,
};
pub(crate) use retained::{decode_retained_analysis, encode_retained_analysis};

pub(crate) const MATCH_FORFEIT_PROPERTY: &str = "BTFORFEIT";
