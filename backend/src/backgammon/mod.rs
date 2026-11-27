//! Standard backgammon without any file format, engine or database: boards,
//! legal checker turns and GNUbg position and match IDs. The board is absolute:
//! White moves 24 -> 1, positive checkers; Black moves 1 -> 24, negative
//! checkers. Index 0 is Black's bar, and index 25 is White's bar.

pub(crate) mod board;
pub(crate) mod ids;
pub(crate) mod notation;
mod rules;
pub(crate) use board::{initial_board, local_board, valid_board};
pub(crate) use rules::{CheckerPath, apply_checker_turn, checker_paths, expand_joined_play};

#[cfg(test)]
mod tests {
    /// Import, engine and play code depend on the rules, never the reverse.
    #[test]
    fn the_rules_depend_on_no_other_module() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backgammon");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let source = std::fs::read_to_string(&path).unwrap();
            for (index, _) in source.match_indices(concat!("crate", "::")) {
                assert!(
                    source[index..].starts_with(concat!("crate", "::backgammon")),
                    "{}: {}",
                    path.display(),
                    source[index..].lines().next().unwrap()
                );
            }
        }
    }
}
