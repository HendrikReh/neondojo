use super::{
    MAX_ARCHIVE_BYTES, MAX_DATA_BYTES, Manifest, TABLES, columns, relation, schema, sequences,
    transfer_settings,
};
use crate::api::{ApiError, ApiResult};
use anyhow::{Context, Result, ensure};
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode, header},
};
use futures_util::StreamExt;
use sqlx::{Acquire, PgPool};
use std::io::{Read, Seek, Write};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::mpsc::{self, Sender},
    time::{Duration, timeout},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MANIFEST: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 8192;
const FORMAT: u32 = 1;

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

async fn stream_snapshot(pool: &PgPool, outgoing: Sender<ExportEntry>) -> Result<()> {
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
        }))
        .await?;
    Ok(())
}

pub(super) async fn export(pool: &PgPool) -> Result<(File, u64)> {
    let (sender, receiver) = mpsc::channel(16);
    let writer = tokio::task::spawn_blocking(move || zip_archive(receiver));
    let snapshot = stream_snapshot(pool, sender).await;
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

pub(super) async fn prepare(file: File, fingerprint: String) -> ApiResult<Prepared> {
    let file = file.into_std().await;
    tokio::task::spawn_blocking(move || {
        let mut archive = ZipArchive::new(file)
            .map_err(|_| ApiError::invalid("Invalid BG Tutor backup archive"))?;
        let manifest = (|| -> Result<Manifest> {
            let mut entry = archive.by_name(MANIFEST)?;
            ensure!(entry.size() <= MAX_MANIFEST_BYTES);
            let mut bytes = Vec::new();
            (&mut entry)
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_MANIFEST_BYTES);
            Ok(serde_json::from_slice(&bytes)?)
        })()
        .map_err(|_| ApiError::invalid("Invalid BG Tutor backup manifest"))?;
        if manifest.format != FORMAT || manifest.schema != fingerprint {
            return Err(ApiError::invalid(
                "Backup schema differs from this BG Tutor installation",
            ));
        }
        if archive.len() != TABLES.len() + 1 {
            return Err(ApiError::invalid(
                "Backup has a missing or unexpected entry",
            ));
        }

        let mut staging = tempfile::tempfile().context("Creating temporary restore file")?;
        let mut lengths = Vec::with_capacity(TABLES.len());
        let mut total = 0;
        let mut buffer = vec![0; 128 * 1024];
        for table in TABLES {
            let mut entry = archive
                .by_name(&entry(table))
                .map_err(|_| ApiError::invalid("Backup table is missing"))?;
            if entry.is_dir() || entry.size() > MAX_DATA_BYTES - total {
                return Err(ApiError::invalid("Invalid or oversized backup table"));
            }
            let mut read = 0;
            loop {
                let count = entry
                    .read(&mut buffer)
                    .map_err(|_| ApiError::invalid("Backup table is damaged"))?;
                if count == 0 {
                    break;
                }
                read += count as u64;
                if read > entry.size() {
                    return Err(ApiError::invalid("Invalid backup table length"));
                }
                staging
                    .write_all(&buffer[..count])
                    .context("Staging backup table")?;
            }
            if read != entry.size() {
                return Err(ApiError::invalid("Incomplete backup table"));
            }
            lengths.push(read);
            total += read;
        }
        staging.rewind().context("Rewinding restore file")?;
        Ok(Prepared {
            file: File::from_std(staging),
            manifest,
            lengths,
        })
    })
    .await
    .context("Preparing backup archive")?
}
