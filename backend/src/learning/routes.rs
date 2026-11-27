use super::{anki, *};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/decks", get(decks).post(create_deck))
        .route("/api/cards", get(search::all_cards))
        .route("/api/cards/delete", post(management::delete_cards))
        .route("/api/cards/duplicates", get(management::duplicates))
        .route(
            "/api/cards/duplicates/resolve",
            post(management::resolve_duplicate),
        )
        .route("/api/decks/{id}/move-cards", post(management::move_cards))
        .route(
            "/api/decks/{id}/add-cards",
            post(management::add_deck_cards),
        )
        .route(
            "/api/decks/{id}/remove-cards",
            post(management::remove_cards),
        )
        .route("/api/decks/{id}", put(rename_deck).delete(delete_deck))
        .route("/api/decks/{id}/cards", get(cards).post(add_cards))
        .route("/api/decks/{id}/positions", post(add_positions))
        .route("/api/decks/{id}/decisions", post(add_decisions))
        .route(
            "/api/decks/{id}/card-suggestions",
            get(suggestions::preview).post(suggestions::save),
        )
        .route("/api/decks/{id}/export.apkg", get(export_deck))
        .route("/api/decks/{id}/send-to-anki", post(anki::connect::send))
        .route(
            "/api/decks/{id}/sync-anki-reviews",
            post(anki::review_sync::sync),
        )
        .route(
            "/api/decks/{id}/anki-auto-sync",
            get(anki::auto_sync::settings).put(anki::auto_sync::set_settings),
        )
        .route("/api/study/anki-auto-sync", get(anki::auto_sync::overview))
        .route(
            "/api/decks/{id}/anki-auto-sync/history",
            get(anki::auto_sync::history),
        )
        .route(
            "/api/decks/{id}/anki-auto-sync/action",
            post(anki::auto_sync::act),
        )
        .route("/api/decks/{id}/import.apkg", post(anki::import::import))
        .route("/api/decks/{id}/next", get(next_card))
        .route("/api/study/learning-summary", get(learning_summary))
        .merge(daily::routes())
        .merge(rounds::routes())
        .merge(topics::routes())
        .merge(transfer::routes())
        .merge(exercises::routes())
        .merge(anki_flashcards::routes())
        .route("/api/cards/{id}/reveal", post(reveal))
        .route("/api/cards/{id}/attempt", post(submit_attempt))
        .route("/api/cards/{id}/review", post(review))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        decks,
        create_deck,
        rename_deck,
        delete_deck,
        cards,
        search::all_cards,
        management::duplicates,
        management::resolve_duplicate,
        management::move_cards,
        management::remove_cards,
        management::add_deck_cards,
        management::delete_cards,
        add_cards,
        add_positions,
        add_decisions,
        suggestions::preview,
        suggestions::save,
        export_deck,
        anki::connect::send,
        anki::review_sync::sync,
        anki::auto_sync::settings,
        anki::auto_sync::set_settings,
        anki::auto_sync::overview,
        anki::auto_sync::history,
        anki::auto_sync::act,
        anki::import::import,
        anki_flashcards::face,
        anki_flashcards::media,
        anki_flashcards::upload,
        next_card,
        learning_summary,
        rounds::start,
        rounds::read,
        rounds::answer,
        rounds::mark,
        rounds::advance,
        rounds::retry,
        topics::suggest_tags,
        topics::edit_tags,
        topics::list_topics,
        topics::save_topic,
        topics::delete_topic,
        transfer::start,
        transfer::history,
        exercises::pips,
        exercises::score_questions,
        exercises::score_answer,
        exercises::preview,
        reveal,
        submit_attempt,
        review
    ),
    components(schemas(
        Deck,
        CreateDeck,
        RenameDeck,
        AddCards,
        AddCardsResult,
        anki::import::AnkiUpload,
        anki_flashcards::ImportedAnkiDeck,
        management::DuplicateCard,
        management::DuplicateDeck,
        management::DuplicatePage,
        management::DuplicatePeer,
        management::MoveCards,
        management::MoveCardsResult,
        management::RemoveCards,
        management::AddDeckCards,
        management::AddDeckCardsResult,
        management::DeleteCards,
        management::DeleteCard,
        management::ResolveDuplicate,
        AddPositions,
        AddPositionsResult,
        AddDecisions,
        AddDecisionsResult,
        DecisionAddItem,
        DecisionAddStatus,
        PositionAddItem,
        PositionAddStatus,
        suggestions::SuggestionReference,
        suggestions::CardSuggestion,
        suggestions::CardSuggestions,
        suggestions::SaveSuggestions,
        CardSource,
        CardSolution,
        CardAnswer,
        CardQuestion,
        CardPage,
        CardBrowseItem,
        CardBrowseContext,
        CardDeck,
        CardKind,
        CardReviewState,
        CardQuery,
        topics::TagCard,
        topics::EditCardTags,
        topics::CardTags,
        topics::SaveCardTopic,
        topics::CardTopic,
        transfer::TransferRequest,
        transfer::TransferSummary,
        exercises::PipAnswer,
        exercises::PipResult,
        exercises::ScoreQuestion,
        exercises::ScorePractice,
        exercises::ScoreAnswer,
        ReviewCounts,
        NextCard,
        LearningSummary,
        StudyDeck,
        RevealCard,
        RatingIntervals,
        RevealedCard,
        CardAttemptRequest,
        AttemptedCard,
        Rating,
        ReviewCard,
        ReviewResult,
        rounds::PracticeRound,
        rounds::RoundResult,
        rounds::RoundSummary,
        rounds::RoundAnswer,
        rounds::RoundAdvance,
        rounds::RoundVersion
    ))
)]
pub struct LearningDoc;

/// OpenAPI fragment for the daily new-card allowance.
pub fn daily_doc() -> utoipa::openapi::OpenApi {
    daily::DailyDoc::openapi()
}

#[utoipa::path(get,path="/api/decks",responses((status=200,body=Vec<Deck>)),tag="Learning")]
pub(super) async fn decks(State(state): State<AppState>) -> ApiResult<Json<Vec<Deck>>> {
    Ok(Json(sqlx::query_as("SELECT d.id,d.name,d.version,d.created_at,count(c.id) AS card_count,count(c.id) FILTER (WHERE c.due_at<=now()) AS due_count FROM decks d LEFT JOIN deck_cards dc ON dc.deck_id=d.id LEFT JOIN learning_cards c ON c.id=dc.card_id GROUP BY d.id ORDER BY d.created_at,d.id").fetch_all(&state.pool).await.map_err(anyhow::Error::from)?))
}

#[utoipa::path(post,path="/api/decks",request_body=CreateDeck,responses((status=201,body=Deck),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn create_deck(
    State(state): State<AppState>,
    Json(body): Json<CreateDeck>,
) -> ApiResult<(StatusCode, Json<Deck>)> {
    let name = deck_name(&body.name)?;
    let deck=sqlx::query_as("INSERT INTO decks(id,name) VALUES($1,$2) RETURNING id,name,version,created_at,0::bigint AS card_count,0::bigint AS due_count").bind(Uuid::new_v4()).bind(name).fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(deck)))
}

#[utoipa::path(put,path="/api/decks/{id}",params(("id"=Uuid,Path)),request_body=RenameDeck,responses((status=200,body=Deck),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn rename_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
    Json(body): Json<RenameDeck>,
) -> ApiResult<Json<Deck>> {
    let Path(id) = path.map_err(ApiError::into_invalid)?;
    let name = deck_name(&body.name)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, body.expected_version).await?;
    let deck = sqlx::query_as("UPDATE decks d SET name=$2,version=version+1 WHERE id=$1 RETURNING d.id,d.name,d.version,d.created_at,(SELECT count(*) FROM deck_cards WHERE deck_id=d.id) AS card_count,(SELECT count(*) FROM deck_cards dc JOIN learning_cards c ON c.id=dc.card_id WHERE dc.deck_id=d.id AND c.due_at<=now()) AS due_count")
        .bind(id).bind(name).fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(deck))
}

#[utoipa::path(delete,path="/api/decks/{id}",params(("id"=Uuid,Path),DeleteDeckQuery),responses((status=204,description="Deck and its memberships deleted; shared cards and history retained"),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn delete_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
    query: Result<Query<DeleteDeckQuery>, ApiError>,
) -> ApiResult<StatusCode> {
    let Path(id) = path.map_err(ApiError::into_invalid)?;
    let Query(query) = query?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, query.expected_version).await?;
    // deck_cards cascades from decks; learning_cards, reviews and saved positions do not.
    sqlx::query("DELETE FROM decks WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get,path="/api/decks/{id}/cards",params(("id"=Uuid,Path),CardQuery),responses((status=200,body=CardPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CardQuery>,
) -> ApiResult<Json<CardPage>> {
    search::load(&state, Some(id), query).await
}

#[utoipa::path(post,path="/api/decks/{id}/cards",params(("id"=Uuid,Path)),request_body=AddCards,responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddCards>,
) -> ApiResult<Json<AddCardsResult>> {
    require_deck(&state.pool, id).await?;
    if body.position_id.is_some() && body.job_id.is_some()
        || body.position_id.is_none() && body.job_id.is_none() && body.match_id.is_none()
    {
        return Err(ApiError::invalid(
            "Supply position_id, or match_id / saved match-analysis job_id",
        ));
    }
    if body.min_loss.is_some_and(|n| !n.is_finite() || n < 0.0)
        || body.player.is_some_and(|p| !(0..=1).contains(&p))
    {
        return Err(ApiError::invalid(
            "min_loss must be finite and nonnegative; player must be 0 or 1",
        ));
    }
    if body.game_no.is_some_and(|n| n < 1) {
        return Err(ApiError::invalid("game_no must be positive"));
    }
    let contents = content::from_source(&state.pool, &body).await?;
    if contents.len() > MAX_CARDS as usize {
        return Err(ApiError::invalid(
            "Batch exceeds 1000 cards; narrow the player or minimum-loss filter",
        ));
    }
    // Keep the legacy last-payload-wins behavior for repeated source decisions.
    let mut unique = BTreeMap::new();
    for c in contents {
        unique.insert(card_signature(&c), c);
    }
    let tags = topics::tags(body.tags)?;
    Ok(Json(
        write_match_cards(&state.pool, id, unique, &tags).await?,
    ))
}

#[utoipa::path(post,path="/api/decks/{id}/positions",params(("id"=Uuid,Path)),request_body=AddPositions,responses((status=200,body=AddPositionsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_positions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddPositions>,
) -> ApiResult<Json<AddPositionsResult>> {
    require_deck(&state.pool, id).await?;
    let ids = body.validated_ids()?;
    let sources = saved_position_cards(&state.pool, &ids).await?;
    let mut unique = BTreeMap::new();
    let mut items = Vec::with_capacity(ids.len());
    let mut resolved = Vec::with_capacity(ids.len());
    for position_id in ids {
        let (status, message, source) = match sources.get(&position_id) {
            Some(Some(content)) => {
                let signature = card_signature(content);
                let first = !unique.contains_key(&signature);
                unique
                    .entry(signature.clone())
                    .or_insert_with(|| content.clone());
                (PositionAddStatus::Duplicate, None, Some((signature, first)))
            }
            Some(None) => (
                PositionAddStatus::AnalysisRequired,
                Some(
                    "Analyze this position or complete its game or match analysis before adding it"
                        .into(),
                ),
                None,
            ),
            None => (PositionAddStatus::NotFound, Some("Not found".into()), None),
        };
        items.push(PositionAddItem {
            position_id,
            status,
            card_id: None,
            message,
        });
        resolved.push(source);
    }
    let written = write_cards(&state.pool, id, unique, &BTreeSet::new()).await?;
    let mut added = 0;
    let mut updated = 0;
    for (item, source) in items.iter_mut().zip(resolved) {
        if let Some((signature, first)) = source {
            let card = &written.cards[&signature];
            item.card_id = Some(card.id);
            if first && card.added {
                item.status = PositionAddStatus::Added;
                added += 1;
            } else if first && card.changed {
                item.status = PositionAddStatus::Updated;
                updated += 1;
            }
        }
    }
    Ok(Json(AddPositionsResult {
        added,
        updated,
        total: written.total,
        items,
    }))
}

#[utoipa::path(post,path="/api/decks/{id}/decisions",params(("id"=Uuid,Path)),request_body=AddDecisions,responses((status=200,body=AddDecisionsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_decisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddDecisions>,
) -> ApiResult<Json<AddDecisionsResult>> {
    require_deck(&state.pool, id).await?;
    let references = body.validated()?;
    let sources = content::decision_cards(&state.pool, &references).await?;
    let mut unique = BTreeMap::new();
    let mut items = Vec::with_capacity(references.len());
    let mut resolved = Vec::with_capacity(references.len());
    for reference in references {
        let (status, message, source) = match sources.get(&reference) {
            Some(Ok(content)) => {
                let signature = card_signature(content);
                let first = !unique.contains_key(&signature);
                unique
                    .entry(signature.clone())
                    .or_insert_with(|| content.clone());
                (DecisionAddStatus::Duplicate, None, Some((signature, first)))
            }
            Some(Err(message)) => (DecisionAddStatus::Unavailable, Some(message.clone()), None),
            None => (DecisionAddStatus::NotFound, Some("Not found".into()), None),
        };
        items.push(DecisionAddItem {
            reference,
            status,
            card_id: None,
            message,
        });
        resolved.push(source);
    }
    let written = write_cards(&state.pool, id, unique, &BTreeSet::new()).await?;
    let mut added = 0;
    let mut updated = 0;
    for (item, source) in items.iter_mut().zip(resolved) {
        if let Some((signature, first)) = source {
            let card = &written.cards[&signature];
            item.card_id = Some(card.id);
            if first && card.added {
                item.status = DecisionAddStatus::Added;
                added += 1;
            } else if first && card.changed {
                item.status = DecisionAddStatus::Updated;
                updated += 1;
            }
        }
    }
    Ok(Json(AddDecisionsResult {
        added,
        updated,
        total: written.total,
        items,
    }))
}

#[utoipa::path(get, path = "/api/study/learning-summary", responses((status = 200, body = LearningSummary)), tag = "Learning")]
pub(super) async fn learning_summary(
    State(state): State<AppState>,
) -> ApiResult<Json<LearningSummary>> {
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

    let allowance = daily::allowance(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
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
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;

    let new_available_today = row.new_count.min(allowance.remaining());
    Ok(Json(LearningSummary {
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
    }))
}

#[utoipa::path(get,path="/api/decks/{id}/next",params(("id"=Uuid,Path)),responses((status=200,body=NextCard),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn next_card(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NextCard>> {
    require_deck(&state.pool, id).await?;
    let allowance = daily::allowance(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let (total,due,new):(i64,i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE due_at<=now() AND review_count+anki_review_count>0),count(*) FILTER(WHERE due_at<=now() AND review_count+anki_review_count=0) FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id WHERE dc.deck_id=$1").bind(id).fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
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
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let (card, active_attempt) = if let Some(row) = row {
        let attempt = if let Some(attempt) = row.active_attempt {
            let mut connection = state.pool.acquire().await.map_err(anyhow::Error::from)?;
            Some(attempted_card(&mut connection, &row.card, attempt).await?)
        } else {
            None
        };
        (Some(row.card.question()), attempt)
    } else {
        (None, None)
    };
    Ok(Json(NextCard {
        card,
        counts: ReviewCounts {
            total,
            due,
            new,
            new_available: new.min(allowance.remaining()),
        },
        active_attempt,
    }))
}

#[utoipa::path(post,path="/api/cards/{id}/reveal",params(("id"=Uuid,Path)),request_body=RevealCard,responses((status=200,body=RevealedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RevealCard>,
) -> ApiResult<Json<RevealedCard>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if row.version != body.expected_version {
        return Err(conflict("Card changed; reload it before revealing"));
    }
    let now = Utc::now();
    if let Some(attempt) = active_attempt(&mut *tx, &row).await? {
        return Ok(Json(
            revealed_card(&mut tx, &row, attempt.session_id, now).await?,
        ));
    }
    let session_id = Uuid::new_v4();
    let revealed = revealed_card(&mut tx, &row, session_id, now).await?;
    sqlx::query(
        "INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES($1,$2,$3,$4)",
    )
    .bind(session_id)
    .bind(id)
    .bind(row.version)
    .bind(now + chrono::Duration::days(1))
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(revealed))
}

#[utoipa::path(post,path="/api/cards/{id}/attempt",params(("id"=Uuid,Path)),request_body=CardAttemptRequest,responses((status=200,body=AttemptedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn submit_attempt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CardAttemptRequest>,
) -> ApiResult<Json<AttemptedCard>> {
    let row = load_attempt_card(&state.pool, id, body.expected_version, false).await?;
    if let Some(previous) = active_attempt(&state.pool, &row).await? {
        // Recheck under the same lock used to finish a review. The card may
        // have advanced since the preliminary lookup.
        previous.confirm_request(&body)?;
        let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
        let row = load_attempt_card(&mut *tx, id, body.expected_version, true).await?;
        let result = attempted_card(&mut tx, &row, previous.into_data()).await?;
        return Ok(Json(result));
    }
    let Some(card) = row.decision() else {
        return Err(ApiError::invalid("Reveal the Anki card before rating"));
    };
    crate::play::require_review_available(&state.pool, card.source.match_id).await?;
    let evaluation = evaluate_card(&state, card, &body.answer).await?;

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
    Ok(Json(result))
}

#[utoipa::path(post,path="/api/cards/{id}/review",params(("id"=Uuid,Path)),request_body=ReviewCard,responses((status=200,body=ReviewResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewCard>,
) -> ApiResult<Json<ReviewResult>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let previous: Option<(Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT session_id,rating,result FROM card_reviews WHERE card_id=$1 AND request_id=$2",
    )
    .bind(id)
    .bind(body.request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if let Some((session, rating, result)) = previous {
        if session != body.session_id || rating != body.rating.as_str() {
            return Err(conflict(
                "request_id was already used for a different review",
            ));
        }
        return Ok(Json(
            serde_json::from_value(result).map_err(anyhow::Error::from)?,
        ));
    }
    let session: Option<(i64, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT card_version,expires_at,attempt_request_id IS NOT NULL FROM review_sessions WHERE id=$1 AND card_id=$2",
    )
    .bind(body.session_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let (version, expires, is_attempt) =
        session.ok_or_else(|| conflict("Reveal this card before rating"))?;
    let now = Utc::now();
    if version != row.version || (!is_attempt && expires < now) {
        return Err(conflict(
            "Review session is stale; reload and reveal the card",
        ));
    }
    if !is_attempt && active_attempt(&mut *tx, &row).await?.is_some() {
        return Err(conflict("Rate the saved first attempt for this card"));
    }
    let mut result = scheduler::rate(&row, body.rating, now)?;
    sqlx::query("UPDATE learning_cards SET stability=$2,difficulty=$3,last_review=$4,due_at=$5,version=$6,review_count=$7,updated_at=$4 WHERE id=$1").bind(id).bind(result.stability).bind(result.difficulty).bind(now).bind(result.due_at).bind(result.version).bind(result.review_count).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    result.review_count += row.anki_review_count;
    sqlx::query("INSERT INTO card_reviews(card_id,request_id,session_id,rating,result,reviewed_at) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(body.request_id).bind(body.session_id).bind(body.rating.as_str()).bind(serde_json::to_value(&result).map_err(anyhow::Error::from)?).bind(now).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/decks/{id}/export.apkg",params(("id"=Uuid,Path),ExportQuery),responses((status=200,description="Native Anki package without review history",content_type="application/zip",body=Vec<u8>),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody)),tag="Learning")]
pub(super) async fn export_deck(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> ApiResult<impl IntoResponse> {
    let bytes = anki::build_deck_package(&state, id, query).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"neondojo.apkg\"",
            ),
        ],
        bytes,
    ))
}
