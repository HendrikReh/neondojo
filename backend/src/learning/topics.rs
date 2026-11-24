use super::*;
use crate::annotations::normalize_tag;
use std::collections::BTreeSet;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TagCard {
    id: Uuid,
    tags_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct EditCardTags {
    cards: Vec<TagCard>,
    add: Vec<String>,
    remove: Vec<String>,
}

#[derive(Serialize, ToSchema, FromRow)]
pub(super) struct CardTags {
    pub(super) id: Uuid,
    tags: Vec<String>,
    tags_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveCardTopic {
    name: String,
    filter: CardQuery,
}

#[derive(Serialize, ToSchema, FromRow)]
pub(super) struct CardTopic {
    id: Uuid,
    name: String,
    #[sqlx(json)]
    filter: CardQuery,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/cards/tags", get(suggest_tags).put(edit_tags))
        .route("/api/study/card-topics", get(list_topics).post(save_topic))
        .route(
            "/api/study/card-topics/{id}",
            axum::routing::delete(delete_topic),
        )
}

pub(super) fn tags(values: Vec<String>) -> ApiResult<BTreeSet<String>> {
    if values.len() > 30 {
        return Err(ApiError::invalid("At most 30 tags are allowed"));
    }
    values
        .into_iter()
        .map(|tag| {
            let tag = normalize_tag(&tag)?;
            if tag.contains(',')
                || tag
                    .split("::")
                    .any(|segment| segment.is_empty() || segment.trim() != segment)
            {
                return Err(ApiError::invalid(
                    "Use nonempty tag names separated by :: for hierarchy",
                ));
            }
            Ok(tag)
        })
        .collect()
}

#[utoipa::path(get,path="/api/cards/tags",responses((status=200,body=Vec<String>)),tag="Learning")]
pub(super) async fn suggest_tags(State(state): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    let tags = sqlx::query_scalar("SELECT DISTINCT tag FROM learning_cards c CROSS JOIN LATERAL unnest(c.tags) tag ORDER BY tag LIMIT 500")
        .fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(tags))
}

#[utoipa::path(put,path="/api/cards/tags",request_body=EditCardTags,responses((status=200,body=Vec<CardTags>),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn edit_tags(
    State(state): State<AppState>,
    Json(body): Json<EditCardTags>,
) -> ApiResult<Json<Vec<CardTags>>> {
    if !(1..=200).contains(&body.cards.len()) {
        return Err(ApiError::invalid("Select between 1 and 200 cards"));
    }
    let add = tags(body.add)?;
    let remove = tags(body.remove)?;
    if !add.is_disjoint(&remove) {
        return Err(ApiError::invalid(
            "A tag cannot be added and removed together",
        ));
    }
    let mut expected = BTreeMap::new();
    for card in body.cards {
        if expected.insert(card.id, card.tags_version).is_some() || card.tags_version < 0 {
            return Err(ApiError::invalid(
                "Supply each card once with a valid tag version",
            ));
        }
    }
    let ids: Vec<_> = expected.keys().copied().collect();
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let mut cards: Vec<CardTags> = sqlx::query_as("SELECT id,tags,tags_version FROM learning_cards WHERE id=ANY($1) ORDER BY signature COLLATE \"C\" FOR UPDATE")
        .bind(ids).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    if cards.len() != expected.len() {
        return Err(ApiError::missing());
    }
    for card in &mut cards {
        if expected[&card.id] != card.tags_version {
            return Err(conflict("Card tags changed; reload them"));
        }
        update_card(&mut tx, card, &add, &remove).await?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(cards))
}

pub(super) async fn update_card(
    connection: &mut sqlx::PgConnection,
    card: &mut CardTags,
    add: &BTreeSet<String>,
    remove: &BTreeSet<String>,
) -> ApiResult<()> {
    let mut next: BTreeSet<_> = card.tags.iter().cloned().collect();
    next.retain(|tag| !remove.contains(tag));
    next.extend(add.iter().cloned());
    if next.len() > 30.max(card.tags.len()) {
        return Err(ApiError::invalid("At most 30 tags are allowed per card"));
    }
    let next: Vec<_> = next.into_iter().collect();
    if next != card.tags {
        *card = sqlx::query_as("UPDATE learning_cards SET tags=$2,tags_version=tags_version+1 WHERE id=$1 RETURNING id,tags,tags_version")
            .bind(card.id).bind(next).fetch_one(connection).await.map_err(anyhow::Error::from)?;
    }
    Ok(())
}

#[utoipa::path(get,path="/api/study/card-topics",responses((status=200,body=Vec<CardTopic>)),tag="Learning")]
pub(super) async fn list_topics(State(state): State<AppState>) -> ApiResult<Json<Vec<CardTopic>>> {
    let topics = sqlx::query_as("SELECT * FROM card_topics ORDER BY lower(name),id LIMIT 200")
        .fetch_all(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(topics))
}

#[utoipa::path(post,path="/api/study/card-topics",request_body=SaveCardTopic,responses((status=201,body=CardTopic),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn save_topic(
    State(state): State<AppState>,
    Json(body): Json<SaveCardTopic>,
) -> ApiResult<(StatusCode, Json<CardTopic>)> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "Topic names must contain 1 to 120 characters",
        ));
    }
    let filter = body.filter.validate()?;
    if filter.after.is_some() || filter.limit.is_some() {
        return Err(ApiError::invalid("Saved topics cannot contain pagination"));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Concurrent saves must not both pass the limit. Deletes only lower the
    // count and the unique name index rejects duplicates, so nothing else
    // waits for this lock.
    crate::advisory_locks::xact_lock(&mut tx, crate::advisory_locks::CARD_TOPIC_LIMIT)
        .await
        .map_err(anyhow::Error::from)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM card_topics")
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if count >= 200 {
        return Err(ApiError::invalid("At most 200 topics are allowed"));
    }
    let topic = sqlx::query_as("INSERT INTO card_topics(id,name,filter) VALUES($1,$2,$3) ON CONFLICT DO NOTHING RETURNING *")
        .bind(Uuid::new_v4()).bind(name).bind(sqlx::types::Json(filter))
        .fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?
        .ok_or_else(|| conflict("A topic with this name already exists"))?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(topic)))
}

#[utoipa::path(delete,path="/api/study/card-topics/{id}",params(("id"=Uuid,Path)),responses((status=204),(status=404,body=ErrorBody)),tag="Learning")]
pub(super) async fn delete_topic(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let deleted = sqlx::query("DELETE FROM card_topics WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    Ok(StatusCode::NO_CONTENT)
}
