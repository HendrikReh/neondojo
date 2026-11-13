use super::*;
use crate::{game_plan::GamePlan, recurring_mistakes::source};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TransferRequest {
    player: String,
    database_id: Option<Uuid>,
    game_plan: GamePlan,
    kind: CardKind,
    #[serde(default)]
    follow_up: bool,
}

#[derive(Serialize, ToSchema, FromRow)]
pub(super) struct TransferSummary {
    id: Uuid,
    #[sqlx(json)]
    scope: TransferRequest,
    completed_at: DateTime<Utc>,
    scored: i64,
    correct: i64,
    revealed: i64,
    mean_loss: Option<f64>,
    duration_seconds: i64,
    unsure: i64,
    lessons: Vec<String>,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/study/transfer-rounds", get(history).post(start))
}

#[utoipa::path(post,path="/api/study/transfer-rounds",request_body=TransferRequest,responses((status=201,body=rounds::PracticeRound),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn start(
    State(state): State<AppState>,
    LearningJson(mut body): LearningJson<TransferRequest>,
) -> ApiResult<(StatusCode, Json<rounds::PracticeRound>)> {
    body.player = body.player.trim().to_owned();
    if body.player.is_empty()
        || body.player.chars().count() > 240
        || body.player.chars().any(char::is_control)
    {
        return Err(ApiError::invalid("Choose a valid exact player name"));
    }
    let player = body.player.to_lowercase();
    let candidates = source::load(&state.pool, body.database_id, None, Some(&player)).await?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("LOCK TABLE transfer_practice_seen IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let candidate_matches: Vec<_> = candidates
        .iter()
        .map(|row| row.question.position.match_uuid)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let trained_matches: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT DISTINCT (payload#>>'{source,match_id}')::uuid FROM learning_cards
         WHERE (payload#>>'{source,match_id}')::uuid=ANY($1)",
    )
    .bind(candidate_matches)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .into_iter()
    .collect();
    let mut questions = Vec::new();
    for row in candidates
        .into_iter()
        .filter(|row| row.in_scope(&player, Some(body.game_plan)))
    {
        let q = row.question;
        if q.kind != body.kind.as_str() || trained_matches.contains(&q.position.match_uuid) {
            continue;
        }
        let (Some(job), Some(plies), Some(original)) =
            (q.reference.job_id, q.plies, row.original_match_state)
        else {
            continue;
        };
        let mut position = q.position;
        position.gnubg_match_id = original;
        let valid = if q.kind == "checker" {
            crate::practice::validate_checker_context(&position).is_ok()
        } else {
            crate::practice::cube_context(&position, q.kind == "cube_response").is_ok()
                && (q.kind == "cube_response"
                    || crate::match_analysis::cube_analysis_available(
                        &position.board,
                        &position.gnubg_match_id,
                        position.player,
                    )
                    .unwrap_or(false))
        };
        if valid {
            questions.push(content::match_card(
                &q.kind,
                position,
                q.players,
                job,
                q.engine,
                plies,
                row.decision,
            ));
        }
    }
    questions.sort_by_cached_key(|_| Uuid::new_v4());
    let signatures: Vec<_> = questions.iter().map(card_signature).collect();
    let mut seen: HashSet<String> = sqlx::query_scalar(
        "SELECT signature FROM transfer_practice_seen WHERE signature=ANY($1)
         UNION SELECT signature FROM learning_cards WHERE signature=ANY($1)",
    )
    .bind(signatures)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .into_iter()
    .collect();
    let mut groups: HashMap<String, Vec<CardContent>> = HashMap::new();
    for card in questions {
        let decision = card
            .solution
            .decision
            .as_ref()
            .ok_or_else(|| ApiError::invalid("Missing transfer analysis"))?;
        let key = if card.kind == "checker" {
            "checker"
        } else {
            &decision.best
        };
        if seen.insert(card_signature(&card)) {
            let group = groups.entry(key.to_owned()).or_default();
            if group.len() < 10 {
                group.push(card);
            }
        }
    }
    let mut selected = if matches!(body.kind, CardKind::Checker) {
        groups.remove("checker").unwrap_or_default()
    } else {
        let (left, right) = if matches!(body.kind, CardKind::CubeOffer) {
            ("double", "no double")
        } else {
            ("take", "drop")
        };
        let mut left = groups.remove(left).unwrap_or_default();
        let mut right = groups.remove(right).unwrap_or_default();
        let count = left.len().min(right.len()).min(5);
        left.truncate(count);
        right.truncate(count);
        left.extend(right);
        left
    };
    if selected.is_empty() {
        return Err(ApiError::invalid(
            "There are not enough unseen comparable positions with saved analysis",
        ));
    }
    selected.sort_by_cached_key(|_| Uuid::new_v4());
    let selected_signatures: Vec<_> = selected.iter().map(card_signature).collect();
    sqlx::query("INSERT INTO transfer_practice_seen(signature) SELECT unnest($1::text[])")
        .bind(selected_signatures)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let metadata = serde_json::to_value(body).map_err(anyhow::Error::from)?;
    Ok((
        StatusCode::CREATED,
        rounds::start_transfer(tx, metadata, selected).await?,
    ))
}

#[utoipa::path(get,path="/api/study/transfer-rounds",responses((status=200,body=Vec<TransferSummary>)),tag="Learning")]
pub(super) async fn history(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<TransferSummary>>> {
    let rows = sqlx::query_as("SELECT r.id,r.transfer AS scope,r.completed_at,
        greatest(0,extract(epoch FROM r.completed_at - r.started_at)::bigint) AS duration_seconds,
        count(*) FILTER (WHERE i.result#>>'{evaluation,verdict}' IN ('best','playable','error')) AS scored,
        count(*) FILTER (WHERE i.result#>>'{evaluation,verdict}' IN ('best','playable')) AS correct,
        count(*) FILTER (WHERE i.result->>'evaluation' IS NULL) AS revealed,
        count(*) FILTER (WHERE i.result->>'confidence'='false') AS unsure,
        coalesce(array_agg(i.lesson ORDER BY i.ordinal) FILTER (WHERE i.lesson IS NOT NULL), '{}') AS lessons,
        avg((i.result#>>'{evaluation,equity_loss}')::double precision)
            FILTER (WHERE i.result#>>'{evaluation,verdict}' IN ('best','playable','error')) AS mean_loss
        FROM card_practice_rounds r JOIN card_practice_items i ON i.round_id=r.id AND i.pass=1
        WHERE r.transfer IS NOT NULL AND r.completed_at IS NOT NULL
        GROUP BY r.id ORDER BY r.completed_at DESC LIMIT 100")
        .fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(rows))
}
