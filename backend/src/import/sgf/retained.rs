use anyhow::{Result, ensure};

/// Compact, bounded transport for foreign native evaluations. The source
/// artifact itself remains separately stored byte for byte.
pub(crate) fn encode_retained_analysis(value: &serde_json::Value) -> Result<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "Retained analysis exceeds 1 MiB limit"
    );
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&bytes)?;
    Ok(format!(
        "NEONDOJO-ZLIB:{}",
        STANDARD_NO_PAD.encode(encoder.finish()?)
    ))
}

pub(crate) fn decode_retained_analysis(value: &str) -> Result<serde_json::Value> {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    use std::io::Read;
    if let Some(value) = value.strip_prefix("NEONDOJO-ZLIB:") {
        let bytes = STANDARD_NO_PAD.decode(value)?;
        let mut decoder = flate2::read::ZlibDecoder::new(bytes.as_slice());
        let mut output = Vec::new();
        (&mut decoder)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut output)?;
        ensure!(
            output.len() <= 1024 * 1024,
            "Retained analysis exceeds 1 MiB limit"
        );
        ensure!(
            decoder.total_in() as usize == bytes.len(),
            "Trailing retained analysis data"
        );
        Ok(serde_json::from_slice(&output)?)
    } else {
        Ok(serde_json::from_str(value)?)
    }
}
