//! Little-endian container and record framing (OGXM v2, sections 2 and 3).
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub(super) struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub(super) fn remaining(&self) -> &'a [u8] {
        self.0
    }

    pub(super) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let (prefix, rest) = self.0.split_at_checked(n).context("Truncated OGXM data")?;
        self.0 = rest;
        Ok(prefix)
    }

    pub(super) fn byte(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().context("Truncated OGXM field")
    }

    pub(super) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(super) fn var(&mut self) -> Result<u32> {
        self.leb(5)?.try_into().context("OGXM integer overflow")
    }

    pub(super) fn leb(&mut self, bytes: usize) -> Result<u64> {
        let mut value = 0;
        for i in 0..bytes {
            let byte = self.byte()?;
            let low = byte & 127;
            ensure!(i < 9 || low <= 1, "OGXM integer overflow");
            value |= u64::from(low) << (i * 7);
            if byte < 128 {
                ensure!(i == 0 || low != 0, "Noncanonical OGXM integer");
                return Ok(value);
            }
        }
        anyhow::bail!("OGXM integer overflow")
    }

    pub(super) fn record(&mut self) -> Result<(u64, Self)> {
        let n = self.var()? as usize;
        let mut body = Self::new(self.take(n)?);
        let mask = body.leb(10)?;
        Ok((mask, body))
    }

    pub(super) fn string(&mut self) -> Result<&'a str> {
        let n = self.var()? as usize;
        ensure!(n <= 4096, "OGXM string exceeds 4096 bytes");
        let s = std::str::from_utf8(self.take(n)?)?;
        ensure!(!s.contains('\0'), "OGXM string contains a NUL byte");
        Ok(s)
    }

    pub(super) fn finish(&self, mask: u64, known_bits: u32) -> Result<()> {
        ensure!(
            self.0.is_empty() || mask >> known_bits != 0,
            "Unexpected bytes in OGXM record"
        );
        Ok(())
    }
}

pub(super) fn has(mask: u64, bit: u32) -> bool {
    mask & (1 << bit) != 0
}

pub(super) struct Sections<'a> {
    pub(super) metadata: &'a [u8],
    pub(super) games: Vec<&'a [u8]>,
    pub(super) analyses: Vec<(&'a [u8], &'a [u8])>,
}

impl<'a> Sections<'a> {
    pub(super) fn read(input: &'a [u8]) -> Result<Self> {
        ensure!(input.len() <= 16 * 1024 * 1024, "OGXM file exceeds 16 MiB");
        let mut header = Reader::new(input);
        ensure!(&header.array::<4>()? == b"OGXM", "Invalid OGXM signature");
        let major = u16::from_le_bytes(header.array()?);
        let minor = u16::from_le_bytes(header.array()?);
        let minimum = (
            u16::from_le_bytes(header.array()?),
            u16::from_le_bytes(header.array()?),
        );
        ensure!(
            major == 2 && minimum <= (2, 3) && minimum <= (major, minor),
            "Unsupported OGXM version {major}.{minor} (minimum reader {}.{})",
            minimum.0,
            minimum.1
        );
        ensure!(
            header.u32()? as usize == input.len(),
            "Incorrect OGXM file size"
        );
        let body = header
            .remaining()
            .strip_suffix(b"END!")
            .context("Missing OGXM end marker")?;
        let mut r = Reader::new(body);
        let mut metadata = None;
        let mut games = Vec::new();
        let mut analyses = Vec::new();
        let mut pending = None;
        let mut phase = 0;
        let mut last = [0; 4];
        let mut seen = std::collections::HashSet::new();
        let mut signatures = 0;
        for index in 0.. {
            if r.remaining().is_empty() {
                break;
            }
            ensure!(index < 4096, "Too many OGXM sections");
            let offset = input.len() - 4 - r.remaining().len();
            let tag = r.array::<4>()?;
            let n = r.u32()? as usize;
            let flags = r.byte()?;
            let payload = r.take(n)?;
            ensure!(flags <= 1, "Invalid OGXM section flags");
            let critical = flags == 1;
            ensure!(
                pending.is_none() || &tag == b"DECS",
                "OGXM analysis missing its DECS section"
            );
            match &tag {
                b"MTCH" => {
                    ensure!(
                        phase == 0 && metadata.is_none() && critical,
                        "Invalid OGXM MTCH section"
                    );
                    metadata = Some(payload);
                }
                b"GAME" => {
                    ensure!(
                        phase <= 1 && metadata.is_some() && critical && games.len() < 1000,
                        "Invalid OGXM GAME section"
                    );
                    phase = 1;
                    games.push(payload);
                }
                b"ANAL" => {
                    ensure!(
                        phase <= 2 && !games.is_empty() && !critical && analyses.len() < 64,
                        "OGXM must contain a match; standalone analysis is unsupported"
                    );
                    phase = 2;
                    pending = Some(payload);
                }
                b"DECS" => {
                    ensure!(!critical, "Invalid OGXM DECS flags");
                    analyses.push((pending.take().context("OGXM DECS without ANAL")?, payload));
                }
                b"SIGN" => {
                    ensure!(
                        phase == 2 && &last == b"DECS" && !critical,
                        "Misplaced OGXM SIGN section"
                    );
                }
                b"CLCK" | b"VIDO" | b"ANNO" => {
                    ensure!(
                        phase <= 3
                            && !critical
                            && seen.insert(tag)
                            && (&tag != b"CLCK" || !seen.contains(b"VIDO")),
                        "Misplaced or duplicate OGXM ancillary section"
                    );
                    phase = 3;
                }
                b"MSIG" => {
                    ensure!(
                        !games.is_empty() && !critical && signatures < 16,
                        "Invalid OGXM MSIG section"
                    );
                    phase = 4;
                    signatures += 1;
                }
                b"CSUM" => {
                    ensure!(
                        critical && r.remaining().is_empty(),
                        "OGXM checksum must be last"
                    );
                    verify_checksum(payload, &input[..offset])?;
                }
                _ => ensure!(!critical, "Unsupported critical OGXM section"),
            }
            last = tag;
        }
        ensure!(
            pending.is_none() && !games.is_empty(),
            "OGXM contains no complete match data"
        );
        Ok(Self {
            metadata: metadata.context("Missing OGXM match metadata")?,
            games,
            analyses,
        })
    }

    pub(super) fn match_digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        for payload in std::iter::once(self.metadata).chain(self.games.iter().copied()) {
            hash.update((payload.len() as u32).to_le_bytes());
            hash.update(payload);
        }
        hash.finalize().into()
    }
}

fn verify_checksum(payload: &[u8], covered: &[u8]) -> Result<()> {
    let mut r = Reader::new(payload);
    let (mask, mut body) = r.record()?;
    r.finish(0, 0)?;
    let algorithm = body.var()?;
    let len = body.var()? as usize;
    let digest = body.take(len)?;
    body.finish(mask, 0)?;
    let matches = match algorithm {
        0 => {
            let mut crc = flate2::Crc::new();
            crc.update(covered);
            digest == crc.sum().to_le_bytes()
        }
        3 => digest == &Sha256::digest(covered)[..],
        _ => true, // §8.5 permits skipping unknown algorithms.
    };
    ensure!(matches, "OGXM checksum mismatch");
    Ok(())
}
