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
    Ok(Json(list_decks(&state.pool).await?))
}

#[utoipa::path(post,path="/api/decks",request_body=CreateDeck,responses((status=201,body=Deck),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn create_deck(
    State(state): State<AppState>,
    Json(body): Json<CreateDeck>,
) -> ApiResult<(StatusCode, Json<Deck>)> {
    let name = deck_name(&body.name)?;
    let deck = insert_deck(&state.pool, name).await?;
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
    Ok(Json(
        update_deck(&state.pool, id, body.expected_version, name).await?,
    ))
}

#[utoipa::path(delete,path="/api/decks/{id}",params(("id"=Uuid,Path),DeleteDeckQuery),responses((status=204,description="Deck and its memberships deleted; shared cards and history retained"),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn delete_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
    query: Result<Query<DeleteDeckQuery>, ApiError>,
) -> ApiResult<StatusCode> {
    let Path(id) = path.map_err(ApiError::into_invalid)?;
    let Query(query) = query?;
    remove_deck(&state.pool, id, query.expected_version).await?;
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
    Ok(Json(queue::summary(&state.pool).await?))
}

#[utoipa::path(get,path="/api/decks/{id}/next",params(("id"=Uuid,Path)),responses((status=200,body=NextCard),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn next_card(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NextCard>> {
    Ok(Json(queue::next(&state.pool, id).await?))
}

#[utoipa::path(post,path="/api/cards/{id}/reveal",params(("id"=Uuid,Path)),request_body=RevealCard,responses((status=200,body=RevealedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RevealCard>,
) -> ApiResult<Json<RevealedCard>> {
    Ok(Json(
        reviews::reveal_card(&state.pool, id, body.expected_version).await?,
    ))
}

#[utoipa::path(post,path="/api/cards/{id}/attempt",params(("id"=Uuid,Path)),request_body=CardAttemptRequest,responses((status=200,body=AttemptedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn submit_attempt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CardAttemptRequest>,
) -> ApiResult<Json<AttemptedCard>> {
    Ok(Json(submit_card_attempt(&state, id, body).await?))
}

#[utoipa::path(post,path="/api/cards/{id}/review",params(("id"=Uuid,Path)),request_body=ReviewCard,responses((status=200,body=ReviewResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewCard>,
) -> ApiResult<Json<ReviewResult>> {
    Ok(Json(reviews::rate_card(&state.pool, id, body).await?))
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
