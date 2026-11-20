//! Import the structured decision model emitted by Neondojo APKG exports.
use super::*;
use axum::extract::Multipart;
use rusqlite::{Connection, OpenFlags};
use std::io::{Cursor, Read, Write};

const MAX_DB_BYTES: u64 = 32 * 1024 * 1024;
static IMPORTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(ToSchema)]
#[allow(dead_code)]
pub(super) struct AnkiUpload {
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post,path="/api/decks/{id}/import.apkg",params(("id"=Uuid,Path)),request_body(content=AnkiUpload,content_type="multipart/form-data"),responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> ApiResult<Json<AddCardsResult>> {
    require_deck(&state.pool, id).await?;
    let permit = IMPORTS.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Two Anki imports are already in progress".into(),
        )
    })?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text()))?
        .filter(|field| field.name() == Some("file"))
        .ok_or_else(|| ApiError::invalid("Select an Anki package"))?;
    let bytes = field
        .bytes()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text()))?;
    let cards = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read_package(&bytes)
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|error| ApiError::invalid(format!("Cannot import this Anki package: {error}")))?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SELECT id FROM decks WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let mut added = 0;
    let mut updated = 0;
    let mut memberships = 0;
    for (content, tags) in cards {
        let signature = card_signature(&content);
        let new_id = Uuid::new_v4();
        let payload = serde_json::to_value(content).map_err(anyhow::Error::from)?;
        let inserted: Option<Uuid> = sqlx::query_scalar("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3) ON CONFLICT(signature) DO NOTHING RETURNING id")
            .bind(new_id).bind(&signature).bind(payload).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
        let mut card: topics::CardTags = sqlx::query_as(
            "SELECT id,tags,tags_version FROM learning_cards WHERE signature=$1 FOR UPDATE",
        )
        .bind(signature)
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        topics::update_card(&mut tx, &mut card, &tags, &BTreeSet::new()).await?;
        memberships += sqlx::query(
            "INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(card.id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .rows_affected();
        if inserted.is_some() {
            added += 1;
        } else {
            updated += 1;
        }
    }
    let total = check_deck_capacity(&mut tx, id).await?;
    if memberships > 0 {
        sqlx::query("UPDATE decks SET version=version+1 WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(AddCardsResult {
        added,
        updated,
        total,
    }))
}

pub(super) fn read_package(bytes: &[u8]) -> anyhow::Result<Vec<(CardContent, BTreeSet<String>)>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let collection = archive.by_name("collection.anki2")?;
    anyhow::ensure!(collection.size() <= MAX_DB_BYTES, "Collection is too large");
    let mut tempfile = tempfile::NamedTempFile::new()?;
    let copied = std::io::copy(&mut collection.take(MAX_DB_BYTES + 1), &mut tempfile)?;
    anyhow::ensure!(copied <= MAX_DB_BYTES, "Collection is too large");
    tempfile.flush()?;
    let db = Connection::open_with_flags(
        tempfile.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA,
        false,
    )?;
    let notes: bool = db.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type='table' AND name='notes' AND upper(sql) NOT LIKE '%VIRTUAL%')", [], |row| row.get(0))?;
    anyhow::ensure!(notes, "Expected a standard notes table");
    let mut statement = db.prepare("SELECT data,tags FROM notes LIMIT 1001")?;
    let mut rows = statement.query([])?;
    let mut cards = BTreeMap::new();
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        anyhow::ensure!(count <= MAX_CARDS, "Package exceeds 1000 notes");
        let data: String = row.get(0)?;
        anyhow::ensure!(data.len() <= 65536, "Structured decision is too large");
        let mut content: CardContent = serde_json::from_str(&data)
            .map_err(|_| anyhow::anyhow!("Expected the Neondojo structured decision model"))?;
        let position = &mut content.position;
        anyhow::ensure!(
            matches!(position.player, 0 | 1)
                && if content.solution.imported.is_some() {
                    content
                        .solution
                        .plies
                        .is_none_or(|plies| (0..=64).contains(&plies))
                } else {
                    content
                        .solution
                        .plies
                        // Decks exported before migration 0079 may carry
                        // GNUbg's native 0-ply; current depths are 1..5.
                        .is_some_and(|plies| (0..=5).contains(&plies))
                }
                && !content.solution.engine.trim().is_empty(),
            "Invalid decision metadata"
        );
        let valid = match content.kind.as_str() {
            "checker" => {
                crate::practice::validate_checker_context(position).is_ok()
                    && has_usable_checker_solution(&content.solution)
                    && content.solution.decision.as_ref().is_none_or(|decision| {
                        decision.kind == "checker"
                            && decision.player == position.player
                            && (decision.evaluation_status == "forced"
                                || (crate::match_analysis::evaluated_equities(decision).is_some()
                                    && crate::move_assessments::after_play(
                                        position,
                                        &decision.best,
                                    )
                                    .is_some()
                                    && crate::move_assessments::after_play(
                                        position,
                                        &decision.played,
                                    )
                                    .is_some()))
                    })
                    && content.solution.candidates.iter().all(|candidate| {
                        candidate.equity.is_finite()
                            && candidate.rank > 0
                            && crate::move_assessments::after_play(position, &candidate.notation)
                                .is_some()
                    })
            }
            "cube_offer" | "cube_response" => {
                crate::practice::cube_context(position, content.kind == "cube_response").is_ok()
                    && content.solution.decision.as_ref().is_some_and(|decision| {
                        let action = |value: &str| {
                            if content.kind == "cube_response" {
                                matches!(value, "take" | "drop")
                            } else {
                                matches!(value, "no double" | "double")
                            }
                        };
                        decision.kind == "cube"
                            && decision.player == position.player
                            && action(&decision.played)
                            && action(&decision.best)
                            && crate::match_analysis::evaluated_equities(decision).is_some()
                    })
            }
            _ => false,
        };
        anyhow::ensure!(valid, "Invalid or unscorable backgammon decision");
        position.id = Uuid::nil();
        position.match_uuid = Uuid::nil();
        content.source = CardSource {
            kind: "anki".into(),
            job_id: None,
            analysis_id: None,
            imported_analysis_id: None,
            match_id: Uuid::nil(),
            position_id: None,
            game_no: 1,
            decision_no: None,
        };
        let tags: String = row.get(1)?;
        let tags = topics::tags(tags.split_whitespace().map(str::to_owned).collect())
            .map_err(|e| anyhow::anyhow!("{}", e.1))?;
        cards.insert(card_signature(&content), (content, tags));
    }
    anyhow::ensure!(
        !cards.is_empty(),
        "Package contains no structured decisions"
    );
    Ok(cards.into_values().collect())
}
