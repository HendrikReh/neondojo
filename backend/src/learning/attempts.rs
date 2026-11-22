use super::*;

pub(super) async fn active_attempt(
    executor: impl sqlx::PgExecutor<'_>,
    card: &CardRow,
) -> ApiResult<Option<AttemptRow>> {
    Ok(sqlx::query_as(
        "SELECT id AS session_id, attempt_request_id AS request_id, attempt_input AS submitted,
                attempt_result AS evaluation FROM review_sessions
         WHERE card_id=$1 AND card_version=$2 AND attempt_request_id IS NOT NULL",
    )
    .bind(card.id)
    .bind(card.version)
    .fetch_optional(executor)
    .await
    .map_err(anyhow::Error::from)?)
}

pub(super) async fn revealed_card(
    connection: &mut sqlx::PgConnection,
    card: &CardRow,
    session_id: Uuid,
    now: DateTime<Utc>,
) -> ApiResult<RevealedCard> {
    Ok(RevealedCard {
        card_id: card.id,
        version: card.version,
        answer: card.answer(connection).await?,
        intervals: scheduler::previews(card, now)?,
        session_id,
    })
}

pub(super) async fn attempted_card(
    connection: &mut sqlx::PgConnection,
    card: &CardRow,
    attempt: AttemptData,
) -> ApiResult<AttemptedCard> {
    Ok(AttemptedCard {
        revealed: revealed_card(connection, card, attempt.session_id, Utc::now()).await?,
        submitted: attempt.submitted,
        evaluation: attempt.evaluation,
    })
}

pub(super) async fn evaluate_card(
    state: &AppState,
    card: &CardContent,
    answer: &PracticeAnswer,
) -> ApiResult<PracticeEvaluation> {
    evaluation::evaluate(
        state,
        &card.kind,
        &card.position,
        SavedEvaluation {
            engine: &card.solution.engine,
            plies: card.solution.plies,
            decision: card.solution.decision.as_ref(),
            candidates: &card.solution.candidates,
            native: card.solution.imported.is_none(),
        },
        answer,
    )
    .await
}

pub(super) async fn load_attempt_card(
    executor: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    expected_version: i64,
    lock: bool,
) -> ApiResult<CardRow> {
    let sql = if lock {
        "SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM learning_cards WHERE id=$1"
    };
    let row: CardRow = sqlx::query_as(sql)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if row.version != expected_version {
        return Err(conflict("Card changed; reload it before answering"));
    }
    Ok(row)
}
