//! The immutable replay shared by every game checkpoint of a NeonGammon job.
use crate::import::{
    ImportedPosition,
    sgf::{SourceMove, parse_sgf_events},
};
use anyhow::{Context, Result, ensure};

pub(crate) struct PreparedMatch(Vec<PreparedGame>);

pub(super) struct PreparedGame {
    pub(super) game_no: i32,
    pub(super) cube_enabled: bool,
    pub(super) positions: Vec<ImportedPosition>,
    pub(super) events: Vec<SourceMove>,
}

impl PreparedMatch {
    pub(crate) fn parse(source: &str) -> Result<Self> {
        let (imported, contexts, moves) = parse_sgf_events(source)?;
        let mut games: Vec<_> = imported
            .games
            .iter()
            .zip(contexts)
            .map(|(game, context)| PreparedGame {
                game_no: game.game_no,
                cube_enabled: context.cube_enabled,
                positions: Vec::new(),
                events: Vec::new(),
            })
            .collect();
        for position in imported.positions {
            games[(position.game_no - 1) as usize]
                .positions
                .push(position);
        }
        for event in moves {
            games[(event.game_no - 1) as usize].events.push(event);
        }
        Ok(Self(games))
    }

    pub(super) fn game(&self, number: i32) -> Result<&PreparedGame> {
        let index = usize::try_from(number).ok().and_then(|n| n.checked_sub(1));
        index
            .and_then(|i| self.0.get(i))
            .context("Game number outside match")
    }
}

impl PreparedGame {
    pub(super) fn position(&self, event: &SourceMove) -> Result<&ImportedPosition> {
        let ply = event.checker_ply.context("Missing checker ply")?;
        let position = usize::try_from(ply)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| self.positions.get(i))
            .context("Missing imported checker context")?;
        ensure!(
            position.ply == ply && event.game_no == self.game_no,
            "Mismatched checker context"
        );
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wc2014_replay_indexes_every_checker_and_retains_event_order() {
        let replay = PreparedMatch::parse(crate::benchmarks::REFERENCE_SGF).unwrap();
        let mut lookups = 0;
        for game_no in 1..=10 {
            let game = replay.game(game_no).unwrap();
            for (index, event) in game.events.iter().enumerate() {
                assert_eq!(event.move_no as usize, index + 1);
                if event.kind == "checker" {
                    let position = game.position(event).unwrap();
                    assert_eq!(
                        (&position.position_id, &position.match_id),
                        (&event.position_id, &event.match_id)
                    );
                    lookups += 1;
                }
            }
        }
        assert_eq!(lookups, 460);
        assert!(replay.game(0).is_err());
        assert!(replay.game(11).is_err());
    }
}
