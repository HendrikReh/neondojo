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

pub(super) async fn submit_card_attempt(
    state: &AppState,
    id: Uuid,
    body: CardAttemptRequest,
) -> ApiResult<AttemptedCard> {
    let row = load_attempt_card(&state.pool, id, body.expected_version, false).await?;
    if let Some(previous) = active_attempt(&state.pool, &row).await? {
        // Recheck under the same lock used to finish a review. The card may
        // have advanced since the preliminary lookup.
        previous.confirm_request(&body)?;
        let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
        let row = load_attempt_card(&mut *tx, id, body.expected_version, true).await?;
        let result = attempted_card(&mut tx, &row, previous.into_data()).await?;
        return Ok(result);
    }
    let Some(card) = row.decision() else {
        return Err(ApiError::invalid("Reveal the Anki card before rating"));
    };
    crate::play::require_review_available(&state.pool, card.source.match_id).await?;
    let evaluation = evaluate_card(state, card, &body.answer).await?;

    // Evaluation can be slow. Serialize only the short write with reviews
    // and card updates and verify the snapshot version again at that point.
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row = load_attempt_card(&mut *tx, id, body.expected_version, true).await?;
    let attempt = if let Some(previous) = active_attempt(&mut *tx, &row).await? {
        previous.confirm_request(&body)?;
        previous.into_data()
    } else {
        crate::play::require_review_available_on(&mut *tx, card.source.match_id).await?;
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO review_sessions(id,card_id,card_version,expires_at,attempt_request_id,attempt_input,attempt_result)
             VALUES($1,$2,$3,now() + interval '1 day',$4,$5,$6)",
        )
        .bind(session_id)
        .bind(id)
        .bind(row.version)
        .bind(body.request_id)
        .bind(sqlx::types::Json(&body.answer))
        .bind(sqlx::types::Json(&evaluation))
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        AttemptData {
            session_id,
            submitted: body.answer,
            evaluation,
        }
    };
    let result = attempted_card(&mut tx, &row, attempt).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(result)
}
