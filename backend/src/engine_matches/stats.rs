use super::{DiceMode, DuelFormat, MatchResult};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeriesStatistics {
    pub wins_a: i32,
    pub wins_b: i32,
    pub win_rate_a: Option<f64>,
    pub complete_pairs: i32,
    /// 95% interval for A's match win rate; matches in paired mode only.
    pub confidence_interval_a: Option<[f64; 2]>,
    /// Cubeless money blocks only. `wins_a`/`wins_b` then count games won.
    pub money: Option<MoneyStatistics>,
    /// Engine work of A over the counted matches that recorded it.
    pub cost_a: Option<SideCostSummary>,
    /// Engine work of B over the counted matches that recorded it.
    pub cost_b: Option<SideCostSummary>,
}

/// Results of cubeless money blocks over the counted blocks.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MoneyStatistics {
    pub games: i64,
    /// Net points per game from A's perspective, between −3 and 3.
    pub points_per_game_a: Option<f64>,
    /// 95% interval for `points_per_game_a`, from 20 pairs or blocks.
    pub confidence_interval: Option<[f64; 2]>,
    pub outcomes_a: GameOutcomes,
    pub outcomes_b: GameOutcomes,
}

/// Games won by one side, by value.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct GameOutcomes {
    pub single: i64,
    pub gammon: i64,
    pub backgammon: i64,
}

impl From<[i64; 3]> for GameOutcomes {
    fn from([single, gammon, backgammon]: [i64; 3]) -> Self {
        Self {
            single,
            gammon,
            backgammon,
        }
    }
}

/// Engine work per checker turn. CPU time covers all engine threads; both
/// include cube decisions but exclude process startup, replay and checks.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SideCostSummary {
    /// Counted matches with a measurement for this side.
    pub measured_matches: i32,
    /// Checker turns of this side in those matches, including forced moves.
    pub moves: i64,
    pub cpu_ms_per_move: f64,
    pub wall_ms_per_move: f64,
}

pub(super) fn summarize(
    format: DuelFormat,
    mode: DiceMode,
    results: &[MatchResult],
) -> SeriesStatistics {
    let complete_pairs = if mode == DiceMode::Paired {
        results.len() / 2
    } else {
        0
    };
    let counted = if mode == DiceMode::Paired {
        &results[..complete_pairs * 2]
    } else {
        results
    };
    // Independent observations: pairs of legs, or single units without pairing.
    let units = counted.chunks(mode.matches_per_unit() as usize);
    let enough = units.len() >= MIN_PAIRS_FOR_INTERVAL;
    let (wins_a, wins_b, confidence_interval_a, money) = match format {
        DuelFormat::Match => {
            let wins_a = counted
                .iter()
                .filter(|result| result.score_a > result.score_b)
                .count() as i32;
            let interval = (mode == DiceMode::Paired && enough)
                .then(|| {
                    let means = units.map(|pair| {
                        let wins = pair.iter().filter(|r| r.score_a > r.score_b).count();
                        wins as f64 / pair.len() as f64
                    });
                    t_interval(&means.collect::<Vec<_>>(), 0.0, 1.0)
                })
                .flatten();
            (wins_a, counted.len() as i32 - wins_a, interval, None)
        }
        DuelFormat::MoneyCubeless => {
            let money = money_statistics(counted, units.clone(), enough);
            (
                money.games_won_a,
                money.games_won_b,
                None,
                Some(money.summary),
            )
        }
    };
    let decided = wins_a + wins_b;
    let win_rate_a = (decided > 0).then(|| f64::from(wins_a) / f64::from(decided));
    SeriesStatistics {
        wins_a,
        wins_b,
        win_rate_a,
        complete_pairs: complete_pairs as i32,
        confidence_interval_a,
        money,
        cost_a: side_cost(counted, 0),
        cost_b: side_cost(counted, 1),
    }
}

struct MoneyTotals {
    games_won_a: i32,
    games_won_b: i32,
    summary: MoneyStatistics,
}

/// Points per game for A over cubeless money blocks. The interval treats each
/// pair of blocks (or each block without pairing) as one observation.
fn money_statistics<'a>(
    counted: &[MatchResult],
    units: impl Iterator<Item = &'a [MatchResult]>,
    enough: bool,
) -> MoneyTotals {
    let mut outcomes = [[0_i64; 3]; 2];
    for counts in counted
        .iter()
        .filter_map(|result| result.outcomes.as_deref())
    {
        for (index, &count) in counts.iter().take(6).enumerate() {
            outcomes[index / 3][index % 3] += i64::from(count);
        }
    }
    let games: i64 = outcomes.iter().flatten().sum();
    let net = |results: &[MatchResult]| {
        let points: i64 = results
            .iter()
            .map(|r| i64::from(r.score_a) - i64::from(r.score_b))
            .sum();
        let games: i64 = results.iter().map(|r| i64::from(r.games)).sum();
        (points, games)
    };
    let (points, _) = net(counted);
    let interval = enough
        .then(|| {
            let means = units
                .map(|unit| {
                    let (points, games) = net(unit);
                    points as f64 / games.max(1) as f64
                })
                .collect::<Vec<_>>();
            t_interval(&means, -3.0, 3.0)
        })
        .flatten();
    let won = |side: [i64; 3]| side.iter().sum::<i64>();
    MoneyTotals {
        games_won_a: i32::try_from(won(outcomes[0])).unwrap_or(i32::MAX),
        games_won_b: i32::try_from(won(outcomes[1])).unwrap_or(i32::MAX),
        summary: MoneyStatistics {
            games,
            points_per_game_a: (games > 0).then(|| points as f64 / games as f64),
            confidence_interval: interval,
            outcomes_a: GameOutcomes::from(outcomes[0]),
            outcomes_b: GameOutcomes::from(outcomes[1]),
        },
    }
}

fn side_cost(counted: &[MatchResult], side: usize) -> Option<SideCostSummary> {
    let (mut matches, mut cpu_us, mut wall_us, mut moves) = (0_i32, 0_i64, 0_i64, 0_i64);
    for (cpu, wall, turns) in counted.iter().filter_map(|result| result.cost.side(side)) {
        matches += 1;
        cpu_us = cpu_us.saturating_add(cpu);
        wall_us = wall_us.saturating_add(wall);
        moves += i64::from(turns);
    }
    (moves > 0).then(|| SideCostSummary {
        measured_matches: matches,
        moves,
        cpu_ms_per_move: cpu_us as f64 / 1000.0 / moves as f64,
        wall_ms_per_move: wall_us as f64 / 1000.0 / moves as f64,
    })
}

const MIN_PAIRS_FOR_INTERVAL: usize = 20;

/// Two-sided 95% Student-t interval for the mean of independent observations,
/// using their sample variance and clamped to `[min, max]`. With paired dice,
/// each observation is a pair of legs, so the variance includes the reduction
/// from swapping sides. Returns `None` for fewer than two observations.
fn t_interval(values: &[f64], min: f64, max: f64) -> Option<[f64; 2]> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (n - 1) as f64;
    let radius = student_t_975(n - 1) * (variance / n as f64).sqrt();
    Some([(mean - radius).max(min), (mean + radius).min(max)])
}

/// 0.975 quantile of Student's t distribution with `df` degrees of freedom.
///
/// Cornish–Fisher expansion around the normal quantile (Abramowitz & Stegun
/// 26.7.5). Its error is below 1e-4 for the 19 or more degrees of freedom a
/// displayed interval requires.
pub(super) fn student_t_975(df: usize) -> f64 {
    const Z: f64 = 1.959_963_984_540_054;
    let n = df as f64;
    let z2 = Z * Z;
    let g1 = (z2 + 1.0) * Z / 4.0;
    let g2 = ((5.0 * z2 + 16.0) * z2 + 3.0) * Z / 96.0;
    let g3 = (((3.0 * z2 + 19.0) * z2 + 17.0) * z2 - 15.0) * Z / 384.0;
    let g4 = ((((79.0 * z2 + 776.0) * z2 + 1482.0) * z2 - 1920.0) * z2 - 945.0) * Z / 92_160.0;
    Z + g1 / n + g2 / (n * n) + g3 / (n * n * n) + g4 / (n * n * n * n)
}
