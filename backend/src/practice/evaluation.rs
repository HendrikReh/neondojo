//! Grade a submitted answer against a saved decision or one fresh engine run.
use crate::{
    api::{ApiError, ApiResult, AppState},
    engine::{Candidate, CubeEquities},
    engines::AnalysisEngine,
    match_analysis::{
        CubeEquityUnit, CubeEvaluation, Decision, cube_options, evaluated_equities, normalized_loss,
    },
    model::Position,
    move_assessments::after_play,
    play::PlayStep,
    practice::{self, CubeContext},
};
use anyhow::{Context, Result};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PracticeAnswer {
    Checker {
        #[schema(max_items = 4)]
        steps: Vec<PlayStep>,
    },
    CubeOffer {
        action: CubeOfferAction,
    },
    CubeResponse {
        action: CubeResponseAction,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CubeOfferAction {
    NoDouble,
    Double,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CubeResponseAction {
    Take,
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PracticeVerdict {
    Best,
    Playable,
    Error,
    Forced,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PracticeProvenanceKind {
    Stored,
    Recomputed,
    Forced,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct PracticeProvenance {
    kind: PracticeProvenanceKind,
    engine: Option<String>,
    plies: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct PracticeEvaluation {
    verdict: PracticeVerdict,
    chosen: String,
    best: String,
    chosen_equity: Option<f64>,
    best_equity: Option<f64>,
    equity_loss: f64,
    provenance: PracticeProvenance,
    /// All cube outcomes behind a cube answer, from the answering player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cube_evaluation: Option<CubeEvaluation>,
}

pub(crate) struct SavedEvaluation<'a> {
    pub(crate) engine: &'a str,
    pub(crate) plies: Option<i32>,
    pub(crate) decision: Option<&'a Decision>,
    pub(crate) candidates: &'a [Candidate],
    /// Imported checker projections may combine candidates of differing depths.
    pub(crate) native: bool,
}

impl PracticeProvenance {
    fn stored(solution: &SavedEvaluation<'_>) -> Option<Self> {
        (!solution.engine.trim().is_empty()).then(|| Self {
            kind: PracticeProvenanceKind::Stored,
            engine: Some(solution.engine.to_owned()),
            plies: solution.plies,
        })
    }

    fn recomputed(engine: String, plies: i32) -> Self {
        Self {
            kind: PracticeProvenanceKind::Recomputed,
            engine: Some(engine),
            plies: Some(plies),
        }
    }
}

fn proven_checker_depth(solution: &SavedEvaluation<'_>) -> Option<i32> {
    // GNUbg's move filter may finish candidates early and older cards omit
    // per-candidate depths. Open Sage verifies each returned candidate depth.
    let plies = solution.plies?;
    if solution.native
        && (plies == 0
            || solution.engine.starts_with("Open Sage ")
                && solution.engine.contains(&format!("; {plies}ply cubeful;")))
    {
        Some(plies)
    } else {
        None
    }
}

impl PracticeEvaluation {
    /// Verdict name and loss, for practice history.
    pub(crate) fn outcome(&self) -> (&'static str, f64) {
        let verdict = match self.verdict {
            PracticeVerdict::Best => "best",
            PracticeVerdict::Playable => "playable",
            PracticeVerdict::Error => "error",
            PracticeVerdict::Forced => "forced",
        };
        (verdict, self.equity_loss)
    }

    pub(crate) fn is_recomputed(&self) -> bool {
        matches!(self.provenance.kind, PracticeProvenanceKind::Recomputed)
    }

    fn scored(
        chosen: &str,
        best: &str,
        chosen_equity: f64,
        best_equity: f64,
        provenance: PracticeProvenance,
    ) -> Option<Self> {
        if !chosen_equity.is_finite()
            || !best_equity.is_finite()
            || chosen_equity - best_equity > 0.0000005
        {
            return None;
        }
        let equity_loss = normalized_loss(chosen_equity, best_equity);
        if !equity_loss.is_finite() {
            return None;
        }
        let verdict = if equity_loss == 0.0 {
            PracticeVerdict::Best
        } else if equity_loss < 0.04 {
            PracticeVerdict::Playable
        } else {
            PracticeVerdict::Error
        };
        Some(Self {
            verdict,
            chosen: chosen.into(),
            best: best.into(),
            chosen_equity: Some(chosen_equity),
            best_equity: Some(best_equity),
            equity_loss,
            provenance,
            cube_evaluation: None,
        })
    }

    fn forced_pass() -> Self {
        Self {
            verdict: PracticeVerdict::Forced,
            chosen: "Cannot move".into(),
            best: "Cannot move".into(),
            chosen_equity: None,
            best_equity: None,
            equity_loss: 0.0,
            provenance: PracticeProvenance {
                kind: PracticeProvenanceKind::Forced,
                engine: None,
                plies: None,
            },
            cube_evaluation: None,
        }
    }
}

#[derive(Clone, Copy)]
enum CubeAnswer {
    Offer(CubeOfferAction),
    Response(CubeResponseAction),
}

impl CubeAnswer {
    fn notation(self) -> &'static str {
        match self {
            Self::Offer(CubeOfferAction::NoDouble) => "no double",
            Self::Offer(CubeOfferAction::Double) => "double",
            Self::Response(CubeResponseAction::Take) => "take",
            Self::Response(CubeResponseAction::Drop) => "drop",
        }
    }

    fn allows(self, action: &str) -> bool {
        match self {
            Self::Offer(_) => matches!(action, "no double" | "double"),
            Self::Response(_) => matches!(action, "take" | "drop"),
        }
    }

    fn options(self, equities: CubeEquities) -> Option<(&'static str, f64, f64)> {
        let CubeEquities {
            no_double,
            take,
            pass,
            ..
        } = equities;
        match self {
            Self::Offer(_) => cube_options(self.notation(), no_double, take, pass),
            Self::Response(_) => {
                practice::cube_response_options(self.notation(), no_double, take, pass)
            }
        }
    }
}

enum CheckedAnswer {
    ForcedPass,
    Checker {
        board: [i32; 26],
    },
    Cube {
        context: CubeContext,
        answer: CubeAnswer,
    },
}

fn validate(kind: &str, position: &Position, answer: &PracticeAnswer) -> ApiResult<CheckedAnswer> {
    if let ("checker", PracticeAnswer::Checker { steps }) = (kind, answer) {
        practice::validate_checker_context(position)
            .map_err(|_| ApiError::invalid("This question has no valid checker context"))?;
        let board = practice::checker_turn(position, steps)
            .ok_or_else(|| ApiError::invalid("Play a complete legal checker move"))?;
        return Ok(if steps.is_empty() {
            CheckedAnswer::ForcedPass
        } else {
            CheckedAnswer::Checker { board }
        });
    }
    let answer = match (kind, answer) {
        ("cube_offer", PracticeAnswer::CubeOffer { action }) => CubeAnswer::Offer(*action),
        ("cube_response", PracticeAnswer::CubeResponse { action }) => CubeAnswer::Response(*action),
        _ => return Err(ApiError::invalid("Answer does not match the question type")),
    };
    let context = practice::cube_context(position, matches!(answer, CubeAnswer::Response(_)))
        .map_err(|_| ApiError::invalid("This question has no valid cube context"))?;
    Ok(CheckedAnswer::Cube { context, answer })
}

fn confirmed_decision<'a>(
    solution: &SavedEvaluation<'a>,
    position: &Position,
    kind: &str,
) -> Option<(&'a Decision, f64, f64)> {
    let decision = solution.decision?;
    if decision.kind != kind || decision.player != position.player {
        return None;
    }
    let (played, best) = evaluated_equities(decision)?;
    Some((decision, played, best))
}

fn stored_evaluation(
    position: &Position,
    solution: &SavedEvaluation<'_>,
    answer: &CheckedAnswer,
) -> Option<PracticeEvaluation> {
    let mut provenance = PracticeProvenance::stored(solution)?;
    match answer {
        CheckedAnswer::ForcedPass => None,
        CheckedAnswer::Checker { board } => {
            provenance.plies = proven_checker_depth(solution);
            if let Some((decision, played, best)) =
                confirmed_decision(solution, position, "checker")
            {
                let [a, b] = decision.dice?;
                if position.dice != [a, b] && position.dice != [b, a] {
                    return None;
                }
                let best_board = after_play(position, &decision.best)?;
                let played_board = after_play(position, &decision.played)?;
                let (notation, equity) = if board == &best_board {
                    (&decision.best, best)
                } else if board == &played_board && provenance.plies.is_some() {
                    (&decision.played, played)
                } else {
                    return None;
                };
                PracticeEvaluation::scored(notation, &decision.best, equity, best, provenance)
            } else {
                // Older position hints do not record per-candidate depths. Only
                // compare distinct candidates if the provider proves their depth.
                let mut first = solution.candidates.iter().filter(|entry| entry.rank == 1);
                let best = first.next()?;
                if first.next().is_some() {
                    return None;
                }
                let best_board = after_play(position, &best.notation)?;
                let chosen = if &best_board == board {
                    best
                } else {
                    provenance.plies?;
                    solution
                        .candidates
                        .iter()
                        .find(|c| after_play(position, &c.notation).as_ref() == Some(board))?
                };
                PracticeEvaluation::scored(
                    &chosen.notation,
                    &best.notation,
                    chosen.equity,
                    best.equity,
                    provenance,
                )
            }
        }
        CheckedAnswer::Cube { answer, .. } => {
            let (decision, played, best) = confirmed_decision(solution, position, "cube")?;
            if decision.dice.is_some()
                || !answer.allows(&decision.best)
                || !answer.allows(&decision.played)
            {
                return None;
            }
            let chosen = answer.notation();
            let equity = if chosen == decision.best {
                best
            } else if chosen == decision.played {
                played
            } else {
                return None;
            };
            let mut result =
                PracticeEvaluation::scored(chosen, &decision.best, equity, best, provenance)?;
            result.cube_evaluation = decision.cube_evaluation.clone();
            Some(result)
        }
    }
}

async fn fresh_checker(
    engine: AnalysisEngine,
    plies: i32,
    position: &Position,
    board: &[i32; 26],
) -> Result<PracticeEvaluation> {
    let (name, depth, chosen, best, chosen_eq, best_eq) = match engine {
        AnalysisEngine::Gnubg(engine) => {
            let analysis = engine
                .analyze_checker_choices(
                    &position.position_id,
                    &position.gnubg_match_id,
                    plies,
                    false,
                )
                .await?;
            let best = analysis
                .candidates
                .iter()
                .max_by(|a, b| {
                    a.equity
                        .total_cmp(&b.equity)
                        .then_with(|| b.rank.cmp(&a.rank))
                })
                .context("GNUbg omitted the best checker move")?;
            let chosen = analysis
                .candidates
                .iter()
                .find(|c| after_play(position, &c.notation).as_ref() == Some(board))
                .context("GNUbg omitted the submitted checker move")?;
            (
                analysis.engine,
                analysis.plies,
                chosen.notation.clone(),
                best.notation.clone(),
                chosen.equity,
                best.equity,
            )
        }
        AnalysisEngine::OpenSage(engine) => {
            let candidates = engine
                .checker_boards(
                    &position.position_id,
                    &position.gnubg_match_id,
                    None,
                    Some(*board),
                )
                .await?;
            let best = candidates
                .first()
                .context("Open Sage omitted the best checker move")?;
            let chosen = candidates
                .iter()
                .find(|c| &c.board == board)
                .context("Open Sage omitted the submitted checker move")?;
            let name = engine
                .saved_version()
                .context("Open Sage did not report its version")?;
            (
                name.to_owned(),
                plies,
                chosen.notation.clone(),
                best.notation.clone(),
                chosen.equity,
                best.equity,
            )
        }
    };
    PracticeEvaluation::scored(
        &chosen,
        &best,
        chosen_eq,
        best_eq,
        PracticeProvenance::recomputed(name, depth),
    )
    .context("Invalid checker evaluation")
}

async fn fresh_cube(
    engine: AnalysisEngine,
    plies: i32,
    context: CubeContext,
    answer: CubeAnswer,
) -> Result<PracticeEvaluation> {
    let (name, depth, equities) = match engine {
        AnalysisEngine::Gnubg(engine) => {
            engine
                .analyze_cube_choices(&context.position_id, &context.match_id, plies)
                .await?
        }
        AnalysisEngine::OpenSage(engine) => {
            let cube = engine.cube(&context.position_id, &context.match_id).await?;
            let name = engine
                .saved_version()
                .context("Open Sage did not report its version")?;
            (
                name.to_owned(),
                plies,
                CubeEquities {
                    no_double: cube.no_double,
                    take: cube.take,
                    pass: cube.pass,
                    probabilities: cube.probabilities.as_slice().try_into().ok(),
                },
            )
        }
    };
    // Engines report the doubler's view; a response is graded from the taker's.
    let response = matches!(answer, CubeAnswer::Response(_));
    let sign = if response { -1.0 } else { 1.0 };
    let cube_evaluation = CubeEvaluation {
        unit: CubeEquityUnit::Cubeful,
        no_double: sign * equities.no_double,
        double_take: sign * equities.take,
        double_pass: sign * equities.pass,
        probabilities: if response {
            equities
                .probabilities
                .map(crate::engine::flip_probabilities)
        } else {
            equities.probabilities
        },
    };
    let (best, chosen_eq, best_eq) = answer.options(equities).context("Invalid cube options")?;
    let mut result = PracticeEvaluation::scored(
        answer.notation(),
        best,
        chosen_eq,
        best_eq,
        PracticeProvenance::recomputed(name, depth),
    )
    .context("Invalid cube evaluation")?;
    result.cube_evaluation = Some(cube_evaluation);
    Ok(result)
}

pub(crate) async fn evaluate(
    state: &AppState,
    kind: &str,
    position: &Position,
    solution: SavedEvaluation<'_>,
    answer: &PracticeAnswer,
) -> ApiResult<PracticeEvaluation> {
    let checked = validate(kind, position, answer)?;
    if matches!(checked, CheckedAnswer::ForcedPass) {
        return Ok(PracticeEvaluation::forced_pass());
    }
    if let Some(result) = stored_evaluation(position, &solution, &checked) {
        return Ok(result);
    }
    evaluate_fresh(state, position, checked, None).await
}

pub(crate) async fn evaluate_with_snapshot(
    state: &AppState,
    kind: &str,
    position: &Position,
    answer: &PracticeAnswer,
    snapshot: (i32, crate::settings::EngineSnapshot),
) -> ApiResult<PracticeEvaluation> {
    evaluate_fresh(
        state,
        position,
        validate(kind, position, answer)?,
        Some(snapshot),
    )
    .await
}

async fn evaluate_fresh(
    state: &AppState,
    position: &Position,
    checked: CheckedAnswer,
    snapshot: Option<(i32, crate::settings::EngineSnapshot)>,
) -> ApiResult<PracticeEvaluation> {
    let _permit = practice::EVALUATION.try_lock().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "An exercise is already being evaluated; try again".into(),
        )
    })?;
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        crate::engine::with_shared_access(async {
            let (plies, snapshot) = match snapshot {
                Some(snapshot) => snapshot,
                None => crate::settings::snapshot_for_practice(state).await?,
            };
            let engine = snapshot.engine()?;
            match checked {
                CheckedAnswer::Checker { board } => {
                    fresh_checker(engine, plies, position, &board).await
                }
                CheckedAnswer::Cube { context, answer } => {
                    fresh_cube(engine, plies, context, answer).await
                }
                CheckedAnswer::ForcedPass => Ok(PracticeEvaluation::forced_pass()),
            }
        }),
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::GATEWAY_TIMEOUT,
            "Exercise evaluation timed out; try again".into(),
        )
    })?;
    result.map_err(|error| {
        tracing::warn!(%error, "Could not evaluate study answer");
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Exercise evaluation is unavailable".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, engines::SageEngine, practice::tests::position};
    use std::time::Instant;

    #[test]
    fn evaluation_uses_existing_threshold_and_rejects_incompatible_values() {
        let score = |chosen, best| {
            PracticeEvaluation::scored(
                "choice",
                "best",
                chosen,
                best,
                PracticeProvenance::recomputed("Reference".into(), 2),
            )
        };
        assert_eq!(
            score(0.300, 0.339999).unwrap().verdict,
            PracticeVerdict::Playable
        );
        assert_eq!(
            score(0.300, 0.340000).unwrap().verdict,
            PracticeVerdict::Error
        );
        assert_eq!(
            score(0.340, 0.340000).unwrap().verdict,
            PracticeVerdict::Best
        );
        assert!(score(0.400, 0.340000).is_none());
        assert!(score(f64::NAN, 0.340000).is_none());
    }

    async fn native_checker_and_cube(engine: AnalysisEngine) {
        let checker = position(0, [3, 1]);
        let low_ranked = after_play(&checker, "8/7 8/5").unwrap();
        let initial = engine
            .analyze(&checker.position_id, &checker.gnubg_match_id, 2, None)
            .await
            .unwrap();
        assert!(
            initial
                .candidates
                .iter()
                .take(5)
                .all(|c| after_play(&checker, &c.notation) != Some(low_ranked))
        );
        let start = Instant::now();
        let result = fresh_checker(engine.clone(), 2, &checker, &low_ranked)
            .await
            .unwrap();
        println!(
            "{} 2-ply checker: {:?}",
            result.provenance.engine.as_deref().unwrap(),
            start.elapsed()
        );
        assert_eq!(after_play(&checker, &result.chosen), Some(low_ranked));
        assert_eq!(
            after_play(&checker, &result.best),
            after_play(&checker, "8/5 6/5")
        );
        assert!(result.equity_loss > 0.25);

        // The saved card encodes the responder, with a cube of 2 before the offer.
        let response = position(1, [0, 0]);
        let start = Instant::now();
        let result = fresh_cube(
            engine.clone(),
            2,
            practice::cube_context(&response, true).unwrap(),
            CubeAnswer::Response(CubeResponseAction::Drop),
        )
        .await
        .unwrap();
        println!(
            "{} 2-ply cube response: {:?}",
            result.provenance.engine.as_deref().unwrap(),
            start.elapsed()
        );
        assert_eq!(result.best, "take");
        assert!((result.chosen_equity.unwrap() + 1.0).abs() < 0.000001);
        assert!(result.equity_loss > 0.50);

        let offer = position(0, [0, 0]);
        let result = fresh_cube(
            engine,
            2,
            practice::cube_context(&offer, false).unwrap(),
            CubeAnswer::Offer(CubeOfferAction::Double),
        )
        .await
        .unwrap();
        assert_eq!(result.best, "no double");
        assert!(result.equity_loss > 0.04);
    }

    #[tokio::test]
    #[ignore = "requires a native GNUbg installation"]
    async fn native_gnubg_grades_checker_offer_and_response() {
        let path = std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into());
        native_checker_and_cube(AnalysisEngine::Gnubg(Engine::new(path))).await;
    }

    #[tokio::test]
    #[ignore = "requires a locked native Open Sage installation"]
    async fn native_open_sage_grades_checker_offer_and_response() {
        let python =
            std::env::var("BGTUTOR_SAGE_PYTHON").expect("Set the Open Sage venv Python path");
        let profile = SageEngine::probe_profile(python, 2, 2).await.unwrap();
        native_checker_and_cube(AnalysisEngine::OpenSage(SageEngine::new(profile).unwrap())).await;
    }
}
