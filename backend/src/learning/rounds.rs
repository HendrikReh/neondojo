use super::*;
use sqlx::{PgConnection, Postgres, QueryBuilder};

#[derive(Serialize, ToSchema)]
pub(super) struct PracticeRound {
    id: Uuid,
    version: i64,
    pass: i16,
    index: i64,
    total: i64,
    card: Option<CardQuestion>,
    missing: bool,
    marked: bool,
    result: Option<RoundResult>,
    summary: RoundSummary,
    transfer: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub(super) struct RoundResult {
    answer: CardAnswer,
    evaluation: Option<PracticeEvaluation>,
    submitted: Option<PracticeAnswer>,
    #[serde(default)]
    confidence: Option<bool>,
}

#[derive(FromRow, Serialize, ToSchema)]
pub(super) struct RoundSummary {
    errors: i64,
    correct: i64,
    revealed: i64,
    skipped: i64,
    retry_count: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RoundVersion {
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RoundAnswer {
    version: i64,
    card_version: i64,
    request_id: Uuid,
    answer: Option<PracticeAnswer>,
    confidence: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RoundAdvance {
    version: i64,
    marked: bool,
    diagnosis: Option<String>,
    lesson: Option<String>,
}

#[derive(FromRow)]
struct Round {
    id: Uuid,
    version: i64,
    pass: i16,
    transfer: Option<serde_json::Value>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct Item {
    ordinal: i32,
    card_id: Uuid,
    answered_by: Option<Uuid>,
    submitted: Option<serde_json::Value>,
    #[sqlx(json(nullable))]
    result: Option<RoundResult>,
    #[sqlx(json(nullable))]
    question: Option<CardContent>,
    marked: bool,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/study/card-rounds", post(start))
        .route("/api/study/card-rounds/{id}", get(read))
        .route("/api/study/card-rounds/{id}/answer", post(answer))
        .route("/api/study/card-rounds/{id}/mark", post(mark))
        .route("/api/study/card-rounds/{id}/advance", post(advance))
        .route("/api/study/card-rounds/{id}/retry", post(retry))
}

#[utoipa::path(post, path="/api/study/card-rounds", params(CardQuery), responses((status=201,body=PracticeRound),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn start(
    State(state): State<AppState>,
    Query(query): Query<CardQuery>,
) -> ApiResult<(StatusCode, Json<PracticeRound>)> {
    let filters = query.validate()?;
    let limit = filters.limit.unwrap_or(10);
    if filters.after.is_some() || !matches!(limit, 10 | 20) {
        return Err(ApiError::invalid(
            "Practice rounds use a limit of 10 or 20 and no cursor",
        ));
    }
    search::prepare_game_plans(&state.pool, &filters).await?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET LOCAL jit=off")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(deck) = filters.deck_id {
        require_deck(&mut *tx, deck).await?;
    }
    let mut sql = QueryBuilder::<Postgres>::new("SELECT c.id FROM learning_cards c");
    search::filters(&mut sql, &filters, filters.deck_id);
    let cards: Vec<Uuid> = sql
        .push(" ORDER BY random() LIMIT ")
        .push_bind(limit)
        .build_query_scalar()
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if cards.is_empty() {
        return Err(ApiError::invalid("No cards match the practice filters"));
    }
    let id = Uuid::new_v4();
    let round = sqlx::query_as("INSERT INTO card_practice_rounds(id) VALUES($1) RETURNING *")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    sqlx::query("INSERT INTO card_practice_items(round_id,pass,ordinal,card_id)
                 SELECT $1,1,ordinality-1,card_id FROM unnest($2::uuid[]) WITH ORDINALITY AS cards(card_id,ordinality)")
        .bind(id).bind(cards).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    let snapshot = snapshot(&mut tx, round).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(snapshot)))
}

async fn lock(connection: &mut PgConnection, id: Uuid) -> ApiResult<Round> {
    sqlx::query_as("SELECT * FROM card_practice_rounds WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

async fn current(connection: &mut PgConnection, round: &Round) -> ApiResult<Option<Item>> {
    Ok(sqlx::query_as(
        "SELECT ordinal,card_id,answered_by,submitted,result,question,marked FROM card_practice_items
                       WHERE round_id=$1 AND pass=$2 AND NOT advanced ORDER BY ordinal LIMIT 1",
    )
    .bind(round.id)
    .bind(round.pass)
    .fetch_optional(connection)
    .await
    .map_err(anyhow::Error::from)?)
}

async fn load_card(connection: &mut PgConnection, id: Uuid) -> ApiResult<Option<CardRow>> {
    Ok(sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(anyhow::Error::from)?)
}

async fn item_card(connection: &mut PgConnection, item: &Item) -> ApiResult<Option<CardRow>> {
    if let Some(question) = &item.question {
        return Ok(Some(CardRow {
            id: item.card_id,
            version: 0,
            payload: serde_json::to_value(question).map_err(anyhow::Error::from)?,
            due_at: DateTime::UNIX_EPOCH,
            review_count: 0,
            anki_review_count: 0,
            stability: None,
            difficulty: None,
            last_review: None,
            tags: Vec::new(),
            tags_version: 0,
        }));
    }
    load_card(connection, item.card_id).await
}

pub(super) async fn start_transfer(
    mut tx: sqlx::Transaction<'_, Postgres>,
    metadata: serde_json::Value,
    questions: Vec<CardContent>,
) -> ApiResult<Json<PracticeRound>> {
    let round: Round =
        sqlx::query_as("INSERT INTO card_practice_rounds(id,transfer) VALUES($1,$2) RETURNING *")
            .bind(Uuid::new_v4())
            .bind(metadata)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let mut sql = QueryBuilder::<Postgres>::new(
        "INSERT INTO card_practice_items(round_id,pass,ordinal,card_id,question) ",
    );
    sql.push_values(
        questions.into_iter().enumerate(),
        |mut row, (index, question)| {
            row.push_bind(round.id)
                .push_bind(1_i16)
                .push_bind(index as i32)
                .push_bind(Uuid::new_v4())
                .push_bind(sqlx::types::Json(question));
        },
    );
    sql.build()
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    finish(tx, round).await
}

const RETRY_PREDICATE: &str = "(marked OR result#>>'{evaluation,verdict}'='error')";

async fn snapshot(connection: &mut PgConnection, round: Round) -> ApiResult<PracticeRound> {
    let item = current(connection, &round).await?;
    let card = match &item {
        Some(item) => item_card(connection, item).await?,
        None => None,
    };
    let (total, index) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE advanced) FROM card_practice_items WHERE round_id=$1 AND pass=$2",
    ).bind(round.id).bind(round.pass).fetch_one(&mut *connection).await.map_err(anyhow::Error::from)?;
    let summary = sqlx::query_as(&format!(
        "SELECT count(*) FILTER (WHERE result#>>'{{evaluation,verdict}}'='error') AS errors,
                count(*) FILTER (WHERE result#>>'{{evaluation,verdict}}' IN ('best','playable','forced')) AS correct,
                count(*) FILTER (WHERE result IS NOT NULL AND result->>'evaluation' IS NULL) AS revealed,
                count(*) FILTER (WHERE advanced AND result IS NULL) AS skipped,
                count(*) FILTER (WHERE pass=1 AND {RETRY_PREDICATE}) AS retry_count
         FROM card_practice_items WHERE round_id=$1 AND pass=$2"
    )).bind(round.id).bind(round.pass).fetch_one(connection).await.map_err(anyhow::Error::from)?;
    Ok(PracticeRound {
        id: round.id,
        version: round.version,
        pass: round.pass,
        index,
        total,
        missing: item.is_some() && card.is_none(),
        marked: item.as_ref().is_some_and(|item| item.marked),
        card: card.as_ref().map(CardRow::question).transpose()?,
        result: item.and_then(|item| item.result),
        summary,
        transfer: round.transfer.is_some(),
    })
}

async fn finish(
    mut tx: sqlx::Transaction<'_, Postgres>,
    round: Round,
) -> ApiResult<Json<PracticeRound>> {
    let needs_completion = round.transfer.is_some() && round.completed_at.is_none();
    let snapshot = snapshot(&mut tx, round).await?;
    if needs_completion && snapshot.pass == 1 && snapshot.index == snapshot.total {
        sqlx::query(
            "UPDATE card_practice_rounds SET completed_at=coalesce(completed_at,now()) WHERE id=$1",
        )
        .bind(snapshot.id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(snapshot))
}

fn check_version(round: &Round, version: i64) -> ApiResult<()> {
    if round.version != version {
        return Err(conflict("Practice round changed; reload it"));
    }
    Ok(())
}

async fn bump(connection: &mut PgConnection, round: Round, pass: i16) -> ApiResult<Round> {
    Ok(sqlx::query_as(
        "UPDATE card_practice_rounds SET version=version+1,pass=$2 WHERE id=$1 RETURNING *",
    )
    .bind(round.id)
    .bind(pass)
    .fetch_one(connection)
    .await
    .map_err(anyhow::Error::from)?)
}

#[utoipa::path(get,path="/api/study/card-rounds/{id}",params(("id"=Uuid,Path)),responses((status=200,body=PracticeRound),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PracticeRound>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    finish(tx, round).await
}

#[utoipa::path(post,path="/api/study/card-rounds/{id}/answer",params(("id"=Uuid,Path)),request_body=RoundAnswer,responses((status=200,body=PracticeRound),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn answer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundAnswer>,
) -> ApiResult<Json<PracticeRound>> {
    let submitted = body
        .answer
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(anyhow::Error::from)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    let item = current(&mut tx, &round)
        .await?
        .ok_or_else(|| conflict("Practice pass is complete"))?;
    if item.answered_by == Some(body.request_id) && item.submitted == submitted {
        return finish(tx, round).await;
    }
    check_version(&round, body.version)?;
    if item.result.is_some() {
        return Err(conflict("This practice card was already answered"));
    }
    let card = item_card(&mut tx, &item)
        .await?
        .ok_or_else(ApiError::missing)?;
    if card.version != body.card_version {
        return Err(conflict("Card changed; reload it before answering"));
    }
    let content = if card.is_anki() {
        if body.answer.is_some() {
            return Err(ApiError::invalid(
                "Reveal this Anki card to view the answer",
            ));
        }
        None
    } else {
        Some(card.content()?)
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    if let Some(content) = &content {
        crate::play::require_review_available(&state.pool, content.source.match_id).await?;
    }
    let evaluation = match (&content, &body.answer) {
        (Some(content), Some(input)) => Some(evaluate_card(&state, content, input).await?),
        _ => None,
    };

    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    let item = current(&mut tx, &round)
        .await?
        .ok_or_else(|| conflict("Practice pass is complete"))?;
    if item.answered_by == Some(body.request_id) && item.submitted == submitted {
        return finish(tx, round).await;
    }
    check_version(&round, body.version)?;
    if item.question.is_none() {
        load_attempt_card(&mut *tx, item.card_id, body.card_version, true).await?;
    }
    if let Some(content) = &content {
        crate::play::require_review_available_on(&mut *tx, content.source.match_id).await?;
    }
    let result = RoundResult {
        answer: if let (Some(_), Some(content)) = (&item.question, content) {
            CardAnswer {
                solution: Some(content.solution),
                notes: String::new(),
                tags: Vec::new(),
                played: content.played,
            }
        } else {
            card.answer(&mut tx).await?
        },
        evaluation,
        submitted: body.answer,
        confidence: body.confidence,
    };
    sqlx::query(
        "UPDATE card_practice_items SET answered_by=$4,submitted=$5,result=$6
                 WHERE round_id=$1 AND pass=$2 AND ordinal=$3",
    )
    .bind(id)
    .bind(round.pass)
    .bind(item.ordinal)
    .bind(body.request_id)
    .bind(submitted)
    .bind(sqlx::types::Json(result))
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let pass = round.pass;
    let round = bump(&mut tx, round, pass).await?;
    finish(tx, round).await
}

#[utoipa::path(post,path="/api/study/card-rounds/{id}/advance",params(("id"=Uuid,Path)),request_body=RoundAdvance,responses((status=200,body=PracticeRound),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Learning")]
pub(super) async fn advance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundAdvance>,
) -> ApiResult<Json<PracticeRound>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    check_version(&round, body.version)?;
    let item = current(&mut tx, &round)
        .await?
        .ok_or_else(|| conflict("Practice pass is complete"))?;
    let answered = item.result.is_some();
    if !answered && item_card(&mut tx, &item).await?.is_some() {
        return Err(conflict("Answer the practice card before advancing"));
    }
    let diagnosis = body.diagnosis.filter(|value| !value.is_empty());
    if diagnosis
        .as_deref()
        .is_some_and(|value| !matches!(value, "pattern" | "principle" | "calculation"))
        || body
            .lesson
            .as_ref()
            .is_some_and(|value| value.chars().count() > 500)
    {
        return Err(ApiError::invalid("Invalid practice diagnosis or lesson"));
    }
    sqlx::query("UPDATE card_practice_items SET advanced=true,marked=$4,diagnosis=$5,lesson=$6 WHERE round_id=$1 AND pass=$2 AND ordinal=$3")
        .bind(id).bind(round.pass).bind(item.ordinal).bind(answered && (body.marked || item.marked))
        .bind(diagnosis).bind(body.lesson.filter(|value| !value.trim().is_empty()))
        .execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    let pass = round.pass;
    let round = bump(&mut tx, round, pass).await?;
    finish(tx, round).await
}

#[utoipa::path(post,path="/api/study/card-rounds/{id}/mark",params(("id"=Uuid,Path)),request_body=RoundAdvance,responses((status=200,body=PracticeRound),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Learning")]
pub(super) async fn mark(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundAdvance>,
) -> ApiResult<Json<PracticeRound>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    check_version(&round, body.version)?;
    let item = current(&mut tx, &round)
        .await?
        .filter(|item| round.pass == 1 && item.result.is_some())
        .ok_or_else(|| conflict("Answer the card before marking it"))?;
    sqlx::query(
        "UPDATE card_practice_items SET marked=$4 WHERE round_id=$1 AND pass=$2 AND ordinal=$3",
    )
    .bind(id)
    .bind(round.pass)
    .bind(item.ordinal)
    .bind(body.marked)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let pass = round.pass;
    let round = bump(&mut tx, round, pass).await?;
    finish(tx, round).await
}

#[utoipa::path(post,path="/api/study/card-rounds/{id}/retry",params(("id"=Uuid,Path)),request_body=RoundVersion,responses((status=200,body=PracticeRound),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Learning")]
pub(super) async fn retry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundVersion>,
) -> ApiResult<Json<PracticeRound>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let round = lock(&mut tx, id).await?;
    check_version(&round, body.version)?;
    if round.pass != 1 || current(&mut tx, &round).await?.is_some() {
        return Err(conflict("Finish the first pass before retrying"));
    }
    let inserted = sqlx::query(&format!(
        "INSERT INTO card_practice_items(round_id,pass,ordinal,card_id,question)
         SELECT round_id,2,row_number() OVER (ORDER BY ordinal)-1,card_id,question
         FROM card_practice_items WHERE round_id=$1 AND pass=1 AND {RETRY_PREDICATE}"
    ))
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if inserted.rows_affected() == 0 {
        return Err(conflict("No practice cards to retry"));
    }
    let round = bump(&mut tx, round, 2).await?;
    finish(tx, round).await
}
