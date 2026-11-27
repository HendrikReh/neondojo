use super::*;

pub(super) async fn summary(pool: &PgPool) -> ApiResult<LearningSummary> {
    #[derive(FromRow)]
    struct SummaryRow {
        card_count: i64,
        due_count: i64,
        review_due_count: i64,
        new_count: i64,
        next_due_at: Option<DateTime<Utc>>,
        deck_id: Option<Uuid>,
        deck_name: Option<String>,
    }

    let allowance = daily::allowance(pool).await.map_err(anyhow::Error::from)?;
    let row: SummaryRow = sqlx::query_as(
        "WITH counts AS (
             SELECT count(*) AS card_count,
                    count(*) FILTER (WHERE c.due_at <= now()) AS due_count,
                    count(*) FILTER (WHERE c.due_at <= now()
                        AND c.review_count + c.anki_review_count > 0) AS review_due_count,
                    count(*) FILTER (WHERE c.due_at <= now()
                        AND c.review_count + c.anki_review_count = 0) AS new_count,
                    min(c.due_at) FILTER (WHERE c.due_at > now()) AS next_due_at
             FROM learning_cards c
             WHERE EXISTS (SELECT 1 FROM deck_cards dc WHERE dc.card_id = c.id)
         ), oldest AS (
             SELECT c.id FROM learning_cards c
             WHERE c.due_at <= now()
               AND (c.review_count + c.anki_review_count > 0 OR $1)
               AND EXISTS (SELECT 1 FROM deck_cards dc WHERE dc.card_id = c.id)
             ORDER BY (c.review_count + c.anki_review_count = 0), c.due_at, c.id LIMIT 1
         ), next_deck AS (
             SELECT d.id, d.name FROM oldest
             JOIN deck_cards dc ON dc.card_id = oldest.id
             JOIN decks d ON d.id = dc.deck_id
             ORDER BY d.created_at, d.id LIMIT 1
         )
         SELECT counts.*, next_deck.id AS deck_id, next_deck.name AS deck_name
         FROM counts LEFT JOIN next_deck ON true",
    )
    .bind(allowance.remaining() > 0)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;

    let new_available_today = row.new_count.min(allowance.remaining());
    Ok(LearningSummary {
        card_count: row.card_count,
        due_count: row.due_count,
        review_due_count: row.review_due_count,
        new_count: row.new_count,
        new_cards_per_day: allowance.per_day,
        new_available_today,
        today_count: row.review_due_count + new_available_today,
        next_due_at: row.next_due_at,
        next_deck: row
            .deck_id
            .zip(row.deck_name)
            .map(|(id, name)| StudyDeck { id, name }),
    })
}

pub(super) async fn next(pool: &PgPool, id: Uuid) -> ApiResult<NextCard> {
    require_deck(pool, id).await?;
    let allowance = daily::allowance(pool).await.map_err(anyhow::Error::from)?;
    let (total, due, new): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),
                count(*) FILTER(WHERE due_at<=now() AND review_count+anki_review_count>0),
                count(*) FILTER(WHERE due_at<=now() AND review_count+anki_review_count=0)
         FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id WHERE dc.deck_id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let row: Option<DueCardRow> = sqlx::query_as(
        "SELECT due.*, (SELECT jsonb_build_object('session_id',s.id,'submitted',s.attempt_input,
                    'evaluation',s.attempt_result) FROM review_sessions s
                    WHERE s.card_id=due.id AND s.card_version=due.version
                      AND s.attempt_request_id IS NOT NULL) AS active_attempt
         FROM (SELECT c.* FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
               WHERE dc.deck_id=$1 AND c.due_at<=now()
                 -- Due reviews come first. A new card needs today's allowance,
                 -- unless its answer has already been checked.
                 AND (c.review_count+c.anki_review_count>0 OR $2
                      OR EXISTS(SELECT 1 FROM review_sessions s
                                WHERE s.card_id=c.id AND s.card_version=c.version
                                  AND s.attempt_request_id IS NOT NULL))
               ORDER BY (c.review_count+c.anki_review_count=0),c.due_at,c.id LIMIT 1) due",
    )
    .bind(id)
    .bind(allowance.remaining() > 0)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let (card, active_attempt) = if let Some(row) = row {
        let attempt = if let Some(attempt) = row.active_attempt {
            let mut connection = pool.acquire().await.map_err(anyhow::Error::from)?;
            Some(attempted_card(&mut connection, &row.card, attempt).await?)
        } else {
            None
        };
        (Some(row.card.question()), attempt)
    } else {
        (None, None)
    };
    Ok(NextCard {
        card,
        counts: ReviewCounts {
            total,
            due,
            new,
            new_available: new.min(allowance.remaining()),
        },
        active_attempt,
    })
}
