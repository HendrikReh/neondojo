//! Rules and engine access shared by interactive exercises.
use crate::{
    backgammon::{
        apply_checker_turn,
        ids::{self, Field},
    },
    model::Position,
    play::PlayStep,
};
use anyhow::{Context, Result, ensure};

pub(crate) mod evaluation;

// Serialize on-demand practice evaluations without holding a database lock.
pub(crate) static EVALUATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) fn checker_turn(position: &Position, steps: &[PlayStep]) -> Option<[i32; 26]> {
    if steps.len() > 4 || !matches!(position.player, 0 | 1) {
        return None;
    }
    let white = position.player == 0;
    let (bar, off) = if white { (25, -1) } else { (0, 26) };
    let relative = |point| {
        if white {
            point as usize
        } else {
            (25 - point) as usize
        }
    };
    let pairs = steps
        .iter()
        .map(|step| {
            let from = match step.from {
                from if from == bar => 25,
                from @ 1..=24 => relative(from),
                _ => return None,
            };
            let to = match step.to {
                to if to == off => 0,
                to @ 1..=24 => relative(to),
                _ => return None,
            };
            Some((from, to))
        })
        .collect::<Option<Vec<_>>>()?;
    apply_checker_turn(
        position.board.as_slice().try_into().ok()?,
        position.player,
        position.dice.as_slice().try_into().ok()?,
        &pairs,
    )
    .ok()
}

fn context(position: &Position) -> Result<ids::MatchId> {
    let id = ids::MatchId::decode(&position.gnubg_match_id).context("Invalid match ID")?;
    let length = id.get(Field::LENGTH);
    ensure!(
        matches!(position.player, 0 | 1)
            && id.get(Field::GAME_STATE) == 1
            && id.get(Field::RESIGNED) == 0
            && id.get(Field::CUBE_OWNER) != 2
            && (length == 0
                || [Field::SCORE_0, Field::SCORE_1]
                    .into_iter()
                    .all(|score| id.get(score) < length))
            && ids::decode_board(&position.position_id, &position.gnubg_match_id)?
                == position.board
            && position.board.iter().any(|&n| n > 0)
            && position.board.iter().any(|&n| n < 0),
        "Invalid stored decision position"
    );
    Ok(id)
}

pub(crate) fn validate_checker_context(position: &Position) -> Result<()> {
    let id = context(position)?;
    let first = id.get(Field::DIE_0);
    let second = id.get(Field::DIE_1);
    ensure!(
        id.get(Field::ON_ROLL) == position.player
            && id.get(Field::TURN) == position.player
            && id.get(Field::DOUBLED) == 0
            && (1..=6).contains(&first)
            && (1..=6).contains(&second)
            && (position.dice == [first, second] || position.dice == [second, first]),
        "Stored checker context does not match the roll and mover"
    );
    Ok(())
}

pub(crate) struct CubeContext {
    pub(crate) position_id: String,
    pub(crate) match_id: String,
}

/// Reconstruct the position before offering the cube. Older response cards
/// encode the responder as the mover, without an outstanding-offer bit.
pub(crate) fn cube_context(position: &Position, response: bool) -> Result<CubeContext> {
    let id = context(position)?;
    let player = position.player;
    let doubler = if response { 1 - player } else { player };
    let pending = id.get(Field::DOUBLED) == 1;
    let length = id.get(Field::LENGTH);
    let scores = [id.get(Field::SCORE_0), id.get(Field::SCORE_1)];
    let cube = 1 << id.get(Field::CUBE_LOG2);
    let owner = id.get(Field::CUBE_OWNER);
    let crawford = id.get(Field::CRAWFORD) == 1;
    let jacoby = id.get(Field::NO_JACOBY) == 0;
    ensure!(
        id.get(Field::TURN) == player
            && if pending {
                response && id.get(Field::ON_ROLL) == doubler
            } else {
                id.get(Field::ON_ROLL) == player
            },
        "Stored cube context does not match the decision maker"
    );
    ensure!(
        position.dice == [0, 0]
            && (!response || (id.get(Field::DIE_0) == 0 && id.get(Field::DIE_1) == 0))
            && !crawford
            && cube < 32768
            && (owner == 3 || owner == doubler),
        "Stored position does not permit this cube decision"
    );
    let board = position
        .board
        .as_slice()
        .try_into()
        .context("Invalid board length")?;
    Ok(CubeContext {
        position_id: ids::position_id(board, doubler),
        match_id: ids::match_id(
            length,
            scores,
            cube,
            owner,
            crawford,
            jacoby,
            doubler,
            [0, 0],
        ),
    })
}

pub(crate) fn cube_response_options(
    chosen: &str,
    no_double: f64,
    take: f64,
    pass: f64,
) -> Option<(&'static str, f64, f64)> {
    crate::match_analysis::cube_options(chosen, -no_double, -take, -pass)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{backgammon::initial_board, move_assessments::after_play};

    pub(crate) fn position(player: i32, dice: [i32; 2]) -> Position {
        let board = initial_board();
        Position {
            id: uuid::Uuid::nil(),
            match_uuid: uuid::Uuid::nil(),
            ordinal: 1,
            game_no: 1,
            ply: 1,
            player,
            dice: dice.to_vec(),
            board: board.to_vec(),
            position_id: ids::position_id(&board, player),
            gnubg_match_id: ids::match_id(7, [1, 2], 2, -1, false, false, player, dice),
            notation: String::new(),
        }
    }

    #[test]
    fn checker_steps_resolve_complete_moves_for_both_colors() {
        for (player, a, b, c) in [(0, 13, 8, 7), (1, 12, 17, 18)] {
            let position = position(player, [6, 1]);
            let complete = [PlayStep { from: a, to: c }, PlayStep { from: b, to: c }];
            assert!(checker_turn(&position, &complete[..1]).is_none());
            assert_eq!(
                checker_turn(&position, &complete),
                after_play(&position, "13/7 8/7")
            );
            assert!(checker_turn(&position, &complete).is_some());
        }
    }

    #[test]
    fn response_engine_context_restores_the_doubler_without_changing_score_or_cube() {
        let p = position(1, [0, 0]);
        let context = cube_context(&p, true).unwrap();
        assert_eq!(context.position_id, ids::position_id(&initial_board(), 0));
        assert_eq!(
            context.match_id,
            ids::match_id(7, [1, 2], 2, -1, false, false, 0, [0, 0])
        );
        assert_eq!(
            cube_response_options("take", 0.4, 1.1, 1.0),
            Some(("drop", -1.1, -1.0))
        );
        assert_eq!(
            cube_response_options("take", 0.4, 0.8, 1.0),
            Some(("take", -0.8, -0.8))
        );
    }

    #[test]
    fn cube_offers_keep_jacoby_hide_legacy_rolls_and_reject_crawford() {
        let mut p = position(1, [0, 0]);
        p.gnubg_match_id = ids::match_id(0, [0, 0], 2, 1, false, true, 1, [6, 3]);
        let context = cube_context(&p, false).unwrap();
        assert_eq!(
            context.match_id,
            ids::match_id(0, [0, 0], 2, 1, false, true, 1, [0, 0])
        );
        assert_eq!(ids::hide_dice(&p.gnubg_match_id).unwrap(), context.match_id);
        p.gnubg_match_id = ids::match_id(7, [6, 3], 1, -1, true, false, 1, [0, 0]);
        assert!(cube_context(&p, false).is_err());
        assert!(cube_context(&p, true).is_err());
    }

    #[test]
    fn checker_pass_requires_a_blocked_bar_and_checker_can_hit_on_entry() {
        let mut p = position(0, [6, 1]);
        p.board.fill(0);
        p.board[25] = 1;
        p.board[24] = -2;
        p.board[19] = -2;
        assert_eq!(checker_turn(&p, &[]).unwrap().as_slice(), p.board);
        assert!(checker_turn(&p, &[PlayStep { from: 25, to: 24 }]).is_none());
        p.board[24] = -1;
        let after_hit = checker_turn(
            &p,
            &[PlayStep { from: 25, to: 24 }, PlayStep { from: 24, to: 18 }],
        )
        .unwrap();
        assert_eq!(after_hit[0], -1);
        assert_eq!(after_hit[18], 1);
    }

    #[test]
    fn checker_turn_respects_higher_die_and_can_bear_off_with_an_oversized_roll() {
        let mut p = position(0, [6, 1]);
        p.board.fill(0);
        p.board[25] = 1;
        p.board[18] = -2;
        assert!(checker_turn(&p, &[PlayStep { from: 25, to: 24 }]).is_none());
        assert!(checker_turn(&p, &[PlayStep { from: 25, to: 19 }]).is_some());
        p.board.fill(0);
        p.board[1] = 1;
        p.board[7] = -1;
        assert!(checker_turn(&p, &[]).is_none());
        assert_eq!(
            checker_turn(&p, &[PlayStep { from: 1, to: -1 }]).unwrap()[1],
            0
        );
    }
}
