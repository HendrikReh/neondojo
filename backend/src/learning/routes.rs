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

pub(crate) async fn list_decks_service(
    state: &AppState,
) -> Result<Vec<Deck>, crate::domain_error::DomainError> {
    Ok(list_decks(&state.pool).await?)
}

#[utoipa::path(get,path="/api/decks",responses((status=200,body=Vec<Deck>)),tag="Learning")]
pub(super) async fn decks(State(state): State<AppState>) -> ApiResult<Json<Vec<Deck>>> {
    Ok(Json(list_decks_service(&state).await?))
}

pub(crate) async fn create_deck_service(
    state: &AppState,
    body: CreateDeck,
) -> Result<Deck, crate::domain_error::DomainError> {
    let name = deck_name(&body.name)?;
    Ok(insert_deck(&state.pool, name).await?)
}

#[utoipa::path(post,path="/api/decks",request_body=CreateDeck,responses((status=201,body=Deck),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn create_deck(
    State(state): State<AppState>,
    Json(body): Json<CreateDeck>,
) -> ApiResult<(StatusCode, Json<Deck>)> {
    Ok((
        StatusCode::CREATED,
        Json(create_deck_service(&state, body).await?),
    ))
}

pub(crate) async fn rename_deck_service(
    state: &AppState,
    id: Uuid,
    body: RenameDeck,
) -> Result<Deck, crate::domain_error::DomainError> {
    let name = deck_name(&body.name)?;
    Ok(update_deck(&state.pool, id, body.expected_version, name).await?)
}

#[utoipa::path(put,path="/api/decks/{id}",params(("id"=Uuid,Path)),request_body=RenameDeck,responses((status=200,body=Deck),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn rename_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
    Json(body): Json<RenameDeck>,
) -> ApiResult<Json<Deck>> {
    let Path(id) = path.map_err(ApiError::into_invalid)?;
    Ok(Json(rename_deck_service(&state, id, body).await?))
}

pub(crate) async fn delete_deck_service(
    state: &AppState,
    id: Uuid,
    expected_version: i64,
) -> Result<(), crate::domain_error::DomainError> {
    remove_deck(&state.pool, id, expected_version).await?;
    Ok(())
}

#[utoipa::path(delete,path="/api/decks/{id}",params(("id"=Uuid,Path),DeleteDeckQuery),responses((status=204,description="Deck and its memberships deleted; shared cards and history retained"),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn delete_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
    query: Result<Query<DeleteDeckQuery>, ApiError>,
) -> ApiResult<StatusCode> {
    let Path(id) = path.map_err(ApiError::into_invalid)?;
    let Query(query) = query?;
    delete_deck_service(&state, id, query.expected_version).await?;
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

pub(crate) async fn add_cards_to_deck_service(
    state: &AppState,
    id: Uuid,
    body: AddCards,
) -> Result<AddCardsResult, crate::domain_error::DomainError> {
    require_deck(&state.pool, id).await?;
    if body.position_id.is_some() && body.job_id.is_some()
        || body.position_id.is_none() && body.job_id.is_none() && body.match_id.is_none()
    {
        return Err(crate::domain_error::DomainError::invalid(
            "Supply position_id, or match_id / saved match-analysis job_id",
        ));
    }
    if body.min_loss.is_some_and(|n| !n.is_finite() || n < 0.0)
        || body.player.is_some_and(|p| !(0..=1).contains(&p))
    {
        return Err(crate::domain_error::DomainError::invalid(
            "min_loss must be finite and nonnegative; player must be 0 or 1",
        ));
    }
    if body.game_no.is_some_and(|n| n < 1) {
        return Err(crate::domain_error::DomainError::invalid(
            "game_no must be positive",
        ));
    }
    let contents = content::from_source(&state.pool, &body).await?;
    if contents.len() > MAX_CARDS as usize {
        return Err(crate::domain_error::DomainError::invalid(
            "Batch exceeds 1000 cards; narrow the player or minimum-loss filter",
        ));
    }
    // Keep the legacy last-payload-wins behavior for repeated source decisions.
    let mut unique = BTreeMap::new();
    for c in contents {
        unique.insert(card_signature(&c), c);
    }
    let tags = topics::tags(body.tags)?;
    Ok(write_match_cards(&state.pool, id, unique, &tags).await?)
}

#[utoipa::path(post,path="/api/decks/{id}/cards",params(("id"=Uuid,Path)),request_body=AddCards,responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddCards>,
) -> ApiResult<Json<AddCardsResult>> {
    Ok(Json(add_cards_to_deck_service(&state, id, body).await?))
}

enum BatchAddSource<'a> {
    Ready(&'a CardContent),
    Rejected { missing: bool, message: String },
}

struct BatchAddOutcome<I> {
    added: i64,
    updated: i64,
    total: i64,
    items: Vec<I>,
}

async fn batch_add_cards<'a, K, S, I>(
    pool: &PgPool,
    deck_id: Uuid,
    keys: Vec<K>,
    mut resolve: impl FnMut(&K) -> BatchAddSource<'a>,
    statuses: (S, S, S, S, S),
    mut build_item: impl FnMut(K, S, Option<Uuid>, Option<String>) -> I,
) -> ApiResult<BatchAddOutcome<I>>
where
    S: Copy,
{
    let (added_status, updated_status, duplicate_status, unavailable_status, not_found_status) =
        statuses;
    let mut unique = BTreeMap::new();
    let mut pending = Vec::with_capacity(keys.len());
    for key in keys {
        match resolve(&key) {
            BatchAddSource::Ready(content) => {
                let signature = card_signature(content);
                let first = !unique.contains_key(&signature);
                unique
                    .entry(signature.clone())
                    .or_insert_with(|| content.clone());
                pending.push((key, duplicate_status, None, Some((signature, first))));
            }
            BatchAddSource::Rejected { missing, message } => {
                let status = if missing {
                    not_found_status
                } else {
                    unavailable_status
                };
                pending.push((key, status, Some(message), None));
            }
        }
    }
    let written = write_cards(pool, deck_id, unique, &BTreeSet::new()).await?;
    let mut added = 0;
    let mut updated = 0;
    let mut items = Vec::with_capacity(pending.len());
    for (key, mut status, message, source) in pending {
        let mut card_id = None;
        if let Some((signature, first)) = source {
            let card = &written.cards[&signature];
            card_id = Some(card.id);
            if first && card.added {
                status = added_status;
                added += 1;
            } else if first && card.changed {
                status = updated_status;
                updated += 1;
            }
        }
        items.push(build_item(key, status, card_id, message));
    }
    Ok(BatchAddOutcome {
        added,
        updated,
        total: written.total,
        items,
    })
}

pub(crate) async fn add_positions_to_deck_service(
    state: &AppState,
    id: Uuid,
    body: AddPositions,
) -> Result<AddPositionsResult, crate::domain_error::DomainError> {
    require_deck(&state.pool, id).await?;
    let ids = body.validated_ids()?;
    let sources = saved_position_cards(&state.pool, &ids).await?;
    let outcome = batch_add_cards(
        &state.pool,
        id,
        ids,
        |position_id| match sources.get(position_id) {
            Some(Some(content)) => BatchAddSource::Ready(content),
            Some(None) => BatchAddSource::Rejected {
                missing: false,
                message:
                    "Analyze this position or complete its game or match analysis before adding it"
                        .into(),
            },
            None => BatchAddSource::Rejected {
                missing: true,
                message: "Not found".into(),
            },
        },
        (
            PositionAddStatus::Added,
            PositionAddStatus::Updated,
            PositionAddStatus::Duplicate,
            PositionAddStatus::AnalysisRequired,
            PositionAddStatus::NotFound,
        ),
        |position_id, status, card_id, message| PositionAddItem {
            position_id,
            status,
            card_id,
            message,
        },
    )
    .await?;
    Ok(AddPositionsResult {
        added: outcome.added,
        updated: outcome.updated,
        total: outcome.total,
        items: outcome.items,
    })
}

#[utoipa::path(post,path="/api/decks/{id}/positions",params(("id"=Uuid,Path)),request_body=AddPositions,responses((status=200,body=AddPositionsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_positions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddPositions>,
) -> ApiResult<Json<AddPositionsResult>> {
    Ok(Json(add_positions_to_deck_service(&state, id, body).await?))
}

pub(crate) async fn add_decisions_to_deck_service(
    state: &AppState,
    id: Uuid,
    body: AddDecisions,
) -> Result<AddDecisionsResult, crate::domain_error::DomainError> {
    require_deck(&state.pool, id).await?;
    let references = body.validated()?;
    let sources = content::decision_cards(&state.pool, &references).await?;
    let outcome = batch_add_cards(
        &state.pool,
        id,
        references,
        |reference| match sources.get(reference) {
            Some(Ok(content)) => BatchAddSource::Ready(content),
            Some(Err(message)) => BatchAddSource::Rejected {
                missing: false,
                message: message.clone(),
            },
            None => BatchAddSource::Rejected {
                missing: true,
                message: "Not found".into(),
            },
        },
        (
            DecisionAddStatus::Added,
            DecisionAddStatus::Updated,
            DecisionAddStatus::Duplicate,
            DecisionAddStatus::Unavailable,
            DecisionAddStatus::NotFound,
        ),
        |reference, status, card_id, message| DecisionAddItem {
            reference,
            status,
            card_id,
            message,
        },
    )
    .await?;
    Ok(AddDecisionsResult {
        added: outcome.added,
        updated: outcome.updated,
        total: outcome.total,
        items: outcome.items,
    })
}

#[utoipa::path(post,path="/api/decks/{id}/decisions",params(("id"=Uuid,Path)),request_body=AddDecisions,responses((status=200,body=AddDecisionsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_decisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddDecisions>,
) -> ApiResult<Json<AddDecisionsResult>> {
    Ok(Json(add_decisions_to_deck_service(&state, id, body).await?))
}

pub(crate) async fn learning_summary_service(
    state: &AppState,
) -> Result<LearningSummary, crate::domain_error::DomainError> {
    Ok(queue::summary(&state.pool).await?)
}

#[utoipa::path(get, path = "/api/study/learning-summary", responses((status = 200, body = LearningSummary)), tag = "Learning")]
pub(super) async fn learning_summary(
    State(state): State<AppState>,
) -> ApiResult<Json<LearningSummary>> {
    Ok(Json(learning_summary_service(&state).await?))
}

pub(crate) async fn next_card_service(
    state: &AppState,
    id: Uuid,
) -> Result<NextCard, crate::domain_error::DomainError> {
    Ok(queue::next(&state.pool, id).await?)
}

#[utoipa::path(get,path="/api/decks/{id}/next",params(("id"=Uuid,Path)),responses((status=200,body=NextCard),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn next_card(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NextCard>> {
    Ok(Json(next_card_service(&state, id).await?))
}

pub(crate) async fn reveal_card_service(
    state: &AppState,
    id: Uuid,
    expected_version: i64,
) -> Result<RevealedCard, crate::domain_error::DomainError> {
    Ok(reviews::reveal_card(&state.pool, id, expected_version).await?)
}

#[utoipa::path(post,path="/api/cards/{id}/reveal",params(("id"=Uuid,Path)),request_body=RevealCard,responses((status=200,body=RevealedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RevealCard>,
) -> ApiResult<Json<RevealedCard>> {
    Ok(Json(
        reveal_card_service(&state, id, body.expected_version).await?,
    ))
}

pub(crate) async fn submit_card_attempt_service(
    state: &AppState,
    id: Uuid,
    body: CardAttemptRequest,
) -> Result<AttemptedCard, crate::domain_error::DomainError> {
    Ok(submit_card_attempt(state, id, body).await?)
}

#[utoipa::path(post,path="/api/cards/{id}/attempt",params(("id"=Uuid,Path)),request_body=CardAttemptRequest,responses((status=200,body=AttemptedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn submit_attempt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CardAttemptRequest>,
) -> ApiResult<Json<AttemptedCard>> {
    Ok(Json(submit_card_attempt_service(&state, id, body).await?))
}

pub(crate) async fn review_card_service(
    state: &AppState,
    id: Uuid,
    body: ReviewCard,
) -> Result<ReviewResult, crate::domain_error::DomainError> {
    Ok(reviews::rate_card(&state.pool, id, body).await?)
}

#[utoipa::path(post,path="/api/cards/{id}/review",params(("id"=Uuid,Path)),request_body=ReviewCard,responses((status=200,body=ReviewResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewCard>,
) -> ApiResult<Json<ReviewResult>> {
    Ok(Json(review_card_service(&state, id, body).await?))
}

pub(crate) async fn export_deck_apkg_to_path_service(
    state: &AppState,
    id: Uuid,
    locale: Option<ConfigurationLocale>,
    destination: &std::path::Path,
) -> Result<std::path::PathBuf, crate::domain_error::DomainError> {
    let bytes = anki::build_deck_package(state, id, ExportQuery { locale }).await?;
    tokio::fs::write(destination, bytes)
        .await
        .map_err(|error| {
            crate::domain_error::DomainError::invalid(format!("Cannot write Anki package: {error}"))
        })?;
    Ok(destination.to_path_buf())
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
