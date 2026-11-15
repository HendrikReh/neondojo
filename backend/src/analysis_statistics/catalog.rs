//! Statistics of the chosen catalog result. Imported summaries use the same
//! validated decisions as review; unknown depth and missing coverage stay absent.
use super::{GameAnalysisStatistics, PlayerErrorBreakdown};
use crate::{
    analysis_catalog::{SelectedAnalysis, resolve_selection},
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::{MatchAnalysisSummary, PerformanceRatingSummary, PlayerAnalysisSummary, Position},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct CatalogAnalysisSummary {
    pub engine: String,
    pub plies: Option<i32>,
    pub recorded_depth: Option<String>,
    pub total_games: i32,
    pub completed_games: i32,
    pub total_moves: i32,
    pub analyzed_moves: i32,
    pub players: [PlayerAnalysisSummary; 2],
    pub performance_rating: Option<PerformanceRatingSummary>,
}
impl From<MatchAnalysisSummary> for CatalogAnalysisSummary {
    fn from(s: MatchAnalysisSummary) -> Self {
        Self {
            engine: s.engine,
            plies: Some(s.plies),
            recorded_depth: Some(format!("{}-ply", s.plies)),
            total_games: s.total_games,
            completed_games: s.completed_games,
            total_moves: s.total_moves,
            analyzed_moves: s.analyzed_moves,
            players: s.players,
            performance_rating: s.performance_rating,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct CatalogGameStatistics {
    pub game_no: i32,
    pub white_score: i32,
    pub black_score: i32,
    pub crawford: bool,
    pub analyzed: bool,
    pub summary: Option<CatalogAnalysisSummary>,
    pub error_breakdown: Option<[PlayerErrorBreakdown; 2]>,
}
impl From<GameAnalysisStatistics> for CatalogGameStatistics {
    fn from(g: GameAnalysisStatistics) -> Self {
        Self {
            game_no: g.game_no,
            white_score: g.white_score,
            black_score: g.black_score,
            crawford: g.crawford,
            analyzed: g.analyzed,
            summary: g.summary.map(Into::into),
            error_breakdown: g.error_breakdown,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct MatchAnalysisStatistics {
    pub analysis_id: Option<String>,
    pub summary: Option<CatalogAnalysisSummary>,
    pub games: Vec<CatalogGameStatistics>,
    pub error_breakdown: Option<[PlayerErrorBreakdown; 2]>,
}
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct CatalogStatisticsQuery {
    pub analysis_id: Option<String>,
    pub game_no: Option<i32>,
}
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/analysis-statistics", get(statistics))
}
#[utoipa::path(get,path="/api/matches/{id}/analysis-statistics",params(("id"=Uuid,Path),CatalogStatisticsQuery),responses((status=200,body=MatchAnalysisStatistics),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn statistics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<CatalogStatisticsQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<MatchAnalysisStatistics>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    if query.game_no.is_some_and(|g| g <= 0) {
        return Err(ApiError::invalid("game_no must be positive"));
    }
    // The imported review loader owns a repeatable-read snapshot. Guard its
    // surrounding source metadata reads with the monotonic source revision so
    // a source replacement cannot combine old decisions with new coverage.
    let revision: i64 = sqlx::query_scalar("SELECT source_revision FROM matches WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let selected = resolve_selection(&state.pool, id, query.analysis_id.as_deref()).await?;
    let analysis_id = selected.as_ref().map(|s| s.id());
    if let Some(SelectedAnalysis::Job(job)) = &selected
        && job.kind == "match"
    {
        let result = super::load_statistics(&state, job.id).await?;
        let mut games: Vec<CatalogGameStatistics> =
            result.games.into_iter().map(Into::into).collect();
        let (summary, error_breakdown) = if let Some(game) = query.game_no {
            if let Some(index) = games.iter().position(|g| g.game_no == game) {
                let selected = games.remove(index);
                let summary = selected.summary;
                let errors = selected.error_breakdown;
                // Return a single scope without duplicating/mixing the full-match rating.
                return Ok(Json(MatchAnalysisStatistics {
                    analysis_id,
                    summary,
                    games: vec![],
                    error_breakdown: errors,
                }));
            }
            games.clear();
            (None, None)
        } else {
            (result.summary.map(Into::into), result.error_breakdown)
        };
        return Ok(Json(MatchAnalysisStatistics {
            analysis_id,
            summary,
            games,
            error_breakdown,
        }));
    }
    // A single-position result has no match statistics, even if it contains a loss.
    if !matches!(selected, Some(SelectedAnalysis::Imported { .. })) {
        return Ok(Json(MatchAnalysisStatistics {
            analysis_id,
            summary: None,
            games: vec![],
            error_breakdown: None,
        }));
    }
    let key = analysis_id.as_deref().ok_or_else(ApiError::missing)?;
    let rows =
        crate::review_decisions::load_all_review(&state, id, key, query.game_no, None).await?;
    let source_games = crate::store::match_games(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if query
        .game_no
        .is_some_and(|n| !source_games.iter().any(|g| g.game_no == n))
    {
        return Err(ApiError::missing());
    }
    let source_games: Vec<_> = source_games
        .into_iter()
        .filter(|g| query.game_no.is_none_or(|n| n == g.game_no))
        .collect();
    let positions:Vec<Position>=sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 AND ($2::integer IS NULL OR game_no=$2) ORDER BY ordinal LIMIT 100001")
        .bind(id).bind(query.game_no).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    if positions.len() > 100_000 {
        return Err(ApiError::invalid(
            "Analysis exceeds supported position count",
        ));
    }
    let explicit_cubes:Vec<(i32,i64)>=sqlx::query_as("SELECT game_no,count(*) FROM match_moves WHERE match_uuid=$1 AND kind='cube' AND ($2::integer IS NULL OR game_no=$2) GROUP BY game_no")
        .bind(id).bind(query.game_no).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    let current_revision: i64 =
        sqlx::query_scalar("SELECT source_revision FROM matches WHERE id=$1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
    if current_revision != revision {
        return Err(ApiError::invalid(
            "Match source changed while reading statistics; reload analysis",
        ));
    }
    let data = tokio::task::spawn_blocking(move || -> anyhow::Result<MatchAnalysisStatistics> {
        let engines: BTreeSet<_> = rows.iter().map(|r| r.engine.clone()).collect();
        let depths: BTreeSet<_> = rows.iter().map(|r| r.depth.clone()).collect();
        let plies: BTreeSet<_> = rows.iter().map(|r| r.depth_plies).collect();
        let engine = engines.into_iter().collect::<Vec<_>>().join(" / ");
        let depth = if depths.len() == 1 {
            depths.into_iter().next().flatten()
        } else {
            None
        };
        let plies = if plies.len() == 1 {
            plies.into_iter().next().flatten()
        } else {
            None
        };
        let empty = |games, moves| CatalogAnalysisSummary {
            engine: engine.clone(),
            plies,
            recorded_depth: depth.clone(),
            total_games: games,
            completed_games: 0,
            total_moves: moves,
            analyzed_moves: 0,
            players: [
                PlayerAnalysisSummary::default(),
                PlayerAnalysisSummary {
                    player: 1,
                    ..Default::default()
                },
            ],
            performance_rating: None,
        };
        let error_counts = || {
            [
                PlayerErrorBreakdown::default(),
                PlayerErrorBreakdown {
                    player: 1,
                    ..Default::default()
                },
            ]
        };
        let mut expected_cubes: BTreeMap<_, _> = explicit_cubes.into_iter().collect();
        for p in &positions {
            if crate::match_analysis::cube_analysis_available(
                &p.board,
                &p.gnubg_match_id,
                p.player,
            )? {
                *expected_cubes.entry(p.game_no).or_default() += 1;
            }
        }
        let mut overall = empty(source_games.len() as i32, positions.len() as i32);
        let mut overall_errors = error_counts();
        let mut games = Vec::with_capacity(source_games.len());
        let mut by_game: BTreeMap<i32, Vec<_>> = BTreeMap::new();
        for row in rows {
            by_game.entry(row.game_no).or_default().push(row);
        }
        for g in source_games {
            let mut summary = empty(1, g.position_count);
            let mut errors = error_counts();
            for row in by_game.remove(&g.game_no).unwrap_or_default() {
                let d = &row.decision;
                if !matches!(d.evaluation_status.as_str(), "evaluated" | "forced") {
                    continue;
                }
                for target in [&mut summary, &mut overall] {
                    crate::match_analysis::accumulate_decision(&mut target.players, d)?;
                    target.analyzed_moves += i32::from(d.kind == "checker");
                }
                for target in [&mut errors, &mut overall_errors] {
                    accumulate_errors(target, d);
                }
            }
            let checker_count = summary
                .players
                .iter()
                .map(|p| p.checker_decisions)
                .sum::<i32>();
            let cube_count = summary
                .players
                .iter()
                .map(|p| p.cube_decisions)
                .sum::<i32>();
            let complete = checker_count == g.position_count
                && i64::from(cube_count) == *expected_cubes.get(&g.game_no).unwrap_or(&0);
            summary.completed_games = i32::from(complete);
            overall.completed_games += i32::from(complete);
            let has_results = checker_count + cube_count > 0;
            games.push(CatalogGameStatistics {
                game_no: g.game_no,
                white_score: g.white_score,
                black_score: g.black_score,
                crawford: g.crawford,
                analyzed: complete,
                summary: has_results.then_some(summary),
                error_breakdown: complete.then_some(errors),
            });
        }
        let complete = overall.total_games > 0 && overall.completed_games == overall.total_games;
        let any = overall
            .players
            .iter()
            .any(|p| p.checker_decisions + p.cube_decisions > 0);
        Ok(MatchAnalysisStatistics {
            analysis_id,
            summary: any.then_some(overall),
            games,
            error_breakdown: complete.then_some(overall_errors),
        })
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(data))
}
fn accumulate_errors(errors: &mut [PlayerErrorBreakdown; 2], d: &crate::match_analysis::Decision) {
    let p = &mut errors[d.player as usize];
    let c = if d.kind == "checker" {
        &mut p.checker
    } else {
        &mut p.cube
    };
    match d.severity.as_str() {
        "doubtful" => c.doubtful += 1,
        "bad" => c.bad += 1,
        "very_bad" => c.very_bad += 1,
        _ => {}
    }
}
#[derive(OpenApi)]
#[openapi(
    paths(statistics),
    components(schemas(MatchAnalysisStatistics, CatalogAnalysisSummary, CatalogGameStatistics))
)]
pub struct CatalogStatisticsDoc;
