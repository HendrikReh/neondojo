use super::{NativeRequest, PlayPhase, open_sage, rules_process::RulesProcess};
use crate::imported_analysis::sage::{RetainedChecker, retain};
use crate::{
    engine::Engine,
    engines::{AnalysisEngine, DuelWorkers, EngineCost},
};
use anyhow::{Context, Result, ensure};

pub(crate) struct GameDice {
    pub opening: Vec<[u8; 2]>,
    pub turns: Vec<[u8; 2]>,
}

pub(super) const ACTION_LIMIT: u32 = 4096;

impl GameDice {
    pub(super) fn rng_file(&self) -> Result<Vec<u8>> {
        ensure!(!self.opening.is_empty(), "Missing scheduled opening roll");
        self.opening
            .iter()
            .chain(&self.turns)
            .flatten()
            .map(|&die| {
                ensure!((1..=6).contains(&die), "Invalid scheduled die");
                Ok(b'0' + die)
            })
            .collect()
    }

    /// Native rolls may reverse the two dice. Validate the complete played
    /// prefix and reject a rewind, including use of GNUbg's end-of-file repeat.
    fn verify_turns(&self, rolls: &[[u8; 2]], consumed: usize) -> Result<usize> {
        let (opening, turns) = rolls.split_first().context("Missing played opening roll")?;
        ensure!(
            Some(sorted(*opening)) == self.opening.last().copied().map(sorted),
            "GNUbg changed the opening roll"
        );
        ensure!(
            (consumed..=self.turns.len()).contains(&turns.len())
                && turns.iter().copied().map(sorted).eq(self
                    .turns
                    .iter()
                    .copied()
                    .take(turns.len())
                    .map(sorted)),
            "GNUbg did not use the scheduled turn rolls"
        );
        Ok(turns.len())
    }
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

#[cfg(test)]
pub(crate) async fn play(
    rules: &Engine,
    bots: [&AnalysisEngine; 2],
    depths: [i32; 2],
    unit: DuelUnit,
    dice: impl Fn(i32) -> Result<GameDice>,
) -> Result<FinishedMatch> {
    let workers = DuelWorkers::new(bots.map(|bot| match bot {
        AnalysisEngine::OpenSage(sage) => Some(sage.snapshot()),
        AnalysisEngine::Gnubg(_) => None,
    }));
    let finished = play_with_workers(rules, bots, depths, unit, &workers, false, dice).await;
    let shutdown = workers.shutdown().await;
    finished.and_then(|match_result| shutdown.map(|()| match_result))
}

pub(crate) async fn play_with_workers(
    rules: &Engine,
    bots: [&AnalysisEngine; 2],
    depths: [i32; 2],
    unit: DuelUnit,
    workers: &DuelWorkers<'_>,
    swapped: bool,
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
        // The rules engine plays its own turns; other bots yield to it.
        bots[side].id().is_rules_engine().then_some(depths[side])
    }));
    let native_bots = match_bots.map_or([false; 2], |bots| bots.map(|depth| depth.is_some()));
    let mut saved: Option<(super::NativeSnapshot, String)> = None;
    let mut checker_evaluations = Vec::new();
    let mut meter = CostMeter::new();
    for game in 1..=last_game {
        let schedule = dice(game)?;
        let (snapshot, sgf) = crate::engine::with_shared_access(async {
            let request = NativeRequest {
                new: saved.is_none(),
                external_bot: true,
                accept_resignation: None,
                human_player: 0,
                match_length: length,
                plies: 1,
                native_id: saved.as_ref().map_or("", |(s, _)| s.native_id.as_str()),
                action: "opening_roll",
                notation: None,
                resign_level: None,
                match_bots,
                cubeless,
            };
            let mut batches = workers.begin_game_sides(swapped).await?;
            let process = RulesProcess::start_with_dice(
                rules,
                saved.as_ref().map(|(_, sgf)| sgf.as_str()),
                Some(&schedule),
            )
            .await?;
            let (mut process, mut snapshot, _) = process.execute(&request).await?;
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
                    .eq(schedule.opening.iter().map(|roll| roll.map(i32::from))),
                "GNUbg did not use the scheduled opening rolls"
            );
            let mut consumed = 0;
            let mut choice = ("start_opening_move", None);
            for _ in 0..ACTION_LIMIT {
                let step = NativeRequest {
                    new: false,
                    native_id: &snapshot.native_id,
                    action: choice.0,
                    notation: choice.1.as_deref(),
                    ..request
                };
                let sgf;
                (process, snapshot, sgf) = process.execute(&step).await?;
                meter.add_native(&snapshot, native_bots);
                consumed = schedule.verify_turns(&snapshot.match_rolls, consumed)?;
                if matches!(snapshot.phase, PlayPhase::MatchOver | PlayPhase::GameOver) {
                    process.finish().await?;
                    batches.finish().await?;
                    return Ok((snapshot, sgf));
                }
                let side = usize::try_from(snapshot.player)?;
                let Some(AnalysisEngine::OpenSage(bot)) = bots.get(side) else {
                    anyhow::bail!("GNUbg did not yield to an external engine");
                };
                let chosen =
                    open_sage::choose(bot, &snapshot, false, Some(batches.for_side(side)?)).await?;
                meter.add(side, chosen.cost);
                if let Some(candidates) = chosen.checker {
                    checker_evaluations.push((
                        snapshot.history.len(),
                        RetainedChecker::new(
                            snapshot.native_id.clone(),
                            bot.snapshot(),
                            candidates,
                        ),
                    ));
                }
                choice = (chosen.action, chosen.notation);
            }
            anyhow::bail!("Match game action limit exceeded")
        })
        .await?;
        let finished = if let Some(counts) = outcomes.as_mut() {
            ensure!(
                matches!(snapshot.phase, PlayPhase::GameOver),
                "Unexpected money match end"
            );
            record_money_game(counts, previous_scores, snapshot.scores)?;
            previous_scores = snapshot.scores;
            game == last_game
        } else {
            matches!(snapshot.phase, PlayPhase::MatchOver)
        };
        if finished {
            return Ok(FinishedMatch {
                scores: snapshot.scores,
                games: game,
                outcomes,
                costs: meter.finish(&snapshot.history),
                sgf: tokio::task::spawn_blocking(move || retain(&sgf, checker_evaluations))
                    .await??,
            });
        }
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
    fn scheduled_rolls_include_opening_ties_and_never_rewind_or_wrap() {
        let mut dice = super::GameDice {
            opening: vec![[3, 3], [2, 6]],
            turns: vec![[5, 2], [4, 4]],
        };
        assert_eq!(dice.rng_file().unwrap(), b"33265244");
        let first = [[6, 2], [2, 5]];
        assert_eq!(dice.verify_turns(&first, 0).unwrap(), 1);
        assert_eq!(dice.verify_turns(&first, 1).unwrap(), 1);
        assert_eq!(dice.verify_turns(&[[2, 6], [5, 2], [4, 4]], 1).unwrap(), 2);
        for rolls in [
            vec![],
            vec![[2, 6]],
            vec![[3, 3], [5, 2]],
            vec![[2, 6], [2, 4]],
            vec![[2, 6], [2, 5], [4, 4], [3, 3]],
        ] {
            assert!(dice.verify_turns(&rolls, 1).is_err());
        }
        dice.turns[0][0] = 7;
        assert!(dice.rng_file().is_err());
        dice.opening.clear();
        assert!(dice.rng_file().is_err());
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
