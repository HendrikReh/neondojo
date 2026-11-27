//! Import the structured decision model emitted by NeonDojo APKG exports.
use super::super::*;
use crate::json_errors::Multipart;
use std::io::{Cursor, Read, Write};

const MAX_DB_BYTES: u64 = 32 * 1024 * 1024;

#[derive(ToSchema)]
#[allow(dead_code)]
pub(in crate::learning) struct AnkiUpload {
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post,path="/api/decks/{id}/import.apkg",params(("id"=Uuid,Path)),request_body(content=AnkiUpload,content_type="multipart/form-data"),responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(in crate::learning) async fn import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Multipart(mut multipart): Multipart,
) -> ApiResult<Json<AddCardsResult>> {
    require_deck(&state.pool, id).await?;
    let permit = state
        .anki_limits
        .decision_imports
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Two Anki imports are already in progress".into(),
            )
        })?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text().into()))?
        .filter(|field| field.name() == Some("file"))
        .ok_or_else(|| ApiError::invalid("Select an Anki package"))?;
    let bytes = field
        .bytes()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text().into()))?;
    let cards = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read_package(&bytes)
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|error| ApiError::invalid(format!("Cannot import this Anki package: {error}")))?;
    let unique: BTreeMap<_, _> = cards
        .into_iter()
        .map(|(content, tags)| (card_signature(&content), (content, tags)))
        .collect();
    let written = write_cards_with_policy(
        &state.pool,
        id,
        unique,
        CardConflictPolicy::PreserveExistingPayload,
    )
    .await?;
    Ok(Json(written.summary()))
}

pub(super) fn read_package(bytes: &[u8]) -> anyhow::Result<Vec<(CardContent, BTreeSet<String>)>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let collection = archive.by_name("collection.anki2")?;
    anyhow::ensure!(collection.size() <= MAX_DB_BYTES, "Collection is too large");
    let mut tempfile = tempfile::NamedTempFile::new()?;
    let copied = std::io::copy(&mut collection.take(MAX_DB_BYTES + 1), &mut tempfile)?;
    anyhow::ensure!(copied <= MAX_DB_BYTES, "Collection is too large");
    tempfile.flush()?;
    let db = crate::untrusted_sqlite::open(tempfile.path())?;
    anyhow::ensure!(
        crate::untrusted_sqlite::is_regular_table(&db, "notes")?,
        "Expected a standard notes table"
    );
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
            .map_err(|_| anyhow::anyhow!("Expected the NeonDojo structured decision model"))?;
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
        CardSource::strip_local_refs(&mut content);
        let tags: String = row.get(1)?;
        let tags = topics::tags(tags.split_whitespace().map(str::to_owned).collect())
            .map_err(|e| anyhow::anyhow!("{}", e.1.error))?;
        cards.insert(card_signature(&content), (content, tags));
    }
    anyhow::ensure!(
        !cards.is_empty(),
        "Package contains no structured decisions"
    );
    Ok(cards.into_values().collect())
}
