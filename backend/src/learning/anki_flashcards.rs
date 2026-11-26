//! Rendered Anki cards share the ordinary deck and review tables. Card faces and
//! media are loaded separately from card lists and the learning queue.
use super::*;
use crate::json_errors::Multipart;
use anyhow::{Context, Result, ensure};
use axum::extract::DefaultBodyLimit;
use axum::response::Response;
use sqlx::{PgConnection, Postgres, Transaction};
use std::{path::Path as FilePath, process::Stdio, time::Duration};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

const MAX_UPLOAD: usize = 128 * 1024 * 1024;
static PACKAGE_IMPORT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// The payload `kind` of a rendered Anki card.
pub(super) const KIND: &str = "anki";

#[derive(Debug, Serialize, Deserialize)]
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
    Multipart(mut multipart): Multipart,
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
        .map_err(|error| ApiError(error.status(), error.body_text().into()))?
        .filter(|field| field.name() == Some("file"))
        .ok_or_else(|| ApiError::invalid("Select an Anki package"))?;
    let directory = crate::async_temp_dir::AsyncTempDir::new().await?;
    let package = directory.path().join("upload.apkg");
    let mut file = tokio::fs::File::create(&package)
        .await
        .map_err(anyhow::Error::from)?;
    let mut size = 0;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| ApiError(error.status(), error.body_text().into()))?
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
    let (directory, inspection) = tokio::task::spawn_blocking(move || {
        let result = inspect_archive(&inspected);
        (directory, result)
    })
    .await
    .map_err(anyhow::Error::from)?;
    inspection.map_err(|error| ApiError::invalid(format!("Invalid Anki package: {error}")))?;
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

/// Upper bound for the decompressed contents of an uploaded package.
const MAX_UNCOMPRESSED: u64 = 512 * 1024 * 1024;

fn inspect_archive(path: &FilePath) -> Result<()> {
    inspect_archive_within(path, MAX_UNCOMPRESSED)
}

/// Checks that the package holds an Anki collection and decompresses to at
/// most `limit` bytes. The sizes in ZIP headers are the uploader's claims, so
/// every entry is decompressed and counted; this also verifies its checksum.
fn inspect_archive_within(path: &FilePath, limit: u64) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    ensure!(archive.len() <= 20_000, "Too many package entries");
    let mut remaining = limit;
    let mut collection = false;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        collection |= matches!(
            file.name(),
            "collection.anki2" | "collection.anki21" | "collection.anki21b"
        );
        let read = std::io::copy(
            &mut std::io::Read::take(&mut file, remaining.saturating_add(1)),
            &mut std::io::sink(),
        )?;
        ensure!(
            read <= remaining,
            "Uncompressed package exceeds {} MiB",
            limit / (1024 * 1024)
        );
        remaining -= read;
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
        // A comma separates terms in the NeonDojo topic filter.
        let tag = tag.replace(',', " ");
        tags.extend(topics::tags(vec![tag]).map_err(|error| anyhow::anyhow!("{}", error.1.error))?);
    }
    let signature = format!("anki:{}:{}", input.guid, input.template_number);
    let payload = serde_json::to_value(AnkiCard {
        kind: KIND.into(),
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
    deck_name(name).map_err(|error| anyhow::anyhow!("{}", error.1.error))?;
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

const MAX_MEDIA_FILE_BYTES: u64 = 32 * 1024 * 1024;
// JSON escaping can take six bytes per byte of the 5 MiB of permitted CSS and card faces.
const MAX_CARD_RECORD_BYTES: u64 = 32 * 1024 * 1024;

async fn read_media_file(path: &FilePath, remaining: &mut u64) -> Result<Vec<u8>> {
    let limit = MAX_MEDIA_FILE_BYTES.min(*remaining);
    let message = if limit == MAX_MEDIA_FILE_BYTES {
        "Anki media is too large"
    } else {
        "Anki package media exceeds the total size limit"
    };
    let file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    ensure!(size <= limit, message);

    // Count actual bytes as well, even if the file grows after the metadata check.
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(limit + 1).read_to_end(&mut bytes).await?;
    let read = bytes.len() as u64;
    ensure!(read <= limit, message);
    *remaining -= read;
    Ok(bytes)
}

async fn read_card_record(
    reader: &mut (impl AsyncBufRead + Unpin),
    limit: u64,
) -> Result<Option<ImportedCard>> {
    let mut line = Vec::new();
    // Allow for CRLF as well as LF and an unterminated final record.
    if reader.take(limit + 2).read_until(b'\n', &mut line).await? == 0 {
        return Ok(None);
    }
    let payload = line
        .strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .unwrap_or(&line);
    ensure!(
        payload.len() as u64 <= limit,
        "Anki card record is too large"
    );
    Ok(Some(serde_json::from_str(std::str::from_utf8(payload)?)?))
}

/// Import an Anki collection prepared by tools/import_anki_flashcards.py.
pub async fn import_rendered_package(
    pool: &PgPool,
    directory: &FilePath,
) -> Result<Vec<(String, usize)>> {
    let manifest: PackageImport =
        serde_json::from_slice(&tokio::fs::read(directory.join("manifest.json")).await?)?;
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
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::advisory_locks::ANKI_PACKAGE_IMPORT)
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

    let mut remaining_media = MAX_UNCOMPRESSED;
    let mut media = tokio::fs::read_dir(directory.join("media")).await?;
    while let Some(file) = media.next_entry().await? {
        ensure!(file.file_type().await?.is_file(), "Invalid media entry");
        let name = file
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Invalid media filename"))?;
        ensure!(
            !name.chars().any(char::is_control) && name.len() <= 255,
            "Invalid media filename"
        );
        let bytes = read_media_file(&file.path(), &mut remaining_media).await?;
        let content_type = media_type(&name, &bytes)?;
        sqlx::query("INSERT INTO anki_media(package,name,content_type,content) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&manifest.package)
            .bind(name)
            .bind(content_type)
            .bind(bytes)
            .execute(&mut *tx)
            .await?;
    }

    let mut cards =
        tokio::io::BufReader::new(tokio::fs::File::open(directory.join("cards.jsonl")).await?);
    let mut imported = BTreeSet::new();
    while let Some(input) = read_card_record(&mut cards, MAX_CARD_RECORD_BYTES).await? {
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
            .map_err(|error| anyhow::anyhow!("{}", error.1.error))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const MIB: u64 = 1024 * 1024;

    #[tokio::test]
    async fn media_reads_enforce_individual_and_remaining_package_bytes() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"GIF89a").unwrap();
        let mut remaining = 12;
        for _ in 0..2 {
            assert_eq!(
                read_media_file(file.path(), &mut remaining).await.unwrap(),
                b"GIF89a"
            );
        }
        assert_eq!(remaining, 0);
        assert_eq!(
            read_media_file(file.path(), &mut remaining)
                .await
                .unwrap_err()
                .to_string(),
            "Anki package media exceeds the total size limit"
        );

        file.as_file().set_len(32 * MIB + 1).unwrap();
        let mut remaining = MAX_UNCOMPRESSED;
        assert_eq!(
            read_media_file(file.path(), &mut remaining)
                .await
                .unwrap_err()
                .to_string(),
            "Anki media is too large"
        );
        assert_eq!(remaining, MAX_UNCOMPRESSED);
    }

    #[tokio::test]
    async fn card_record_limit_excludes_line_endings() {
        let row = r#"{"guid":"x","template_number":1,"deck":"D","prompt":"","tags":[],"css":"","front":"Q","back":"A"}"#;
        let limit = row.len() as u64;
        let records = format!("{row}\r\n{row}\n{row}");
        let mut reader = records.as_bytes();
        for _ in 0..3 {
            assert_eq!(
                read_card_record(&mut reader, limit)
                    .await
                    .unwrap()
                    .unwrap()
                    .guid,
                "x"
            );
        }
        assert!(
            read_card_record(&mut reader, limit)
                .await
                .unwrap()
                .is_none()
        );
        for ending in ["", "\r\n", "\n"] {
            let oversized = format!("{row} {ending}");
            let error = read_card_record(&mut oversized.as_bytes(), limit)
                .await
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "Anki card record is too large");
        }
    }

    /// A package with a collection and one deflated entry of `size` zero bytes.
    /// With `claimed_size`, both ZIP headers of that entry claim that size.
    fn package(size: u32, claimed_size: Option<u32>) -> tempfile::NamedTempFile {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("collection.anki21", options).unwrap();
        zip.write_all(b"SQLite format 3\0").unwrap();
        zip.start_file("0", options).unwrap();
        zip.write_all(&vec![0; size as usize]).unwrap();
        let mut bytes = zip.finish().unwrap().into_inner();
        if let Some(claimed) = claimed_size {
            // The uncompressed size is at offset 22 of a local file header and
            // at offset 24 of a central directory header.
            let mut patched = 0;
            for (signature, offset) in [(0x0403_4b50_u32, 22), (0x0201_4b50, 24)] {
                for start in 0..bytes.len() - offset - 4 {
                    let field = start + offset..start + offset + 4;
                    if bytes[start..start + 4] == signature.to_le_bytes()
                        && bytes[field.clone()] == size.to_le_bytes()
                    {
                        bytes[field].copy_from_slice(&claimed.to_le_bytes());
                        patched += 1;
                    }
                }
            }
            assert_eq!(patched, 2, "both headers of the entry are patched");
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();
        file
    }

    #[test]
    fn package_size_counts_decompressed_bytes() {
        let honest = package(2 * MIB as u32, None);
        inspect_archive_within(honest.path(), 3 * MIB).unwrap();
        let error = inspect_archive_within(honest.path(), MIB).unwrap_err();
        assert_eq!(error.to_string(), "Uncompressed package exceeds 1 MiB");

        // Both headers claim 16 bytes, so summing them would accept it.
        let lying = package(2 * MIB as u32, Some(16));
        let mut archive = zip::ZipArchive::new(std::fs::File::open(lying.path()).unwrap()).unwrap();
        assert_eq!(archive.by_index_raw(1).unwrap().size(), 16);
        let error = inspect_archive_within(lying.path(), MIB).unwrap_err();
        assert_eq!(error.to_string(), "Uncompressed package exceeds 1 MiB");
    }

    #[test]
    fn package_needs_a_collection() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("media", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"{}").unwrap();
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&zip.finish().unwrap().into_inner()).unwrap();
        let error = inspect_archive(file.path()).unwrap_err();
        assert_eq!(error.to_string(), "No Anki collection in the package");
    }
}
