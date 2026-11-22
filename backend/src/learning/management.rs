//! Deck membership changes and deliberate resolution of duplicate cards.
use super::*;
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeSet, HashMap};

const MAX_SELECTION: usize = 200;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct MoveCards {
    source_version: i64,
    target_id: Uuid,
    target_version: i64,
    #[schema(min_items = 1, max_items = 200)]
    card_ids: Vec<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub(super) struct MoveCardsResult {
    moved: usize,
    already_in_target: usize,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RemoveCards {
    expected_version: i64,
    #[schema(min_items = 1, max_items = 200)]
    card_ids: Vec<Uuid>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AddDeckCards {
    expected_version: i64,
    #[schema(min_items = 1, max_items = 200)]
    card_ids: Vec<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub(super) struct AddDeckCardsResult {
    added: usize,
    already_in_target: usize,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct DeleteCards {
    #[schema(min_items = 1, max_items = 200)]
    cards: Vec<DeleteCard>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct DeleteCard {
    id: Uuid,
    version: i64,
    deck_ids: Vec<Uuid>,
}

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub(super) struct DuplicateQuery {
    after: Option<Uuid>,
    /// One-based result page. Mutually exclusive with `after`.
    page: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(super) struct DuplicateCard {
    card: CardQuestion,
    decks: Vec<DuplicateDeck>,
    #[schema(required = true)]
    duplicate_key: Option<String>,
    copies: i64,
}

#[derive(Serialize, FromRow, ToSchema)]
pub(super) struct DuplicateDeck {
    id: Uuid,
    name: String,
    version: i64,
}

#[derive(Serialize, FromRow, ToSchema)]
pub(super) struct DuplicatePeer {
    id: Uuid,
    version: i64,
    review_count: i32,
}

#[derive(Serialize, ToSchema)]
pub(super) struct DuplicatePage {
    items: Vec<DuplicateCard>,
    #[schema(required = true)]
    next_cursor: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<i64>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    peers: HashMap<String, Vec<DuplicatePeer>>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ResolveDuplicate {
    keep_id: Uuid,
    keep_version: i64,
    delete_id: Uuid,
    delete_version: i64,
}

fn validate_selection(ids: &[Uuid]) -> ApiResult<()> {
    if ids.is_empty()
        || ids.len() > MAX_SELECTION
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(ApiError::invalid("Select one to 200 distinct cards"));
    }
    Ok(())
}

async fn bump_decks(tx: &mut Transaction<'_, Postgres>, ids: &[Uuid]) -> ApiResult<()> {
    sqlx::query("UPDATE decks SET version=version+1 WHERE id=ANY($1)")
        .bind(ids)
        .execute(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

async fn remove_memberships(
    tx: &mut Transaction<'_, Postgres>,
    deck_id: Uuid,
    cards: &[Uuid],
) -> ApiResult<()> {
    let removed = sqlx::query("DELETE FROM deck_cards WHERE deck_id=$1 AND card_id=ANY($2)")
        .bind(deck_id)
        .bind(cards)
        .execute(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?
        .rows_affected();
    if removed != cards.len() as u64 {
        return Err(conflict("Card selection changed; reload the deck"));
    }
    Ok(())
}

async fn insert_memberships(
    tx: &mut Transaction<'_, Postgres>,
    deck_id: Uuid,
    ids: &[Uuid],
) -> ApiResult<usize> {
    let added = sqlx::query(
        "INSERT INTO deck_cards(deck_id,card_id)
         SELECT $1,id FROM learning_cards WHERE id=ANY($2)
         ORDER BY signature COLLATE \"C\"
         ON CONFLICT DO NOTHING",
    )
    .bind(deck_id)
    .bind(ids)
    .execute(&mut **tx)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected() as usize;
    check_deck_capacity(tx, deck_id).await?;
    Ok(added)
}

#[utoipa::path(post,path="/api/decks/{id}/add-cards",params(("id"=Uuid,Path)),request_body=AddDeckCards,responses((status=200,body=AddDeckCardsResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn add_deck_cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Json(body): crate::json_errors::Json<AddDeckCards>,
) -> ApiResult<Json<AddDeckCardsResult>> {
    validate_selection(&body.card_ids)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, body.expected_version).await?;
    let cards: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM learning_cards WHERE id=ANY($1)
         ORDER BY signature COLLATE \"C\" FOR KEY SHARE",
    )
    .bind(&body.card_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if cards.len() != body.card_ids.len() {
        return Err(conflict("Card selection changed; reload the cards"));
    }
    let added = insert_memberships(&mut tx, id, &body.card_ids).await?;
    if added > 0 {
        bump_decks(&mut tx, &[id]).await?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(AddDeckCardsResult {
        added,
        already_in_target: cards.len() - added,
    }))
}

#[utoipa::path(post,path="/api/decks/{id}/move-cards",params(("id"=Uuid,Path)),request_body=MoveCards,responses((status=200,body=MoveCardsResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn move_cards(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
    crate::json_errors::Json(body): crate::json_errors::Json<MoveCards>,
) -> ApiResult<Json<MoveCardsResult>> {
    validate_selection(&body.card_ids)?;
    if source_id == body.target_id {
        return Err(ApiError::invalid("Choose a different target deck"));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let mut decks = [
        (source_id, body.source_version),
        (body.target_id, body.target_version),
    ];
    decks.sort_unstable_by_key(|&(id, _)| id);
    for (id, version) in decks {
        lock_deck_version(&mut tx, id, version).await?;
    }

    remove_memberships(&mut tx, source_id, &body.card_ids).await?;
    let added = insert_memberships(&mut tx, body.target_id, &body.card_ids).await?;
    let mut changed = vec![source_id];
    if added > 0 {
        changed.push(body.target_id);
    }
    bump_decks(&mut tx, &changed).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(MoveCardsResult {
        moved: body.card_ids.len(),
        already_in_target: body.card_ids.len() - added,
    }))
}

#[utoipa::path(post,path="/api/decks/{id}/remove-cards",params(("id"=Uuid,Path)),request_body=RemoveCards,responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn remove_cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Json(body): crate::json_errors::Json<RemoveCards>,
) -> ApiResult<StatusCode> {
    validate_selection(&body.card_ids)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, body.expected_version).await?;
    remove_memberships(&mut tx, id, &body.card_ids).await?;
    bump_decks(&mut tx, &[id]).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lock_decks(tx: &mut Transaction<'_, Postgres>, ids: &[Uuid]) -> ApiResult<()> {
    sqlx::query("SELECT id FROM decks WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
}

#[utoipa::path(post,path="/api/cards/delete",request_body=DeleteCards,responses((status=204),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn delete_cards(
    State(state): State<AppState>,
    crate::json_errors::Json(body): crate::json_errors::Json<DeleteCards>,
) -> ApiResult<StatusCode> {
    let ids: Vec<_> = body.cards.iter().map(|card| card.id).collect();
    validate_selection(&ids)?;
    if body.cards.iter().any(|card| card.version < 0) {
        return Err(ApiError::invalid("Expected nonnegative card versions"));
    }
    let expected_memberships: BTreeSet<_> = body
        .cards
        .iter()
        .flat_map(|card| card.deck_ids.iter().map(move |&deck_id| (card.id, deck_id)))
        .collect();
    let decks: Vec<_> = expected_memberships
        .iter()
        .map(|&(_, deck_id)| deck_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_decks(&mut tx, &decks).await?;
    let versions: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT id,version FROM learning_cards WHERE id=ANY($1)
         ORDER BY signature COLLATE \"C\" FOR UPDATE",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .into_iter()
    .collect();
    let changed = || conflict("Cards or deck memberships changed; reload the cards");
    if body
        .cards
        .iter()
        .any(|card| versions.get(&card.id) != Some(&card.version))
    {
        return Err(changed());
    }
    let memberships: BTreeSet<(Uuid, Uuid)> =
        sqlx::query_as("SELECT card_id,deck_id FROM deck_cards WHERE card_id=ANY($1)")
            .bind(&ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .into_iter()
            .collect();
    if memberships != expected_memberships {
        return Err(changed());
    }
    sqlx::query("DELETE FROM learning_cards WHERE id=ANY($1)")
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    bump_decks(&mut tx, &decks).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get,path="/api/cards/duplicates",params(DuplicateQuery),responses((status=200,body=DuplicatePage),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn duplicates(
    State(state): State<AppState>,
    query: Result<Query<DuplicateQuery>, QueryRejection>,
) -> ApiResult<Json<DuplicatePage>> {
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    const LIMIT: usize = 50;
    let offset = crate::search::page_offset(query.page, query.after.is_some(), LIMIT as i64)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    #[derive(FromRow)]
    struct Entry {
        #[sqlx(flatten)]
        row: CardRow,
        identity: Option<String>,
        copies: i64,
    }
    const IDENTITIES: &str = "WITH identities AS (
             SELECT id, (payload#>>'{position,position_id}') || ':' ||
                        (payload#>>'{position,gnubg_match_id}') || ':' ||
                        (payload->>'kind') AS identity
             FROM learning_cards
         ), copies AS (
             SELECT identity, count(*) AS copies
             FROM identities WHERE identity IS NOT NULL
             GROUP BY identity HAVING count(*) > 1
         )";
    const MATCHING: &str = "FROM identities i LEFT JOIN copies USING (identity)
         WHERE copies.copies IS NOT NULL OR EXISTS (
             SELECT 1 FROM deck_cards WHERE card_id=i.id OFFSET 1
         )";
    let total = if offset.is_some() {
        Some(
            sqlx::query_scalar(&format!("{IDENTITIES} SELECT count(*) {MATCHING}"))
                .fetch_one(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?,
        )
    } else {
        None
    };
    let mut rows: Vec<Entry> = sqlx::query_as(&format!(
        "{IDENTITIES}, matching AS (
            SELECT i.id,copies.identity,coalesce(copies.copies,1) AS copies {MATCHING}
        )
        SELECT c.*,m.identity,m.copies FROM matching m JOIN learning_cards c ON c.id=m.id
        WHERE ($1::uuid IS NULL OR m.id>$1) ORDER BY m.id LIMIT $2 OFFSET $3"
    ))
    .bind(query.after)
    .bind((LIMIT + 1) as i64)
    .bind(offset.unwrap_or(0))
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let more = rows.len() > LIMIT;
    rows.truncate(LIMIT);
    let ids: Vec<_> = rows.iter().map(|entry| entry.row.id).collect();
    #[derive(FromRow)]
    struct Membership {
        card_id: Uuid,
        #[sqlx(flatten)]
        deck: DuplicateDeck,
    }
    let mut memberships: HashMap<Uuid, Vec<DuplicateDeck>> = HashMap::new();
    let decks: Vec<Membership> = sqlx::query_as(
        "SELECT dc.card_id,d.id,d.name,d.version FROM deck_cards dc
         JOIN decks d ON d.id=dc.deck_id WHERE dc.card_id=ANY($1)
         ORDER BY lower(d.name),d.id",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    for row in decks {
        memberships.entry(row.card_id).or_default().push(row.deck);
    }
    let keys: Vec<_> = rows.iter().filter_map(|row| row.identity.clone()).collect();
    let mut peers = HashMap::<String, Vec<DuplicatePeer>>::new();
    if offset.is_some() && !keys.is_empty() {
        #[derive(FromRow)]
        struct Peer {
            identity: String,
            #[sqlx(flatten)]
            card: DuplicatePeer,
        }
        let matching: Vec<Peer> = sqlx::query_as(&format!(
            "{IDENTITIES} SELECT i.identity,c.id,c.version,c.review_count+c.anki_review_count AS review_count
             FROM identities i JOIN learning_cards c ON c.id=i.id
             WHERE i.identity=ANY($1) ORDER BY c.id"
        ))
        .bind(keys)
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        for peer in matching {
            peers.entry(peer.identity).or_default().push(peer.card);
        }
    }
    let items = rows
        .into_iter()
        .map(|entry| {
            Ok(DuplicateCard {
                card: entry.row.question()?,
                decks: memberships.remove(&entry.row.id).unwrap_or_default(),
                duplicate_key: entry.identity,
                copies: entry.copies,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(DuplicatePage {
        items,
        next_cursor: ids.last().copied().filter(|_| more),
        total,
        peers,
    }))
}

async fn memberships(tx: &mut Transaction<'_, Postgres>, card_id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT deck_id FROM deck_cards WHERE card_id=$1 ORDER BY deck_id")
            .bind(card_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?,
    )
}

#[utoipa::path(post,path="/api/cards/duplicates/resolve",request_body=ResolveDuplicate,responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn resolve_duplicate(
    State(state): State<AppState>,
    crate::json_errors::Json(body): crate::json_errors::Json<ResolveDuplicate>,
) -> ApiResult<StatusCode> {
    if body.keep_id == body.delete_id || body.keep_version < 0 || body.delete_version < 0 {
        return Err(ApiError::invalid(
            "Select two different cards with current versions",
        ));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let affected = memberships(&mut tx, body.delete_id).await?;
    // Same lock order as moves and deck writes, followed by signature order as in export.
    lock_decks(&mut tx, &affected).await?;
    #[derive(FromRow)]
    struct Candidate {
        id: Uuid,
        signature: String,
        payload: serde_json::Value,
        version: i64,
    }
    let candidates: Vec<Candidate> = sqlx::query_as(
        "SELECT id,signature,payload,version FROM learning_cards WHERE id=ANY($1)
         ORDER BY signature COLLATE \"C\" FOR UPDATE",
    )
    .bind([body.keep_id, body.delete_id].as_slice())
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let changed = || conflict("Cards or deck memberships changed; reload duplicates");
    let keep = candidates
        .iter()
        .find(|row| row.id == body.keep_id)
        .ok_or_else(changed)?;
    let delete = candidates
        .iter()
        .find(|row| row.id == body.delete_id)
        .ok_or_else(changed)?;
    if keep.version != body.keep_version
        || delete.version != body.delete_version
        || memberships(&mut tx, body.delete_id).await? != affected
    {
        return Err(changed());
    }
    let identity = card_signature(
        &serde_json::from_value::<CardContent>(keep.payload.clone())
            .map_err(anyhow::Error::from)?,
    );
    let removed_identity = card_signature(
        &serde_json::from_value::<CardContent>(delete.payload.clone())
            .map_err(anyhow::Error::from)?,
    );
    if identity != removed_identity {
        return Err(ApiError::invalid(
            "These cards have different match contexts or question types",
        ));
    }
    sqlx::query(
        "INSERT INTO deck_cards(deck_id,card_id)
         SELECT deck_id,$1 FROM deck_cards WHERE card_id=$2 ON CONFLICT DO NOTHING",
    )
    .bind(keep.id)
    .bind(delete.id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    sqlx::query("DELETE FROM learning_cards WHERE id=$1")
        .bind(delete.id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if delete.signature == identity {
        sqlx::query("UPDATE learning_cards SET signature=$2 WHERE id=$1")
            .bind(keep.id)
            .bind(identity)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    bump_decks(&mut tx, &affected).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}
