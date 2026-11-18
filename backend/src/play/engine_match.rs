use super::{NativeRequest, PlayPhase, open_sage, run_native_with_dice};
use crate::imported_analysis::sage::{RetainedChecker, retain};
use crate::{
    engine::Engine,
    engines::{AnalysisEngine, EngineCost},
};
use anyhow::{Context, Result, ensure};

pub(crate) struct GameDice {
    pub opening: Vec<[u8; 2]>,
    pub turns: Vec<[u8; 2]>,
}

/// One duel unit: a point match with the cube, or a block of independent
/// cubeless money games without the Jacoby rule in one native session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DuelUnit {
    Match { length: i32 },
    MoneyCubeless { games: i32 },
}

pub(crate) struct FinishedMatch {
    /// Match score, or total points won in a money block, per native player.
    pub scores: [i32; 2],
    pub games: i32,
    pub sgf: String,
    /// Money blocks only: games won per native player as single, gammon and
    /// backgammon.
    pub outcomes: Option<[[i32; 3]; 2]>,
    /// Per native player; `None` when an engine step reported no cost.
    pub costs: [Option<SideCost>; 2],
}

/// Engine work of one side over a whole match. `moves` counts the side's
/// checker turns, including forced moves and passes; cube decisions add cost
/// but no moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SideCost {
    pub cost: EngineCost,
    pub moves: i32,
}

/// Accumulates measured engine work per native player during one match.
struct CostMeter {
    totals: [Option<EngineCost>; 2],
}

impl CostMeter {
    fn new() -> Self {
        Self {
            totals: [Some(EngineCost::default()); 2],
        }
    }

    fn add(&mut self, side: usize, cost: Option<EngineCost>) {
        if let Some(total) = self.totals.get_mut(side) {
            *total = total.zip(cost).map(|(mut total, cost)| {
                total += cost;
                total
            });
        }
    }

    fn add_native(&mut self, snapshot: &super::NativeSnapshot, bots: [bool; 2]) {
        for (side, &native) in bots.iter().enumerate() {
            if native {
                self.add(side, snapshot.bot_costs.map(|costs| costs[side]));
            }
        }
    }

    fn finish(self, history: &[super::PlayHistory]) -> [Option<SideCost>; 2] {
        std::array::from_fn(|side| {
            self.totals[side].map(|cost| SideCost {
                cost,
                moves: checker_turns(history, side),
            })
        })
    }
}

fn checker_turns(history: &[super::PlayHistory], side: usize) -> i32 {
    let count = history
        .iter()
        .filter(|record| {
            usize::try_from(record.player).is_ok_and(|player| player == side)
                && !matches!(
                    record.notation.as_str(),
                    "double" | "take" | "drop" | "resign"
                )
        })
        .count();
    i32::try_from(count).unwrap_or(i32::MAX)
}

pub(crate) async fn play(
    rules: &Engine,
    bots: [&AnalysisEngine; 2],
    depths: [i32; 2],
    unit: DuelUnit,
    dice: impl Fn(i32) -> Result<GameDice>,
) -> Result<FinishedMatch> {
    let (length, last_game, cubeless) = match unit {
        // In a point match each completed game awards at least one point.
        DuelUnit::Match { length } => (length, 2 * length - 1, false),
        DuelUnit::MoneyCubeless { games } => (0, games, true),
    };
    let mut outcomes = cubeless.then_some([[0; 3]; 2]);
    let mut previous_scores = [0; 2];
    let match_bots = Some(std::array::from_fn(|side| {
        matches!(bots[side], AnalysisEngine::Gnubg(_)).then_some(depths[side])
    }));
    let native_bots = match_bots.map_or([false; 2], |bots| bots.map(|depth| depth.is_some()));
    let mut saved: Option<(super::NativeSnapshot, String)> = None;
    let mut checker_evaluations = Vec::new();
    let mut meter = CostMeter::new();
    for game in 1..=last_game {
        let GameDice { opening, turns } = dice(game)?;
        let request = NativeRequest {
            new: saved.is_none(),
            external_bot: true,
            accept_resignation: None,
            human_player: 0,
            match_length: length,
            plies: 1,
            native_id: "",
            action: "opening_roll",
            notation: None,
            resign_level: None,
            match_bots,
            cubeless,
        };
        let (mut snapshot, mut sgf) = run_native_with_dice(
            rules,
            &NativeRequest {
                native_id: saved.as_ref().map_or("", |(s, _)| s.native_id.as_str()),
                ..request
            },
            saved.as_ref().map(|(_, sgf)| sgf.as_str()),
            Some(&opening),
        )
        .await?;
        ensure!(snapshot.game_no == game, "Unexpected native game number");
        let native_opening = snapshot
            .opening
            .as_ref()
            .context("Missing native opening")?;
        ensure!(
            native_opening
                .rolls
                .iter()
                .copied()
                .eq(opening.iter().map(|roll| roll.map(i32::from))),
            "GNUbg did not use the scheduled opening rolls"
        );
        let first_roll = sorted(*opening.last().context("Missing scheduled opening roll")?);
        let mut consumed = 0;
        let mut choice = ("start_opening_move", None);
        for _ in 0..4096 {
            // An empty file would allow the native file RNG to fail or fall back.
            // One sentinel roll permits a pending checker/cube decision to finish.
            let remaining = if consumed < turns.len() {
                &turns[consumed..]
            } else {
                &[[1, 1]]
            };
            let step = NativeRequest {
                new: false,
                native_id: &snapshot.native_id,
                action: choice.0,
                notation: choice.1.as_deref(),
                ..request
            };
            (snapshot, sgf) =
                run_native_with_dice(rules, &step, Some(&sgf), Some(remaining)).await?;
            meter.add_native(&snapshot, native_bots);
            let (actual_opening, actual_turns) = snapshot
                .match_rolls
                .split_first()
                .context("Missing played opening roll")?;
            ensure!(
                sorted(*actual_opening) == first_roll,
                "GNUbg changed the opening roll"
            );
            ensure!(
                (consumed..=turns.len()).contains(&actual_turns.len())
                    && actual_turns.iter().copied().map(sorted).eq(turns
                        .iter()
                        .copied()
                        .map(sorted)
                        .take(actual_turns.len())),
                "GNUbg did not use the scheduled turn rolls"
            );
            consumed = actual_turns.len();
            match snapshot.phase {
                PlayPhase::MatchOver if !cubeless => {
                    return Ok(FinishedMatch {
                        scores: snapshot.scores,
                        games: game,
                        outcomes: None,
                        costs: meter.finish(&snapshot.history),
                        sgf: tokio::task::spawn_blocking(move || retain(&sgf, checker_evaluations))
                            .await??,
                    });
                }
                PlayPhase::GameOver => {
                    if let Some(outcomes) = outcomes.as_mut() {
                        record_money_game(outcomes, previous_scores, snapshot.scores)?;
                        previous_scores = snapshot.scores;
                        if game == last_game {
                            return Ok(FinishedMatch {
                                scores: snapshot.scores,
                                games: game,
                                outcomes: Some(*outcomes),
                                costs: meter.finish(&snapshot.history),
                                sgf: tokio::task::spawn_blocking(move || {
                                    retain(&sgf, checker_evaluations)
                                })
                                .await??,
                            });
                        }
                    }
                    break;
                }
                _ => {}
            }
            let side = usize::try_from(snapshot.player)?;
            let Some(AnalysisEngine::OpenSage(bot)) = bots.get(side) else {
                anyhow::bail!("GNUbg did not yield to an external engine");
            };
            let chosen = open_sage::choose(bot, &snapshot).await?;
            meter.add(side, chosen.cost);
            if let Some(candidates) = chosen.checker {
                checker_evaluations.push((
                    snapshot.history.len(),
                    RetainedChecker::new(snapshot.native_id.clone(), bot.snapshot(), candidates),
                ));
            }
            choice = (chosen.action, chosen.notation);
        }
        ensure!(
            matches!(snapshot.phase, PlayPhase::GameOver),
            "Match game action limit exceeded"
        );
        saved = Some((snapshot, sgf));
    }
    anyhow::bail!("Match game limit exceeded")
}

/// Classifies one finished cubeless money game from the session score change:
/// exactly one side gains 1, 2 or 3 points.
fn record_money_game(
    outcomes: &mut [[i32; 3]; 2],
    before: [i32; 2],
    after: [i32; 2],
) -> Result<()> {
    let gained = [after[0] - before[0], after[1] - before[1]];
    let (winner, points) = match gained {
        [points, 0] => (0, points),
        [0, points] => (1, points),
        _ => anyhow::bail!("Unexpected cubeless money game result {gained:?}"),
    };
    ensure!(
        (1..=3).contains(&points),
        "Unexpected cubeless money game value {points}"
    );
    outcomes[winner][points as usize - 1] += 1;
    Ok(())
}

fn sorted([first, second]: [u8; 2]) -> [u8; 2] {
    [first.max(second), first.min(second)]
}

#[cfg(test)]
mod tests {
    use super::{CostMeter, EngineCost, SideCost};
    use crate::play::PlayHistory;

    fn record(player: i32, notation: &str) -> PlayHistory {
        PlayHistory {
            player,
            dice: vec![],
            notation: notation.into(),
        }
    }

    #[test]
    fn money_games_are_classified_by_the_winners_score_gain() {
        let mut outcomes = [[0; 3]; 2];
        super::record_money_game(&mut outcomes, [0, 0], [2, 0]).unwrap();
        super::record_money_game(&mut outcomes, [2, 0], [2, 3]).unwrap();
        super::record_money_game(&mut outcomes, [2, 3], [3, 3]).unwrap();
        assert_eq!(outcomes, [[1, 1, 0], [0, 0, 1]]);
        for after in [[2, 3], [3, 4], [6, 3], [2, 7]] {
            assert!(super::record_money_game(&mut outcomes, [2, 3], after).is_err());
        }
    }

    #[test]
    fn cost_meter_counts_checker_turns_and_drops_unmeasured_sides() {
        let cost = |cpu_us, wall_us| Some(EngineCost { cpu_us, wall_us });
        let mut meter = CostMeter::new();
        meter.add(0, cost(300, 200));
        meter.add(0, cost(100, 50));
        meter.add(1, cost(10, 10));
        // One unreported external decision makes the whole side unmeasured.
        meter.add(1, None);
        let history = [
            record(0, "8/5 6/5"),
            record(1, "double"),
            record(0, "take"),
            record(1, "pass"),
            record(0, "13/7"),
            record(1, "24/18"),
            record(0, "resign"),
        ];
        assert_eq!(
            meter.finish(&history),
            [
                Some(SideCost {
                    cost: EngineCost {
                        cpu_us: 400,
                        wall_us: 250,
                    },
                    moves: 2,
                }),
                None,
            ]
        );
    }
}
