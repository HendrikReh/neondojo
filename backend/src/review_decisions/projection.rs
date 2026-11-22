use super::*;

pub(super) fn filtered_decision(
    row: Row,
    id: Uuid,
    format: &str,
    job: Option<&Job>,
    errors_only: bool,
    filter: Option<MoveSeverity>,
    thresholds: ErrorThresholds,
) -> Option<ReviewDecision> {
    let mut result = project(row, id, format, job)?;
    let severity = review_severity(
        result.decision.evaluation_status == "evaluated",
        result.decision.equity_loss,
        thresholds,
    );
    if !severity_selected(severity, errors_only, filter) {
        return None;
    }
    result.decision.severity = severity.as_str().into();
    Some(result)
}

/// Only evaluated decisions are graded by the error thresholds.
pub(super) fn review_severity(
    evaluated: bool,
    equity_loss: f64,
    thresholds: ErrorThresholds,
) -> MoveSeverity {
    if evaluated {
        thresholds.classify(equity_loss)
    } else {
        MoveSeverity::None
    }
}

pub(super) fn severity_selected(
    severity: MoveSeverity,
    errors_only: bool,
    filter: Option<MoveSeverity>,
) -> bool {
    !(errors_only && severity == MoveSeverity::None) && filter.is_none_or(|grade| grade == severity)
}

pub(super) fn project(
    row: Row,
    match_id: Uuid,
    format: &str,
    job: Option<&Job>,
) -> Option<ReviewDecision> {
    let (decision, move_id, review_position, engine, depth, depth_plies, job_id, source) = if row
        .native
    {
        let job = job?;
        if row.kind == "checker" && row.data["evaluation_status"] != "unavailable" {
            let position: crate::model::Position =
                serde_json::from_value(row.position.clone()?).ok()?;
            crate::move_assessments::native_checker_loss(&position, &row.data, row.equity_loss?)?;
        }
        let decision: Decision = serde_json::from_value(row.data).ok()?;
        if decision.decision_no != row.decision_no
            || decision.kind != row.kind
            || decision.player != row.player
            || row
                .equity_loss
                .is_none_or(|loss| !loss.is_finite() || (loss - decision.equity_loss).abs() > 1e-9)
        {
            return None;
        }
        let valid_status = match decision.evaluation_status.as_str() {
            "evaluated" => source::evaluated(&decision),
            "forced" => {
                decision.kind == "checker"
                    && decision.equity_loss == 0.0
                    && row
                        .position
                        .and_then(|value| {
                            serde_json::from_value::<crate::model::Position>(value).ok()
                        })
                        .is_some_and(|p| {
                            p.player == decision.player
                                && crate::move_assessments::after_play(&p, "Cannot move").is_some()
                        })
            }
            "unavailable" => decision.played_equity.is_none() && decision.best_equity.is_none(),
            _ => false,
        };
        if !valid_status {
            return None;
        }
        let saved_engine = job
            .engine_snapshot
            .as_ref()
            .map(|s| match s.engine_id.as_str() {
                "bgsage" => "NeonGammon",
                "gnubg" => "GNU Backgammon",
                _ => s.engine_id.as_str(),
            })
            .unwrap_or("GNU Backgammon");
        let engine = job
            .result
            .as_ref()
            .and_then(|r| r["engine"].as_str())
            .unwrap_or(saved_engine)
            .to_owned();
        let context = native_cube_context(row.event, row.checker_ply, match_id, &decision);
        let (move_id, review_position) = context.unzip();
        (
            decision,
            move_id,
            review_position,
            engine,
            Some(format!("{}-ply", job.plies)),
            Some(job.plies),
            Some(job.id),
            crate::move_assessments::AssessmentSource::Native,
        )
    } else {
        let event: StoredMove = serde_json::from_value(row.event?).ok()?;
        let source = imported_projection::source_move(&event, row.checker_ply)?;
        let record: ImportedAnalysisData = serde_json::from_value(row.data).ok()?;
        if record.kind != row.kind {
            return None;
        }
        let projected =
            imported_projection::project(&record, &source, match_id, row.position_uuid, format)?;
        let engine = record
            .engine_version
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map_or_else(
                || record.engine.clone(),
                |v| format!("{} {v}", record.engine),
            );
        (
            projected.decision,
            Some(event.id),
            (record.kind == "cube").then_some(projected.position),
            engine,
            projected.depth,
            projected.depth_plies,
            None,
            crate::move_assessments::AssessmentSource::Imported,
        )
    };
    Some(ReviewDecision {
        id: row.id,
        position_uuid: row.position_uuid,
        move_id,
        review_position,
        next_checker_no: row.next_checker_no,
        game_no: row.game_no,
        decision,
        source,
        engine,
        depth,
        depth_plies,
        job_id,
    })
}

pub(super) fn native_cube_context(
    event: Option<serde_json::Value>,
    checker_ply: Option<i32>,
    match_id: Uuid,
    decision: &Decision,
) -> Option<(Uuid, crate::model::Position)> {
    if decision.kind != "cube" {
        return None;
    }
    let event: StoredMove = serde_json::from_value(event?).ok()?;
    let before_roll = decision.played == "no double";
    if event.player != decision.player
        || if before_roll {
            event.kind != "checker" || checker_ply.is_none() || checker_ply != decision.checker_no
        } else {
            event.kind != "cube" || !event.notation.eq_ignore_ascii_case(&decision.played)
        }
    {
        return None;
    }
    let source = imported_projection::source_move(&event, checker_ply)?;
    let position =
        imported_projection::review_position(&source, match_id, event.position_uuid, before_roll)?;
    Some((event.id, position))
}
