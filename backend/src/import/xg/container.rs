use crate::import::match_context::MatchContext;
use anyhow::{Context, Result, ensure};
use chrono::{Days, NaiveDate};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Size of one TSaveRec record in the `temp.xg` match stream.
pub(in crate::import) const RECORD_BYTES: usize = 2560;
/// Size of the fixed TRichGameHeader before the thumbnail and archive.
pub(in crate::import) const RICH_HEADER_BYTES: usize = 8232;
/// Size of one TZLBFileRec entry in the archive registry.
pub(in crate::import) const REGISTRY_ENTRY_BYTES: usize = 532;
/// Size of the TZLBArchiveRec footer that ends the archive.
pub(in crate::import) const ARCHIVE_FOOTER_BYTES: usize = 36;
/// TSaveRec match header magic (`MagicNumber`, the bytes `DMLI`).
pub(in crate::import) const MATCH_MAGIC: u32 = 0x494c4d44;
const MAX_FILE: usize = 32 * 1024 * 1024;
pub(super) const MAX_UNCOMPRESSED: usize = 64 * 1024 * 1024;

pub(super) fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .context("XG: truncated integer")?
            .try_into()?,
    ))
}

pub(super) fn i32_at(b: &[u8], p: usize) -> Result<i32> {
    Ok(u32_at(b, p)? as i32)
}

fn usize_at(b: &[u8], p: usize) -> Result<usize> {
    let n = i32_at(b, p)?;
    ensure!(n >= 0, "XG: negative size/offset");
    Ok(n as usize)
}

pub(super) fn bool_at(b: &[u8], p: usize) -> Result<bool> {
    let n = *b.get(p).context("XG: truncated boolean")?;
    ensure!(n <= 1, "XG: invalid boolean at offset {p}");
    Ok(n != 0)
}

pub(super) fn f32_at(b: &[u8], p: usize) -> Result<f64> {
    let value = f32::from_bits(u32_at(b, p)?) as f64;
    ensure!(value.is_finite(), "XG: non-finite evaluation");
    Ok(value)
}

pub(super) fn f64_at(b: &[u8], p: usize) -> Result<f64> {
    let value = f64::from_le_bytes(
        b.get(p..p + 8)
            .context("XG: truncated evaluation")?
            .try_into()?,
    );
    ensure!(value.is_finite(), "XG: non-finite evaluation");
    Ok(value)
}

pub(in crate::import) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn inflate(bytes: &[u8], expected: usize) -> Result<Vec<u8>> {
    ensure!(
        expected <= MAX_UNCOMPRESSED,
        "XG: decompressed data exceeds 64 MiB limit"
    );
    let mut decoder = ZlibDecoder::new(bytes);
    let mut out = Vec::with_capacity(expected.min(1024 * 1024));
    (&mut decoder)
        .take(expected as u64 + 1)
        .read_to_end(&mut out)
        .context("XG: invalid zlib stream")?;
    ensure!(
        out.len() == expected,
        "XG: decompressed size does not match archive registry"
    );
    ensure!(
        decoder.total_in() as usize == bytes.len(),
        "XG: unused bytes after zlib stream"
    );
    Ok(out)
}

pub(in crate::import) fn main_records(input: &[u8]) -> Result<Vec<u8>> {
    ensure!(input.len() <= MAX_FILE, "XG file exceeds 32 MiB limit");
    ensure!(
        input.get(..4) == Some(b"RGMH"),
        "Not an XG RichGame file (missing RGMH signature)"
    );
    ensure!(
        u32_at(input, 4)? == 1,
        "Unsupported XG RichGame header version"
    );
    ensure!(
        u32_at(input, 8)? as usize == RICH_HEADER_BYTES,
        "Unsupported XG RichGame header size"
    );
    // XG writes a zero thumbnail offset; the payload always follows the fixed
    // header and thumbnail, as specified by XG_format.pas.
    let thumbnail = u32_at(input, 20)? as usize;
    let start = RICH_HEADER_BYTES
        .checked_add(thumbnail)
        .context("XG: thumbnail size overflow")?;
    let archive = input.get(start..).context("XG: truncated thumbnail")?;
    ensure!(
        archive.len() >= ARCHIVE_FOOTER_BYTES,
        "XG: missing archive footer"
    );
    let footer_at = archive.len() - ARCHIVE_FOOTER_BYTES;
    let footer = &archive[footer_at..];
    ensure!(
        u32_at(footer, 0)? == crc32(&archive[..footer_at]),
        "XG archive CRC mismatch; file may be damaged"
    );
    let count = usize_at(footer, 4)?;
    ensure!((1..=16).contains(&count), "XG: invalid archive entry count");
    ensure!(u32_at(footer, 8)? == 1, "Unsupported XG archive version");
    let registry_size = usize_at(footer, 12)?;
    let registry_at = usize_at(footer, 16)?;
    ensure!(
        registry_at.checked_add(registry_size) == Some(footer_at),
        "XG: invalid registry bounds"
    );
    let registry_bytes = archive
        .get(registry_at..footer_at)
        .context("XG: registry outside archive")?;
    let registry = if bool_at(footer, 20)? {
        inflate(registry_bytes, count * REGISTRY_ENTRY_BYTES)?
    } else {
        ensure!(
            registry_bytes.len() == count * REGISTRY_ENTRY_BYTES,
            "XG: invalid uncompressed registry length"
        );
        registry_bytes.to_vec()
    };
    let mut main = None;
    let mut ranges = Vec::new();
    for entry in registry.as_chunks::<REGISTRY_ENTRY_BYTES>().0 {
        let name = std::str::from_utf8(&entry[1..=usize::from(entry[0])])
            .context("XG: invalid archive entry name")?
            .to_ascii_lowercase();
        let original_size = usize_at(entry, 512)?;
        let compressed_size = usize_at(entry, 516)?;
        let offset = usize_at(entry, 520)?;
        let end = offset
            .checked_add(compressed_size)
            .context("XG: entry size overflow")?;
        ensure!(end <= registry_at, "XG: entry lies outside archive payload");
        ensure!(
            original_size <= MAX_UNCOMPRESSED,
            "XG: entry exceeds decompression limit"
        );
        ensure!(entry[528] <= 1, "XG: unsupported compression status");
        ensure!(
            !ranges.iter().any(|&(a, b)| offset < b && a < end),
            "XG: overlapping archive entries"
        );
        ranges.push((offset, end));
        if name.ends_with(".xg") {
            ensure!(main.is_none(), "XG: multiple match streams");
            let bytes = &archive[offset..end];
            let decoded = if entry[528] == 0 {
                inflate(bytes, original_size)?
            } else {
                ensure!(
                    bytes.len() == original_size,
                    "XG: stored entry size mismatch"
                );
                bytes.to_vec()
            };
            ensure!(
                crc32(&decoded) == u32_at(entry, 524)?,
                "XG match stream CRC mismatch"
            );
            main = Some(decoded);
        }
    }
    main.context("XG archive contains no .xg match stream")
}

/// Read the declared rule independently of whether any Beaver was played.
/// The played-line import continues to reject unsupported Beaver actions.
pub(crate) fn xg_beaver_rule(input: &[u8]) -> Result<bool> {
    let records = main_records(input)?;
    let header = records
        .get(..RECORD_BYTES)
        .context("XG: truncated match header")?;
    ensure!(header[8] == 0, "XG: match header must be the first record");
    bool_at(header, 102)
}

pub(super) fn header_text(
    record: &[u8],
    unicode_at: usize,
    ansi_at: usize,
    ansi_limit: usize,
) -> Result<String> {
    let words: Vec<u16> = record[unicode_at..unicode_at + 258]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .take_while(|n| *n != 0)
        .collect();
    if !words.is_empty() {
        return String::from_utf16(&words).context("XG: invalid Unicode header text");
    }
    let count = usize::from(record[ansi_at]);
    ensure!(count <= ansi_limit, "XG: invalid short header text length");
    // The ANSI fallback is byte-preserving Latin-1. Current XG2 normally uses
    // the UTF-16 fields, including for names outside the western code page.
    Ok(record[ansi_at + 1..ansi_at + 1 + count]
        .iter()
        .map(|b| char::from(*b))
        .collect())
}

pub(in crate::import) fn read_match_context(input: &[u8]) -> Result<MatchContext> {
    let records = main_records(input)?;
    let header = records
        .get(..RECORD_BYTES)
        .context("XG: truncated match header")?;
    ensure!(header[8] == 0, "XG: match header must be the first record");
    ensure!(
        u32_at(header, 556)? == MATCH_MAGIC,
        "XG: invalid match magic"
    );
    ensure!(
        (24..=40).contains(&i32_at(header, 552)?),
        "Unsupported XG save version"
    );
    Ok(match_context(header))
}

pub(super) fn match_context(header: &[u8]) -> MatchContext {
    // The integral part of Delphi TDateTime counts calendar days since
    // 1899-12-30. Zero is the unset date and the fraction is local time.
    let date = f64_at(header, 128)
        .ok()
        .filter(|days| (1.0..2_958_466.0).contains(days))
        .and_then(|days| {
            NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_days(Days::new(days as u64))
        });
    let event = header_text(header, 622, 136, 128).ok();
    let round = header_text(header, 1654, 417, 128).ok();
    let place = header_text(header, 1396, 283, 128).ok();
    MatchContext::new(date, event.as_deref(), round.as_deref(), place.as_deref())
}
