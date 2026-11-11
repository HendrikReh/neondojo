//! Game-bounded checker assessments with explicit source and evaluation provenance.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    imported_analysis::ImportedAnalysisData,
    model::{Job, MoveSeverity, Position},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::collections::HashMap;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

// Matches the supported import limit. Every SQL result is bounded before any
// notation is replayed; this endpoint never starts engine work.
const MAX_GAME_POSITIONS: usize = 100_000;

#[derive(Debug, Deserialize, IntoParams)]
pub struct MoveAssessmentQuery {
    /// One source game, required so loading history never scans the whole match.
    pub game_no: i32,
    /// Selected native match-analysis job; its scope must include this game.
    pub job_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentSource {
    Native,
    Imported,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MoveAssessment {
    /// Persisted checker-position UUID, not a GNUbg position encoding.
    pub position_id: Uuid,
    pub severity: MoveSeverity,
    /// Actual best-minus-played equity, never a default for missing evaluation.
    pub equity_loss: f64,
    pub source: AssessmentSource,
    pub engine: String,
    pub depth: Option<String>,
    pub depth_plies: Option<i32>,
    /// Native match-analysis or individual-hint job that supplied this result.
    pub job_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GameMoveAssessments {
    pub match_id: Uuid,
    pub game_no: i32,
    /// Requested job identity, null when the caller allowed automatic selection.
    pub job_id: Option<Uuid>,
    /// Unknown, unavailable, invalid and non-comparable evaluations are omitted.
    pub items: Vec<MoveAssessment>,
}

#[derive(FromRow)]
struct NativeRow {
    position_uuid: Uuid,
    job_id: Uuid,
    plies: i32,
    engine: String,
    equity_loss: f64,
    data: Value,
}

#[derive(FromRow)]
struct HintRow {
    position_uuid: Uuid,
    job_id: Uuid,
    plies: i32,
    result: Value,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/move-assessments", get(assessments))
}

#[utoipa::path(get, path="/api/matches/{id}/move-assessments", params(("id"=Uuid,Path),MoveAssessmentQuery), responses((status=200,body=GameMoveAssessments),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn assessments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<MoveAssessmentQuery>,
) -> ApiResult<Json<GameMoveAssessments>> {
    crate::play::require_review_available(&state.pool, id).await?;
    if query.game_no <= 0 {
        return Err(ApiError::invalid("game_no must be positive"));
    }
    // Reuse the established lazy source catalog for pre-catalog libraries.
    if !crate::imported_analysis::ensure_catalog(&state.pool, id).await? {
        return Err(ApiError::missing());
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let game_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM match_games WHERE match_uuid=$1 AND game_no=$2)",
    )
    .bind(id)
    .bind(query.game_no)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if !game_exists {
        return Err(ApiError::missing());
    }
    if let Some(job_id) = query.job_id {
        let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
        if job.kind != "match"
            || job.match_uuid != Some(id)
            || job.game_no.is_some_and(|game| game != query.game_no)
        {
            return Err(ApiError::invalid(
                "job_id must be a match analysis of this match and game",
            ));
        }
    }
    let limit = (MAX_GAME_POSITIONS + 1) as i64;
    let positions: Vec<Position> = sqlx::query_as(
        "SELECT * FROM positions WHERE match_uuid=$1 AND game_no=$2 ORDER BY ordinal LIMIT $3",
    )
    .bind(id)
    .bind(query.game_no)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let native: Vec<NativeRow> = sqlx::query_as("SELECT DISTINCT ON (ma.position_uuid) ma.position_uuid,j.id AS job_id,j.plies,coalesce(nullif(j.result->>'engine',''),'GNU Backgammon') AS engine,ma.equity_loss,ma.data FROM positions p JOIN move_analyses ma ON ma.position_uuid=p.id AND ma.game_no=p.game_no AND ma.player=p.player AND ma.kind='checker' JOIN jobs j ON j.id=ma.job_id AND j.kind='match' AND j.match_uuid=p.match_uuid AND (j.game_no IS NULL OR j.game_no=p.game_no) JOIN match_analysis_games c ON c.job_id=j.id AND c.game_no=p.game_no WHERE p.match_uuid=$1 AND p.game_no=$2 AND (($3::uuid IS NOT NULL AND j.id=$3) OR ($3::uuid IS NULL AND j.status='completed')) ORDER BY ma.position_uuid,j.plies DESC,j.created_at DESC,j.id DESC,ma.id DESC LIMIT $4")
        .bind(id).bind(query.game_no).bind(query.job_id).bind(limit).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    // As for existing single-position solutions, select the strongest completed
    // individual hint first. An unusable strongest hint cannot revive an older one.
    let hints: Vec<HintRow> = sqlx::query_as("SELECT DISTINCT ON (p.id) p.id AS position_uuid,j.id AS job_id,j.plies,j.result FROM positions p JOIN jobs j ON j.position_uuid=p.id AND j.kind='analysis' AND j.status='completed' AND j.result IS NOT NULL WHERE p.match_uuid=$1 AND p.game_no=$2 ORDER BY p.id,j.plies DESC,j.created_at DESC,j.id DESC LIMIT $3")
        .bind(id).bind(query.game_no).bind(limit).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    let imported: Vec<(Uuid,Value)> = sqlx::query_as("SELECT p.id,a.data FROM positions p JOIN imported_analyses a ON a.position_uuid=p.id AND a.match_uuid=p.match_uuid AND a.game_no=p.game_no AND a.kind='checker' WHERE p.match_uuid=$1 AND p.game_no=$2 ORDER BY p.ordinal,a.source_ordinal LIMIT $3")
        .bind(id).bind(query.game_no).bind(limit).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    if [positions.len(), native.len(), hints.len(), imported.len()]
        .iter()
        .any(|n| *n > MAX_GAME_POSITIONS)
    {
        return Err(ApiError::invalid(
            "Game assessments exceed the supported position count",
        ));
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    let job_id = query.job_id;
    let items = tokio::task::spawn_blocking(move || {
        let mut native: HashMap<_, _> = native
            .into_iter()
            .map(|row| (row.position_uuid, row))
            .collect();
        let mut hints: HashMap<_, _> = hints
            .into_iter()
            .map(|row| (row.position_uuid, row))
            .collect();
        let mut sources: HashMap<_, Vec<ImportedAnalysisData>> = HashMap::new();
        for (position, data) in imported {
            // A malformed source record is unknown, not an endpoint failure.
            if let Ok(record) = serde_json::from_value(data) {
                sources.entry(position).or_default().push(record);
            }
        }
        positions
            .into_iter()
            .filter_map(|position| {
                let selected = native
                    .remove(&position.id)
                    .and_then(|row| native_assessment(&position, row));
                let mut hint = || {
                    hints
                        .remove(&position.id)
                        .and_then(|row| hint_assessment(&position, row))
                };
                let native = if job_id.is_some() {
                    selected.or_else(hint)
                } else {
                    hint().or(selected)
                };
                native.or_else(|| {
                    sources.remove(&position.id).and_then(|records| {
                        records
                            .into_iter()
                            .find_map(|record| imported_assessment(&position, record))
                    })
                })
            })
            .collect()
    })
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(GameMoveAssessments {
        match_id: id,
        game_no: query.game_no,
        job_id,
        items,
    }))
}

fn rounded_loss(played: f64, best: f64) -> Option<f64> {
    let loss = ((best - played) * 1_000_000.0).round() / 1_000_000.0;
    (played.is_finite() && best.is_finite() && loss.is_finite() && loss >= 0.0).then_some(loss)
}

fn severity(loss: f64) -> MoveSeverity {
    if loss >= 0.16 {
        MoveSeverity::VeryBad
    } else if loss >= 0.08 {
        MoveSeverity::Bad
    } else if loss >= 0.04 {
        MoveSeverity::Doubtful
    } else {
        MoveSeverity::None
    }
}

fn native_assessment(position: &Position, row: NativeRow) -> Option<MoveAssessment> {
    let data = &row.data;
    if data["kind"] != "checker"
        || data["player"].as_i64()? != i64::from(position.player)
        || data["best"].as_str()?.trim().is_empty()
        || data["played"].as_str()?.trim().is_empty()
        || !row.equity_loss.is_finite()
        || row.equity_loss < 0.0
    {
        return None;
    }
    let actual = after_play(position, &position.notation)?;
    if after_play(position, data["played"].as_str()?)? != actual {
        return None;
    }
    after_play(position, data["best"].as_str()?)?;
    let stored = data["equity_loss"].as_f64()?;
    if !stored.is_finite() || (stored - row.equity_loss).abs() > 1e-9 {
        return None;
    }
    let loss = match data["evaluation_status"].as_str()? {
        "forced" if stored == 0.0 && after_play(position, "Cannot move").is_some() => 0.0,
        "evaluated" => rounded_loss(
            data["played_equity"].as_f64()?,
            data["best_equity"].as_f64()?,
        )?,
        _ => return None,
    };
    if (loss - stored).abs() > 1e-9 {
        return None;
    }
    Some(MoveAssessment {
        position_id: position.id,
        severity: severity(loss),
        equity_loss: loss,
        source: AssessmentSource::Native,
        engine: row.engine,
        depth: Some(format!("{}-ply cubeful", row.plies)),
        depth_plies: Some(row.plies),
        job_id: Some(row.job_id),
    })
}

fn hint_assessment(position: &Position, row: HintRow) -> Option<MoveAssessment> {
    let analysis: crate::engine::Analysis = serde_json::from_value(row.result).ok()?;
    if analysis.engine.trim().is_empty()
        || analysis.plies != row.plies
        || analysis.candidates.len() > 256
    {
        return None;
    }
    let best = analysis
        .candidates
        .iter()
        .find(|candidate| candidate.rank == 1)?;
    if !best.equity.is_finite() || best.notation.trim().is_empty() {
        return None;
    }
    after_play(position, &best.notation)?;
    let actual = after_play(position, &position.notation)?;
    let played = analysis
        .candidates
        .iter()
        .find(|candidate| after_play(position, &candidate.notation) == Some(actual))?;
    let loss = rounded_loss(played.equity, best.equity)?;
    Some(MoveAssessment {
        position_id: position.id,
        severity: severity(loss),
        equity_loss: loss,
        source: AssessmentSource::Native,
        engine: analysis.engine,
        depth: Some(format!("{}-ply cubeful", row.plies)),
        depth_plies: Some(row.plies),
        job_id: Some(row.job_id),
    })
}

fn imported_assessment(
    position: &Position,
    record: ImportedAnalysisData,
) -> Option<MoveAssessment> {
    if record.kind != "checker"
        || record.player != position.player
        || record.game_no != position.game_no
        || record.engine.trim().is_empty()
        || record.candidates.len() > 256
    {
        return None;
    }
    let best = record
        .candidates
        .iter()
        .find(|candidate| candidate.rank == 1)?;
    // MWC and unknown units cannot use normalized-equity severity thresholds.
    if !matches!(best.equity_kind.as_str(), "cubeful" | "cubeless") {
        return None;
    }
    after_play(position, &best.notation)?;
    let actual = after_play(position, &position.notation)?;
    let played = record.candidates.iter().find(|candidate| {
        candidate.equity_kind == best.equity_kind
            && after_play(position, &candidate.notation) == Some(actual)
    })?;
    let loss = rounded_loss(played.equity?, best.equity?)?;
    let engine = record
        .engine_version
        .filter(|version| !version.trim().is_empty())
        .map_or_else(
            || record.engine.clone(),
            |version| format!("{} {version}", record.engine),
        );
    Some(MoveAssessment {
        position_id: position.id,
        severity: severity(loss),
        equity_loss: loss,
        source: AssessmentSource::Imported,
        engine,
        depth: record.depth,
        depth_plies: record.depth_plies,
        job_id: None,
    })
}

/// Resolve reordered, grouped and combined checker notation to the same legal
/// final board. Mere absence of the played move in a top-N hint stays unknown.
fn after_play(position: &Position, notation: &str) -> Option<[i32; 26]> {
    if notation.trim().is_empty() || notation.len() > 160 {
        return None;
    }
    let mut board: [i32; 26] = position.board.as_slice().try_into().ok()?;
    let dice: [i32; 2] = position.dice.as_slice().try_into().ok()?;
    let mut runs = Vec::new();
    if !matches!(
        notation.trim().to_lowercase().as_str(),
        "cannot move" | "can't move" | "dance"
    ) {
        for token in notation.split_whitespace() {
            let (path, count) = if let Some((path, count)) =
                token.strip_suffix(')').and_then(|s| s.rsplit_once('('))
            {
                let count: usize = count.parse().ok()?;
                if !(1..=4).contains(&count) {
                    return None;
                }
                (path, count)
            } else {
                (token, 1)
            };
            let points: Option<Vec<usize>> = path
                .split('/')
                .map(|point| {
                    let point = point.strip_suffix('*').unwrap_or(point);
                    match point.to_lowercase().as_str() {
                        "bar" => Some(25),
                        "off" => Some(0),
                        number => number
                            .parse::<usize>()
                            .ok()
                            .filter(|p| (1..=24).contains(p)),
                    }
                })
                .collect();
            let points = points?;
            if points.len() < 2 {
                return None;
            }
            for _ in 0..count {
                for pair in points.windows(2) {
                    runs.push((pair[0], pair[1]));
                }
            }
            if runs.len() > 4 {
                return None;
            }
        }
    }
    crate::import::sgf::expand_xg_play(&mut board, position.player, dice, &runs).ok()?;
    Some(board)
}

#[derive(OpenApi)]
#[openapi(
    paths(assessments),
    components(schemas(GameMoveAssessments, MoveAssessment, AssessmentSource))
)]
pub struct MoveAssessmentsDoc;
