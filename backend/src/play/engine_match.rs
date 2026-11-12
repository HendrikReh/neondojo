use super::{NativeRequest, PlayPhase, open_sage, run_native_with_dice};
use crate::imported_analysis::sage::{RetainedChecker, retain};
use crate::{engine::Engine, engines::AnalysisEngine};
use anyhow::{Context, Result, ensure};

pub(crate) struct GameDice {
    pub opening: Vec<[u8; 2]>,
    pub turns: Vec<[u8; 2]>,
}

pub(crate) struct FinishedMatch {
    pub scores: [i32; 2],
    pub games: i32,
    pub sgf: String,
}

pub(crate) async fn play(
    rules: &Engine,
    bots: [&AnalysisEngine; 2],
    depths: [i32; 2],
    length: i32,
    dice: impl Fn(i32) -> Result<GameDice>,
) -> Result<FinishedMatch> {
    let match_bots = Some(std::array::from_fn(|side| {
        matches!(bots[side], AnalysisEngine::Gnubg(_)).then_some(depths[side])
    }));
    let mut saved: Option<(super::NativeSnapshot, String)> = None;
    let mut checker_evaluations = Vec::new();
    // In a point match each completed game awards at least one point.
    for game in 1..2 * length {
        let GameDice { opening, turns } = dice(game)?;
        let request = NativeRequest {
            new: saved.is_none(),
            external_bot: true,
            accept_resignation: None,
            human_player: 0,
            match_length: length,
            plies: 0,
            native_id: "",
            action: "opening_roll",
            notation: None,
            resign_level: None,
            match_bots,
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
                PlayPhase::MatchOver => {
                    return Ok(FinishedMatch {
                        scores: snapshot.scores,
                        games: game,
                        sgf: tokio::task::spawn_blocking(move || retain(&sgf, checker_evaluations))
                            .await??,
                    });
                }
                PlayPhase::GameOver => break,
                _ => {}
            }
            let side = usize::try_from(snapshot.player)?;
            let Some(AnalysisEngine::OpenSage(bot)) = bots.get(side) else {
                anyhow::bail!("GNUbg did not yield to an external engine");
            };
            let chosen = open_sage::choose(bot, &snapshot).await?;
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

fn sorted([first, second]: [u8; 2]) -> [u8; 2] {
    [first.max(second), first.min(second)]
}
