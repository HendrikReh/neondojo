//! Decoder for the public XG2 RichGame/ZLIBArchive container and TSaveRec.
//! Format references and fixture provenance live in tests/fixtures/README.md.
mod analysis;
mod container;
mod convert;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use container::main_records;
pub(crate) use container::xg_beaver_rule;
pub(super) use container::{
    ARCHIVE_FOOTER_BYTES, MATCH_MAGIC, RECORD_BYTES, REGISTRY_ENTRY_BYTES, RICH_HEADER_BYTES,
    crc32, read_match_context,
};
pub use convert::{convert_xg, xg_to_sgf};
