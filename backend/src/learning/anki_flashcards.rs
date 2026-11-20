//! Rendered Anki cards share the ordinary deck and review tables. Card faces and
//! media are loaded separately from card lists and the learning queue.
use super::*;
use anyhow::{Context, Result, ensure};
use axum::extract::{DefaultBodyLimit, Multipart};
use axum::response::Response;
use sqlx::{PgConnection, Postgres, Transaction};
use std::{io::BufRead, path::Path as FilePath, process::Stdio, time::Duration};
use tokio::io::AsyncWriteExt;

const MAX_UPLOAD: usize = 128 * 1024 * 1024;
static PACKAGE_IMPORT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Serialize, Deserialize)]
pub(super) struct AnkiCard {
    pub kind: String,
    pub prompt: String,
}

#[derive(Deserialize)]
struct DeckImport {
    name: String,
    cards: usize,
}

#[derive(Deserialize)]
struct PackageImport {
    package: String,
    decks: Vec<DeckImport>,
}

#[derive(Deserialize)]
struct ImportedCard {
    guid: String,
    template_number: i32,
    deck: String,
    prompt: String,
    tags: Vec<String>,
    css: String,
    front: String,
    back: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/cards/{id}/anki/{side}", get(face))
        .route("/api/anki-media/{package}/{name}", get(media))
        .route(
            "/api/study/anki-imports",
            post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD + 64 * 1024)),
        )
}

#[derive(Serialize, ToSchema)]
pub(super) struct ImportedAnkiDeck {
    name: String,
    cards: usize,
}

#[utoipa::path(post,path="/api/study/anki-imports",request_body(content=super::anki_import::AnkiUpload,content_type="multipart/form-data"),
    responses((status=201,body=Vec<ImportedAnkiDeck>),(status=413,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Vec<ImportedAnkiDeck>>)> {
    let _permit = PACKAGE_IMPORT.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "An Anki package import is already running".into(),
        )
    })?;
    let mut field = multipart
        .next_field()
        .await
        .map_err(|error| ApiError(error.status(), error.body_text()))?
        .filter(|field| field.name() == Some("file"))
        .ok_or_else(|| ApiError::invalid("Select an Anki package"))?;
    let directory = tempfile::tempdir().map_err(anyhow::Error::from)?;
    let package = directory.path().join("upload.apkg");
    let mut file = tokio::fs::File::create(&package)
        .await
        .map_err(anyhow::Error::from)?;
    let mut size = 0;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| ApiError(error.status(), error.body_text()))?
    {
        size += chunk.len();
        if size > MAX_UPLOAD {
            return Err(ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Anki package exceeds 128 MiB".into(),
            ));
        }
        file.write_all(&chunk).await.map_err(anyhow::Error::from)?;
    }
    file.flush().await.map_err(anyhow::Error::from)?;
    drop(file);
    let inspected = package.clone();
    tokio::task::spawn_blocking(move || inspect_archive(&inspected))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|error| ApiError::invalid(format!("Invalid Anki package: {error}")))?;
    let output = directory.path().join("rendered");
    tokio::fs::create_dir(&output)
        .await
        .map_err(anyhow::Error::from)?;
    let python = std::env::var_os("NEONDOJO_ANKI_PYTHON").unwrap_or_else(|| "python3".into());
    let mut renderer = tokio::process::Command::new(python)
        .arg("-I")
        .arg("-c")
        .arg(include_str!("../../../tools/import_anki_flashcards.py"))
        .arg(&package)
        .arg("--render-to")
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("DATABASE_URL")
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| renderer_unavailable())?;
    let status = tokio::time::timeout(Duration::from_secs(90), renderer.wait())
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::GATEWAY_TIMEOUT,
                "Anki package rendering timed out".into(),
            )
        })?
        .map_err(anyhow::Error::from)?;
    if status.code() == Some(3) {
        return Err(renderer_unavailable());
    }
    if !status.success() {
        return Err(ApiError::invalid("Anki could not render this package"));
    }
    let decks = import_rendered_package(&state.pool, &output)
        .await
        .map_err(|error| {
            if error.is::<sqlx::Error>() {
                ApiError::from(error)
            } else {
                ApiError::invalid(format!("Cannot import this Anki package: {error}"))
            }
        })?;
    Ok((
        StatusCode::CREATED,
        Json(
            decks
                .into_iter()
                .map(|(name, cards)| ImportedAnkiDeck { name, cards })
                .collect(),
        ),
    ))
}

fn renderer_unavailable() -> ApiError {
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "Install the Anki Python package and set NEONDOJO_ANKI_PYTHON to its Python interpreter"
            .into(),
    )
}

fn inspect_archive(path: &FilePath) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    ensure!(archive.len() <= 20_000, "Too many package entries");
    let mut size = 0_u64;
    let mut collection = false;
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index)?;
        size = size.saturating_add(file.size());
        ensure!(
            size <= 512 * 1024 * 1024,
            "Uncompressed package exceeds 512 MiB"
        );
        collection |= matches!(
            file.name(),
            "collection.anki2" | "collection.anki21" | "collection.anki21b"
        );
    }
    ensure!(collection, "No Anki collection in the package");
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Side {
    Front,
    Back,
}

#[derive(FromRow)]
struct CardFace {
    package: String,
    css: String,
    content: String,
    template_number: i32,
}

#[utoipa::path(get, path="/api/cards/{id}/anki/{side}", params(("id"=Uuid,Path),("side"=String,Path)),
    responses((status=200, content_type="text/html", body=String), (status=404, body=ErrorBody)), tag="Learning")]
pub(super) async fn face(
    State(state): State<AppState>,
    Path((id, side)): Path<(Uuid, Side)>,
) -> ApiResult<Response> {
    let row = sqlx::query_as::<_, CardFace>(
        "SELECT package, css, CASE WHEN $2 THEN back ELSE front END AS content, template_number
         FROM anki_card_faces WHERE card_id=$1",
    )
    .bind(id)
    .bind(matches!(side, Side::Back))
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, "sandbox allow-scripts; default-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data:; media-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><base href="/api/anki-media/{}/" target="_blank"><style>{}</style></head><body class="card card{} mac">{}</body></html>"#,
            row.package, row.css, row.template_number, row.content,
        ),
    )
        .into_response())
}

#[utoipa::path(get, path="/api/anki-media/{package}/{name}", params(("package"=String,Path),("name"=String,Path)),
    responses((status=200, description="Anki media bytes", body=Vec<u8>), (status=404, body=ErrorBody)), tag="Learning")]
pub(super) async fn media(
    State(state): State<AppState>,
    Path((package, name)): Path<(String, String)>,
) -> ApiResult<Response> {
    let (content_type, content): (String, Vec<u8>) =
        sqlx::query_as("SELECT content_type,content FROM anki_media WHERE package=$1 AND name=$2")
            .bind(package)
            .bind(name)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type.as_str()),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        content,
    )
        .into_response())
}

fn media_type(name: &str, bytes: &[u8]) -> Result<&'static str> {
    let media_type = match bytes {
        [0xff, 0xd8, 0xff, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => "image/png",
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => anyhow::bail!("Unsupported media in {name}"),
    };
    Ok(media_type)
}

async fn save_card(
    tx: &mut PgConnection,
    package: &str,
    input: ImportedCard,
    deck_id: Uuid,
) -> Result<bool> {
    ensure!(
        !input.guid.is_empty() && input.guid.len() <= 64,
        "Invalid Anki card GUID"
    );
    ensure!(input.template_number > 0, "Invalid Anki template number");
    ensure!(
        input.prompt.len() <= 500 && !input.front.is_empty() && !input.back.is_empty(),
        "Invalid Anki card content"
    );
    ensure!(
        input.css.len() <= 1024 * 1024
            && input.front.len() <= 2 * 1024 * 1024
            && input.back.len() <= 2 * 1024 * 1024,
        "Anki card is too large"
    );
    ensure!(input.tags.len() <= 100, "Too many Anki tags");
    let mut tags = BTreeSet::new();
    for tag in input.tags {
        // A comma separates terms in the Neondojo topic filter.
        let tag = tag.replace(',', " ");
        tags.extend(topics::tags(vec![tag]).map_err(|error| anyhow::anyhow!("{}", error.1))?);
    }
    let signature = format!("anki:{}:{}", input.guid, input.template_number);
    let payload = serde_json::to_value(AnkiCard {
        kind: "anki".into(),
        prompt: input.prompt,
    })?;
    let candidate = Uuid::new_v4();
    let card_id: Uuid = sqlx::query_scalar(
        "INSERT INTO learning_cards(id,signature,payload,tags) VALUES($1,$2,$3,$4)
         ON CONFLICT(signature) DO UPDATE SET payload=EXCLUDED.payload RETURNING id",
    )
    .bind(candidate)
    .bind(&signature)
    .bind(&payload)
    .bind(tags.into_iter().collect::<Vec<_>>())
    .fetch_one(&mut *tx)
    .await?;
    let updated = sqlx::query(
        "INSERT INTO anki_card_faces(card_id,package,css,front,back,template_number)
         VALUES($1,$2,$3,$4,$5,$6)
         ON CONFLICT(card_id) DO UPDATE SET package=EXCLUDED.package,css=EXCLUDED.css,
           front=EXCLUDED.front,back=EXCLUDED.back,template_number=EXCLUDED.template_number
         WHERE anki_card_faces IS DISTINCT FROM EXCLUDED",
    )
    .bind(card_id)
    .bind(package)
    .bind(input.css)
    .bind(input.front)
    .bind(input.back)
    .bind(input.template_number)
    .execute(&mut *tx)
    .await?;
    if card_id != candidate && updated.rows_affected() > 0 {
        sqlx::query("UPDATE learning_cards SET version=version+1,updated_at=now() WHERE id=$1")
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
    }
    let membership =
        sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(deck_id)
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
    Ok(membership.rows_affected() == 1)
}

async fn source_deck(tx: &mut Transaction<'_, Postgres>, name: &str) -> Result<Uuid> {
    deck_name(name).map_err(|error| anyhow::anyhow!("{}", error.1))?;
    if let Some(id) =
        sqlx::query_scalar("SELECT deck_id FROM anki_source_decks WHERE source_name=$1")
            .bind(name)
            .fetch_optional(&mut **tx)
            .await?
    {
        sqlx::query("SELECT id FROM decks WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        return Ok(id);
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,$2)")
        .bind(id)
        .bind(name)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO anki_source_decks(source_name,deck_id) VALUES($1,$2)")
        .bind(name)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

/// Import an Anki collection prepared by tools/import_anki_flashcards.py.
pub async fn import_rendered_package(
    pool: &PgPool,
    directory: &FilePath,
) -> Result<Vec<(String, usize)>> {
    let manifest: PackageImport =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    ensure!(
        manifest.package.len() == 64
            && manifest
                .package
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "Invalid package fingerprint"
    );
    ensure!(!manifest.decks.is_empty(), "No Anki decks");
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('neondojo-anki-import',0))")
        .execute(&mut *tx)
        .await?;
    let mut decks = BTreeMap::new();
    for deck in manifest.decks {
        ensure!(
            (1..=MAX_CARDS as usize).contains(&deck.cards),
            "Invalid size for {}",
            deck.name
        );
        let id = source_deck(&mut tx, &deck.name).await?;
        ensure!(
            decks
                .insert(deck.name, (id, deck.cards, 0, false))
                .is_none(),
            "Duplicate source deck"
        );
    }

    for file in std::fs::read_dir(directory.join("media"))? {
        let file = file?;
        ensure!(file.file_type()?.is_file(), "Invalid media entry");
        let name = file
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Invalid media filename"))?;
        ensure!(
            !name.chars().any(char::is_control) && name.len() <= 255,
            "Invalid media filename"
        );
        let bytes = std::fs::read(file.path())?;
        ensure!(bytes.len() <= 32 * 1024 * 1024, "Anki media is too large");
        let content_type = media_type(&name, &bytes)?;
        sqlx::query("INSERT INTO anki_media(package,name,content_type,content) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&manifest.package)
            .bind(name)
            .bind(content_type)
            .bind(bytes)
            .execute(&mut *tx)
            .await?;
    }

    let cards = std::io::BufReader::new(std::fs::File::open(directory.join("cards.jsonl"))?);
    let mut imported = BTreeSet::new();
    for line in cards.lines() {
        let input: ImportedCard = serde_json::from_str(&line?)?;
        let (deck_id, expected, seen, added) =
            decks.get_mut(&input.deck).context("Unknown source deck")?;
        *seen += 1;
        ensure!(*seen <= *expected, "More cards than expected");
        ensure!(
            imported.insert((
                input.deck.clone(),
                input.guid.clone(),
                input.template_number
            )),
            "Duplicate Anki card"
        );
        *added |= save_card(&mut tx, &manifest.package, input, *deck_id).await?;
    }
    let mut counts = Vec::new();
    for (name, (id, expected, seen, added)) in decks {
        ensure!(
            seen == expected,
            "Incomplete deck {name}: {seen}/{expected}"
        );
        check_deck_capacity(&mut tx, id)
            .await
            .map_err(|error| anyhow::anyhow!("{}", error.1))?;
        if added {
            sqlx::query("UPDATE decks SET version=version+1 WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        counts.push((name, seen));
    }
    tx.commit().await?;
    Ok(counts)
}
