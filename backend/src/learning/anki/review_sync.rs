//! Exchange review records for cards exported using NeonDojo's decision model.
use super::{
    super::*,
    apkg,
    connect::{self, Client, upstream_error},
};
use anyhow::ensure;
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

mod cards;
pub(super) use cards::deck_cards;

const PAGE_SIZE: usize = 100;
const MAX_REVIEWS: usize = 100_000;

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncResult {
    pub profile: String,
    pub matched_cards: usize,
    pub unmatched_cards: usize,
    pub received_reviews: usize,
    pub sent_reviews: usize,
}

#[derive(Deserialize)]
pub(super) struct Collection {
    pub(super) profile: String,
    session: String,
}

impl Collection {
    pub(super) async fn open(client: &Client) -> ApiResult<Self> {
        client.rpc("collection.get", json!({})).await
    }

    pub(super) async fn collection_key(&self, client: &Client) -> ApiResult<String> {
        use sha2::{Digest, Sha256};
        #[derive(Deserialize)]
        struct MediaDirectory {
            path: String,
        }
        let directory: MediaDirectory = client
            .rpc("media.getDirectory", json!({"session":self.session}))
            .await?;
        let mut hash = Sha256::new();
        hash.update(self.profile.as_bytes());
        hash.update([0]);
        hash.update(directory.path.as_bytes());
        Ok(format!("{:x}", hash.finalize()))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    #[serde(flatten)]
    values: T,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Ids {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

#[derive(Deserialize)]
struct NoteItem {
    result: Note,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Note {
    fields: HashMap<String, String>,
    card_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Interval {
    value: u32,
    unit: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Unit {
    Days,
    Seconds,
}

impl Interval {
    fn from_seconds(seconds: i64) -> anyhow::Result<Self> {
        ensure!(seconds >= 0, "Invalid NeonDojo review interval");
        let (unit, value) = if seconds % 86400 == 0 {
            (Unit::Days, seconds / 86400)
        } else {
            (Unit::Seconds, seconds)
        };
        Ok(Self {
            value: value.try_into()?,
            unit,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Learning,
    Review,
    Relearning,
    Filtered,
    Manual,
    Rescheduled,
}

#[derive(Deserialize)]
struct Factor {
    value: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteReview {
    id: String,
    card_id: String,
    rating: Option<Rating>,
    interval: Interval,
    previous_interval: Interval,
    factor: Factor,
    elapsed_ms: u32,
    kind: Kind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutgoingReview {
    id: String,
    card_id: String,
    rating: Rating,
    interval: Interval,
    previous_interval: Interval,
    factor: Value,
    elapsed_ms: u32,
    kind: Kind,
}

impl OutgoingReview {
    fn matches(&self, remote: &RemoteReview) -> bool {
        self.card_id == remote.card_id
            && Some(self.rating) == remote.rating
            && self.interval == remote.interval
            && self.previous_interval == remote.previous_interval
            && self.factor["value"] == remote.factor.value
            && self.elapsed_ms == remote.elapsed_ms
            && self.kind == remote.kind
    }
}

#[derive(sqlx::FromRow)]
struct LocalReview {
    card_id: Uuid,
    request_id: Uuid,
    rating: String,
    reviewed_at: DateTime<Utc>,
    interval: i64,
    previous_interval: i64,
    difficulty: f64,
    export_id: Option<i64>,
}

pub(crate) async fn sync_deck_service(
    state: &AppState,
    id: Uuid,
) -> Result<SyncResult, crate::domain_error::DomainError> {
    let _permit = connect::reserve(state)?;
    let cards = deck_cards(&state.pool, id).await?;
    let client = Client::local();
    let collection = Collection::open(&client).await?;
    Ok(sync_deck(&state.pool, id, &client, &collection, &cards, None).await?)
}

#[utoipa::path(post, path="/api/decks/{id}/sync-anki-reviews", params(("id"=Uuid,Path)), request_body=SyncRequest,
    responses((status=200,body=SyncResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),
    (status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=502,body=ErrorBody),
    (status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(in crate::learning) async fn sync(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(_body): Json<SyncRequest>,
) -> ApiResult<Json<SyncResult>> {
    Ok(Json(sync_deck_service(&state, id).await?))
}

pub(super) async fn sync_deck(
    pool: &PgPool,
    id: Uuid,
    client: &Client,
    collection: &Collection,
    cards: &HashMap<String, Uuid>,
    auto_link: Option<Uuid>,
) -> ApiResult<SyncResult> {
    let matched = collection.matches(client, cards).await?;
    let remote = read_reviews(client, &collection.session, &matched).await?;
    let (received, outgoing) = reconcile(pool, id, &matched, &remote, auto_link).await?;
    let sent = outgoing.len();
    for batch in outgoing.chunks(PAGE_SIZE) {
        if let Some(link) = auto_link {
            let enabled: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM anki_auto_sync WHERE deck_id=$1 AND link_id=$2)",
            )
            .bind(id)
            .bind(link)
            .fetch_one(pool)
            .await
            .map_err(anyhow::Error::from)?;
            if !enabled {
                return Err(conflict(
                    "The automatic Anki link changed during synchronization",
                ));
            }
        }
        write_reviews(client, &collection.session, batch).await?;
        if let Some(link) = auto_link {
            sqlx::query(
                "UPDATE anki_auto_sync SET sent_reviews=sent_reviews+$3
                 WHERE deck_id=$1 AND link_id=$2",
            )
            .bind(id)
            .bind(link)
            .bind(batch.len() as i64)
            .execute(pool)
            .await
            .map_err(anyhow::Error::from)?;
        }
    }
    Ok(SyncResult {
        profile: collection.profile.clone(),
        matched_cards: matched.len(),
        unmatched_cards: cards.len() - matched.len(),
        received_reviews: received,
        sent_reviews: sent,
    })
}

impl Collection {
    pub(super) async fn matches(
        &self,
        client: &Client,
        cards: &HashMap<String, Uuid>,
    ) -> ApiResult<HashMap<Uuid, String>> {
        match_cards(client, &self.session, cards).await
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::learning) struct SyncRequest {}

async fn lock_deck(connection: &mut sqlx::PgConnection, id: Uuid) -> ApiResult<()> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM decks WHERE id=$1 FOR SHARE")
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    Ok(())
}

async fn match_cards(
    client: &Client,
    session: &str,
    cards: &HashMap<String, Uuid>,
) -> ApiResult<HashMap<Uuid, String>> {
    let mut matched = HashMap::new();
    let identities = cards.keys().collect::<Vec<_>>();
    for batch in identities.chunks(50) {
        let filters = batch
            .iter()
            .map(|key| format!("\"Identity:{key}\""))
            .collect::<Vec<_>>();
        let query = format!("\"note:{}\" ({})", apkg::MODEL_NAME, filters.join(" OR "));
        let page: Page<Ids> = client
            .rpc(
                "notes.search",
                json!({"query":query,"limit":PAGE_SIZE,"session":session}),
            )
            .await?;
        if page.next_cursor.is_some() {
            return Err(conflict(
                "Resolve duplicate NeonDojo notes in Anki before synchronizing",
            ));
        }
        if page.values.ids.is_empty() {
            continue;
        }
        let notes: Items<NoteItem> = client
            .rpc(
                "notes.getMany",
                json!({"ids":page.values.ids,"session":session}),
            )
            .await?;
        for note in notes.items {
            let note = note.result;
            let Some(card) = note.fields.get("Identity").and_then(|key| cards.get(key)) else {
                continue;
            };
            let [remote] = note.card_ids.as_slice() else {
                return Err(conflict(
                    "An exported NeonDojo note must have exactly one Anki card",
                ));
            };
            if matched.insert(*card, remote.clone()).is_some() {
                return Err(conflict(
                    "Resolve duplicate NeonDojo notes in Anki before synchronizing",
                ));
            }
        }
    }
    Ok(matched)
}

async fn read_reviews(
    client: &Client,
    session: &str,
    matched: &HashMap<Uuid, String>,
) -> ApiResult<HashMap<i64, RemoteReview>> {
    if matched.is_empty() {
        return Ok(HashMap::new());
    }
    let mut reviews = HashMap::new();
    let mut cursor = None::<String>;
    let ids = matched.values().collect::<Vec<_>>();
    loop {
        let mut params = json!({"cardIds":ids,"limit":1000,"session":session});
        if let Some(cursor) = &cursor {
            params["cursor"] = json!(cursor);
        }
        let page: Page<Items<RemoteReview>> = client.rpc("reviews.list", params).await?;
        for row in page.values.items {
            let id = row
                .id
                .parse()
                .ok()
                .filter(|&id| id > 0)
                .ok_or_else(|| upstream_error("AnkiConnect returned an invalid review ID"))?;
            if reviews.insert(id, row).is_some() {
                return Err(upstream_error("AnkiConnect returned duplicate review IDs"));
            }
        }
        if reviews.len() > MAX_REVIEWS {
            return Err(ApiError::invalid(
                "The selected deck has more than 100000 Anki review records",
            ));
        }
        if page.next_cursor.is_none() {
            return Ok(reviews);
        }
        if page.next_cursor == cursor || reviews.is_empty() {
            return Err(upstream_error(
                "AnkiConnect did not advance the review history",
            ));
        }
        cursor = page.next_cursor;
    }
}

async fn reconcile(
    pool: &PgPool,
    deck_id: Uuid,
    matched: &HashMap<Uuid, String>,
    remote: &HashMap<i64, RemoteReview>,
    auto_link: Option<Uuid>,
) -> ApiResult<(usize, Vec<OutgoingReview>)> {
    let ids = matched.keys().copied().collect::<Vec<_>>();
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck(&mut tx, deck_id).await?;
    if let Some(link) = auto_link {
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT link_id FROM anki_auto_sync WHERE deck_id=$1 AND link_id=$2 FOR UPDATE",
        )
        .bind(deck_id)
        .bind(link)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        if exists.is_none() {
            return Err(conflict(
                "The automatic Anki link changed during synchronization",
            ));
        }
    }
    let locked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
         WHERE dc.deck_id=$1 AND c.id=ANY($2) ORDER BY c.signature COLLATE \"C\" FOR UPDATE OF c",
    )
    .bind(deck_id)
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if locked.len() != ids.len() {
        return Err(conflict("The NeonDojo deck changed during synchronization"));
    }

    let local: Vec<LocalReview> = sqlx::query_as(
        "WITH reviews AS (
           SELECT r.*, (r.result->>'interval_seconds')::bigint AS interval,
                  (r.result->>'difficulty')::double precision AS difficulty
           FROM card_reviews r WHERE r.card_id=ANY($1)
         )
         SELECT r.card_id,r.request_id,r.rating,r.reviewed_at,r.interval,r.difficulty,
                lag(r.interval,1,0) OVER (PARTITION BY r.card_id ORDER BY r.reviewed_at,r.request_id)
                    AS previous_interval, e.review_id AS export_id
         FROM reviews r LEFT JOIN anki_review_exports e USING(card_id,request_id)
         ORDER BY r.reviewed_at,r.request_id LIMIT 100001",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if local.len() > MAX_REVIEWS {
        return Err(ApiError::invalid(
            "The selected deck has more than 100000 NeonDojo reviews",
        ));
    }

    // Exports use collection-wide Anki IDs, including when different decks
    // are synchronized through two NeonDojo processes.
    crate::advisory_locks::xact_lock(&mut tx, crate::advisory_locks::ANKI_REVIEW_EXPORT_IDS)
        .await
        .map_err(anyhow::Error::from)?;
    let mut occupied = remote.keys().copied().collect::<BTreeSet<_>>();
    if let (Some(first), Some(last)) = (local.first(), local.last()) {
        let existing: Vec<i64> = sqlx::query_scalar(
            "SELECT review_id FROM anki_review_exports WHERE review_id BETWEEN $1 AND $2",
        )
        .bind(first.reviewed_at.timestamp_millis())
        .bind(
            last.reviewed_at
                .timestamp_millis()
                .saturating_add((MAX_REVIEWS * 3) as i64),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        occupied.extend(existing);
    }
    let mut echoed = BTreeSet::new();
    let mut outgoing = Vec::new();
    let mut reserved = Vec::new();
    let mut last_candidate = 0;
    for row in &local {
        let id = match row.export_id {
            Some(id) => id,
            None => {
                let mut id = row.reviewed_at.timestamp_millis().max(last_candidate + 1);
                while !occupied.insert(id) {
                    id += 1;
                }
                last_candidate = id;
                reserved.push((row.card_id, row.request_id, id));
                id
            }
        };
        let entry = outgoing_review(id, &matched[&row.card_id], row)?;
        match remote.get(&id) {
            Some(other) if entry.matches(other) => {
                echoed.insert(id);
            }
            Some(_) => {
                return Err(conflict(
                    "An Anki review ID is already used for a different review",
                ));
            }
            None => outgoing.push(entry),
        }
    }

    if !reserved.is_empty() {
        let (cards, requests): (Vec<_>, Vec<_>) = reserved
            .iter()
            .map(|(card, request, _)| (*card, *request))
            .unzip();
        let keys = reserved.iter().map(|(_, _, id)| *id).collect::<Vec<_>>();
        sqlx::query(
            "INSERT INTO anki_review_exports(card_id,request_id,review_id)
             SELECT * FROM unnest($1::uuid[],$2::uuid[],$3::bigint[])",
        )
        .bind(cards)
        .bind(requests)
        .bind(keys)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    }

    let by_remote = matched
        .iter()
        .map(|(id, anki)| (anki.as_str(), *id))
        .collect::<HashMap<_, _>>();
    let mut incoming = Vec::new();
    for (&id, row) in remote {
        let card = by_remote.get(row.card_id.as_str()).ok_or_else(|| {
            upstream_error("AnkiConnect returned a review for an unrequested card")
        })?;
        if let Some(rating) = incoming_rating(row).filter(|_| !echoed.contains(&id)) {
            incoming.push((*card, id, rating));
        }
    }
    let received = save_incoming(&mut tx, &incoming).await?;
    if received > 0 && auto_link.is_some() {
        sqlx::query(
            "UPDATE anki_auto_sync SET last_received_at=now(),
                received_reviews=received_reviews+$2 WHERE deck_id=$1",
        )
        .bind(deck_id)
        .bind(received as i64)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((received, outgoing))
}

fn outgoing_review(id: i64, card: &str, row: &LocalReview) -> anyhow::Result<OutgoingReview> {
    let rating = match row.rating.as_str() {
        "again" => Rating::Again,
        "hard" => Rating::Hard,
        "good" => Rating::Good,
        "easy" => Rating::Easy,
        _ => anyhow::bail!("Invalid NeonDojo review rating"),
    };
    let kind = if row.previous_interval == 0 {
        Kind::Learning
    } else if row.interval >= 86400 {
        Kind::Review
    } else if row.previous_interval >= 86400 {
        Kind::Relearning
    } else {
        Kind::Learning
    };
    let difficulty = (((row.difficulty - 1.0) / 9.0 + 0.1) * 1000.0).round() as u32;
    Ok(OutgoingReview {
        id: id.to_string(),
        card_id: card.to_owned(),
        rating,
        interval: Interval::from_seconds(row.interval)?,
        previous_interval: Interval::from_seconds(row.previous_interval)?,
        factor: json!({"encoding":"fsrsNormalizedDifficulty","value":difficulty}),
        elapsed_ms: 0,
        kind,
    })
}

fn incoming_rating(review: &RemoteReview) -> Option<Rating> {
    match review.kind {
        Kind::Manual | Kind::Rescheduled => None,
        Kind::Filtered if review.factor.value == 0 => None,
        _ => review.rating,
    }
}

async fn save_incoming(
    connection: &mut sqlx::PgConnection,
    incoming: &[(Uuid, i64, Rating)],
) -> ApiResult<usize> {
    let cards = incoming
        .iter()
        .map(|(card, _, _)| *card)
        .collect::<Vec<_>>();
    let keys = incoming.iter().map(|(_, key, _)| *key).collect::<Vec<_>>();
    let ratings = incoming
        .iter()
        .map(|(_, _, rating)| rating.as_str())
        .collect::<Vec<_>>();
    let count: i64 = sqlx::query_scalar(
        "WITH added AS (
           INSERT INTO anki_received_reviews(card_id,review_id,rating)
           SELECT * FROM unnest($1::uuid[],$2::bigint[],$3::text[])
           ON CONFLICT DO NOTHING RETURNING card_id
         ), counts AS (
           SELECT card_id,count(*)::integer AS total FROM added GROUP BY card_id
         ), updated AS (
           UPDATE learning_cards c SET anki_review_count=c.anki_review_count+counts.total
           FROM counts WHERE c.id=counts.card_id RETURNING counts.total
         )
         SELECT coalesce(sum(total),0)::bigint FROM updated",
    )
    .bind(cards)
    .bind(keys)
    .bind(ratings)
    .fetch_one(&mut *connection)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(count as usize)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationHandle {
    operation_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
enum Operation {
    Queued,
    Running,
    WaitingForUser,
    Succeeded { result: Changed },
    Failed { error: OperationError },
    Cancelled { error: OperationError },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Changed {
    changed_count: usize,
}

#[derive(Deserialize)]
struct OperationError {
    message: String,
}

async fn write_reviews(client: &Client, session: &str, batch: &[OutgoingReview]) -> ApiResult<()> {
    let handle: OperationHandle = client
        .rpc(
            "advanced.reviews.import",
            json!({"session":session,"reviews":batch}),
        )
        .await?;
    for _ in 0..80 {
        let operation = client
            .rpc("operations.get", json!({"operationId":handle.operation_id}))
            .await?;
        match operation {
            Operation::Succeeded { result } if result.changed_count == batch.len() => return Ok(()),
            Operation::Succeeded { .. } => {
                return Err(upstream_error(
                    "AnkiConnect imported an unexpected number of reviews",
                ));
            }
            Operation::Failed { error } | Operation::Cancelled { error } => {
                return Err(client.error(&error.message));
            }
            Operation::WaitingForUser => {
                return Err(conflict(
                    "Complete the pending operation in Anki Desktop and synchronize again",
                ));
            }
            Operation::Queued | Operation::Running => {
                tokio::time::sleep(Duration::from_millis(250)).await
            }
        }
    }
    Err(ApiError(
        StatusCode::GATEWAY_TIMEOUT,
        "AnkiConnect did not confirm the review import in time".into(),
    ))
}

#[cfg(test)]
mod tests;
