use super::{
    BackupContents, MAX_ARCHIVE_BYTES, MAX_DATA_BYTES, Manifest, TABLES, columns, relation, schema,
    sequences, transfer_settings,
};
use crate::api::{ApiError, ApiResult};
use anyhow::{Context, Result, ensure};
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode, header},
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use sqlx::{Acquire, PgPool};
use std::{
    io::{self, Read, Seek, Write},
    sync::Arc,
};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{
        OwnedSemaphorePermit,
        mpsc::{self, Sender},
    },
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MANIFEST: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 8192;
const LEGACY_FORMAT: u32 = 1;
const FORMAT: u32 = 2;
const SUMMARY_TABLES: [&str; 3] = ["matches", "saved_positions", "learning_cards"];

enum ExportEntry {
    Table(&'static str),
    Data(Bytes),
    Finish(Manifest),
}

pub(super) struct Prepared {
    pub file: File,
    pub manifest: Manifest,
    pub lengths: Vec<u64>,
}

fn entry(table: &str) -> String {
    format!("tables/{table}.copy")
}

fn zip_archive(mut incoming: mpsc::Receiver<ExportEntry>) -> Result<std::fs::File> {
    let mut zip = ZipWriter::new(tempfile::tempfile()?);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    let mut total = 0_u64;
    while let Some(message) = incoming.blocking_recv() {
        match message {
            ExportEntry::Table(table) => zip.start_file(entry(table), options)?,
            ExportEntry::Data(chunk) => {
                total += chunk.len() as u64;
                ensure!(total <= MAX_DATA_BYTES, "Backup data exceeds size limit");
                zip.write_all(&chunk)?;
            }
            ExportEntry::Finish(manifest) => {
                zip.start_file(MANIFEST, options)?;
                serde_json::to_writer(&mut zip, &manifest)?;
                let mut file = zip.finish()?;
                ensure!(
                    file.metadata()?.len() <= MAX_ARCHIVE_BYTES,
                    "Backup archive exceeds size limit"
                );
                file.rewind()?;
                return Ok(file);
            }
        }
    }
    anyhow::bail!("Backup export was interrupted")
}

async fn stream_snapshot(
    pool: &PgPool,
    created_at: DateTime<Utc>,
    outgoing: Sender<ExportEntry>,
) -> Result<()> {
    let mut connection = pool.acquire().await?;
    // Cancel an unfinished COPY by closing this connection instead of returning
    // it to the pool with pending COPY protocol messages.
    connection.close_on_drop();
    let mut tx = connection.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    transfer_settings(&mut tx).await?;
    let schema = schema(&mut tx).await?;
    for table in TABLES {
        let columns = columns(&mut tx, table).await?;
        outgoing.send(ExportEntry::Table(table)).await?;
        let mut stream = tx
            .copy_out_raw(&format!(
                "COPY {} ({columns}) TO STDOUT WITH (ENCODING 'UTF8')",
                relation(table)
            ))
            .await?;
        while let Some(chunk) = stream.next().await {
            outgoing.send(ExportEntry::Data(chunk?)).await?;
        }
    }
    let mut next_values = Vec::new();
    for sequence in sequences(&mut tx).await? {
        let (value, called): (i64, bool) = sqlx::query_as(&format!(
            "SELECT last_value, is_called FROM {}",
            relation(&sequence.name)
        ))
        .fetch_one(&mut *tx)
        .await?;
        next_values.push(if called {
            value.checked_add(1).context("Backup sequence exhausted")?
        } else {
            value
        });
    }
    tx.commit().await?;
    outgoing
        .send(ExportEntry::Finish(Manifest {
            format: FORMAT,
            schema,
            sequences: next_values,
            created_at: Some(created_at),
        }))
        .await?;
    Ok(())
}

pub(super) async fn export(pool: &PgPool, created_at: DateTime<Utc>) -> Result<(File, u64)> {
    let (sender, receiver) = mpsc::channel(16);
    let writer = tokio::task::spawn_blocking(move || zip_archive(receiver));
    let snapshot = stream_snapshot(pool, created_at, sender).await;
    let result = writer.await?;
    // Surface a ZIP or disk error if the writer stopped receiving.
    let file = File::from_std(match snapshot {
        Ok(()) => result?,
        Err(error) => match result {
            Err(writer_error)
                if error
                    .downcast_ref::<mpsc::error::SendError<ExportEntry>>()
                    .is_some() =>
            {
                return Err(writer_error);
            }
            _ => return Err(error),
        },
    });
    let length = file.metadata().await?.len();
    Ok((file, length))
}

fn too_large() -> ApiError {
    ApiError(
        StatusCode::PAYLOAD_TOO_LARGE,
        "Backup exceeds the 16 GiB file limit".into(),
    )
}

pub(super) async fn receive(headers: &HeaderMap, body: Body) -> ApiResult<File> {
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok())
        .and_then(|length| length.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err(too_large());
    }
    let mut file = File::from_std(tempfile::tempfile().context("Creating backup upload file")?);
    let mut stream = body.into_data_stream();
    let mut length = 0_u64;
    while let Some(chunk) = timeout(Duration::from_secs(60), stream.next())
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::REQUEST_TIMEOUT,
                "Backup upload timed out".into(),
            )
        })?
    {
        let chunk = chunk.map_err(|_| ApiError::invalid("Backup upload was interrupted"))?;
        length += chunk.len() as u64;
        if length > MAX_ARCHIVE_BYTES {
            return Err(too_large());
        }
        file.write_all(&chunk)
            .await
            .context("Storing backup upload")?;
    }
    file.flush().await.context("Finishing backup upload")?;
    Ok(file)
}

pub(super) struct Checked {
    pub manifest: Manifest,
    pub compatible: bool,
    pub contents: Option<BackupContents>,
}

// Format 1 table exports use catalog column order. The 12th COPY field
// is database_id, introduced by migration 0009. Collection-only hosts
// carry saved positions and do not appear in the match library.
struct MatchCounter {
    column: usize,
    prefix: usize,
    matches: u64,
    collection: String,
}

impl MatchCounter {
    fn new() -> Self {
        Self {
            column: 0,
            prefix: 0,
            matches: 0,
            collection: crate::position_retention::COLLECTION_DATABASE_ID.to_string(),
        }
    }

    fn count(&mut self, chunk: &[u8]) {
        const DATABASE_ID: usize = 11;
        for part in chunk.split_inclusive(|&b| b == b'\n') {
            let complete = part.ends_with(b"\n");
            if self.column <= DATABASE_ID {
                for value in part.split_inclusive(|&b| b == b'\t') {
                    if self.column == DATABASE_ID {
                        let separator = value.ends_with(b"\t");
                        let field = if separator {
                            &value[..value.len() - 1]
                        } else {
                            value
                        };
                        let collection = self.collection.as_bytes();
                        self.prefix = match collection.get(self.prefix..) {
                            Some(rest) if rest.starts_with(field) => self.prefix + field.len(),
                            _ => collection.len() + 1,
                        };
                        if separator {
                            self.column += 1;
                            break;
                        }
                    } else if value.ends_with(b"\t") {
                        self.column += 1;
                    }
                }
            }
            if complete {
                if self.prefix != self.collection.len() || self.column <= DATABASE_ID {
                    self.matches += 1;
                }
                self.column = 0;
                self.prefix = 0;
            }
        }
    }
}

struct Scanned<W> {
    checked: Checked,
    lengths: Vec<u64>,
    output: W,
}

// A compatible archive is decoded to EOF to check every table. For a
// different schema, only the entries needed for the summary are inspected.
async fn scan<W: Write + Send + 'static>(
    file: File,
    fingerprint: String,
    destination: impl FnOnce() -> io::Result<W> + Send + 'static,
    permit: Arc<OwnedSemaphorePermit>,
    summarize: bool,
) -> ApiResult<Scanned<W>> {
    let cancel = CancellationToken::new();
    let _cancel_on_drop = cancel.clone().drop_guard();
    let file = file.into_std().await;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut archive = ZipArchive::new(file)
            .map_err(|_| ApiError::invalid("Invalid Neondojo backup archive"))?;
        let manifest = (|| -> Result<Manifest> {
            let mut entry = archive.by_name(MANIFEST)?;
            ensure!(entry.size() <= MAX_MANIFEST_BYTES);
            let mut bytes = Vec::new();
            (&mut entry)
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_MANIFEST_BYTES);
            let manifest: Manifest = serde_json::from_slice(&bytes)?;
            ensure!(
                manifest.schema.len() == 64
                    && manifest.schema.bytes().all(|b| b.is_ascii_hexdigit())
            );
            ensure!(manifest.format != FORMAT || manifest.created_at.is_some());
            Ok(manifest)
        })()
        .map_err(|_| ApiError::invalid("Invalid Neondojo backup manifest"))?;
        if !matches!(manifest.format, LEGACY_FORMAT | FORMAT) {
            return Err(ApiError::invalid("Unsupported Neondojo backup format"));
        }
        let compatible = manifest.schema == fingerprint;
        if compatible && archive.len() != TABLES.len() + 1 {
            return Err(ApiError::invalid(
                "Backup has a missing or unexpected entry",
            ));
        }

        if !compatible && !summarize {
            return Err(super::schema_mismatch());
        }
        let tables: &[&str] = if compatible {
            TABLES
        } else if SUMMARY_TABLES
            .iter()
            .all(|table| archive.index_for_name(&entry(table)).is_some())
        {
            &SUMMARY_TABLES
        } else {
            &[]
        };
        let mut counts = [0; SUMMARY_TABLES.len()];
        let mut matches = MatchCounter::new();
        let mut output = destination().context("Creating backup scan destination")?;
        let mut lengths = Vec::with_capacity(tables.len());
        let mut total = 0;
        let mut buffer = vec![0; 128 * 1024];
        for &table in tables {
            let mut entry = archive
                .by_name(&entry(table))
                .map_err(|_| ApiError::invalid("Backup table is missing"))?;
            if entry.is_dir() || entry.size() > MAX_DATA_BYTES - total {
                return Err(ApiError::invalid("Invalid or oversized backup table"));
            }
            let count = if summarize {
                SUMMARY_TABLES.iter().position(|&name| name == table)
            } else {
                None
            };
            let mut read = 0;
            loop {
                if cancel.is_cancelled() {
                    return Err(ApiError::invalid("Backup scan was interrupted"));
                }
                let size = entry
                    .read(&mut buffer)
                    .map_err(|_| ApiError::invalid("Backup table is damaged"))?;
                if size == 0 {
                    break;
                }
                read += size as u64;
                if read > entry.size() {
                    return Err(ApiError::invalid("Invalid backup table length"));
                }
                let bytes = &buffer[..size];
                match count {
                    Some(0) => matches.count(bytes),
                    Some(index) => {
                        // PostgreSQL text COPY escapes field newlines as \n.
                        counts[index] += bytes.iter().filter(|&&byte| byte == b'\n').count() as u64;
                    }
                    None => {}
                }
                output
                    .write_all(bytes)
                    .context("Writing decoded backup table")?;
            }
            if read != entry.size() {
                return Err(ApiError::invalid("Incomplete backup table"));
            }
            lengths.push(read);
            total += read;
        }
        let [_, saved_positions, learning_cards] = counts;
        Ok(Scanned {
            checked: Checked {
                contents: (summarize && !tables.is_empty()).then_some(BackupContents {
                    created_at: manifest.created_at,
                    matches: matches.matches,
                    saved_positions,
                    learning_cards,
                }),
                manifest,
                compatible,
            },
            lengths,
            output,
        })
    })
    .await
    .context("Scanning backup archive")?
}

pub(super) async fn check(
    file: File,
    fingerprint: String,
    permit: Arc<OwnedSemaphorePermit>,
) -> ApiResult<Checked> {
    Ok(scan(file, fingerprint, || Ok(io::sink()), permit, true)
        .await?
        .checked)
}

pub(super) async fn prepare(
    file: File,
    fingerprint: String,
    permit: Arc<OwnedSemaphorePermit>,
) -> ApiResult<Prepared> {
    let Scanned {
        checked,
        lengths,
        output,
    } = scan(file, fingerprint, tempfile::tempfile, permit, false).await?;
    let mut file = File::from_std(output);
    tokio::io::AsyncSeekExt::rewind(&mut file)
        .await
        .context("Rewinding restore file")?;
    Ok(Prepared {
        file,
        manifest: checked.manifest,
        lengths,
    })
}
