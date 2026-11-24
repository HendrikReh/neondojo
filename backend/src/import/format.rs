//! The match file formats NeonDojo imports. A new format is added here, in its
//! own module, in the migrations' format check and in the shared configuration
//! options; the matches below then name every property it must answer.

use super::match_context::MatchContext;
use crate::{engine::Engine, model::ImportWarning};
use anyhow::{Context, Result};
use std::future::Future;

/// The stored format of a match. Its text value is the one in the database,
/// the API and, except for `.txt` transcripts, the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub(crate) enum MatchFormat {
    Mat,
    Sgf,
    Xg,
    Bgf,
    Ogxm,
}

/// Canonical SGF of a match file and the irregularities its import resolved.
#[derive(Debug, Clone)]
pub struct ConvertedMatch {
    pub sgf: String,
    pub warnings: Vec<ImportWarning>,
}

/// A file extension and the format its files are read and stored as. A `.txt`
/// upload is a plain-text match transcript in the Jellyfish MAT layout, as
/// published by tournament archives.
const EXTENSIONS: [(&str, MatchFormat); 6] = [
    ("mat", MatchFormat::Mat),
    ("txt", MatchFormat::Mat),
    ("sgf", MatchFormat::Sgf),
    ("xg", MatchFormat::Xg),
    ("bgf", MatchFormat::Bgf),
    ("ogxm", MatchFormat::Ogxm),
];

/// Every supported extension with its dot, for messages.
pub(crate) fn supported_extensions() -> String {
    EXTENSIONS
        .map(|(extension, _)| format!(".{extension}"))
        .join(", ")
}

/// Reads something from the bytes of a match file.
type Read<T> = fn(&[u8]) -> Result<T>;

/// Matches whose file can hold evaluations that count as decisions, see
/// [`MatchFormat::supplies_decisions`]. The migrations' partial index
/// `matches_pending_imported_catalog` has the same condition.
macro_rules! supplies_decisions_sql {
    ($match_row:literal) => {
        concat!($match_row, ".format IN ('xg','bgf','sgf')")
    };
}
pub(crate) use supplies_decisions_sql;

impl MatchFormat {
    pub(crate) const ALL: [Self; 5] = [Self::Mat, Self::Sgf, Self::Xg, Self::Bgf, Self::Ogxm];

    /// The format of files with this lowercase extension.
    pub(crate) fn from_extension(extension: &str) -> Option<Self> {
        EXTENSIONS
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, format)| *format)
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Mat => "mat",
            Self::Sgf => "sgf",
            Self::Xg => "xg",
            Self::Bgf => "bgf",
            Self::Ogxm => "ogxm",
        }
    }

    /// Converts an uploaded file to canonical SGF. Only MAT needs GNUbg; the
    /// other conversions run off the async workers. The outer error is a
    /// failure of NeonDojo, the inner one a fault of the file.
    pub(crate) async fn convert(
        self,
        bytes: impl AsRef<[u8]> + Send + 'static,
        gnubg: impl Future<Output = Result<Engine>>,
    ) -> Result<Result<ConvertedMatch>> {
        let read: Read<ConvertedMatch> = match self {
            Self::Mat => return Ok(gnubg.await?.import_mat(bytes.as_ref()).await),
            Self::Sgf => |bytes| {
                let sgf = std::str::from_utf8(bytes).context("SGF must be UTF-8")?;
                Ok(sgf.to_owned().into())
            },
            Self::Xg => super::convert_xg,
            Self::Bgf => |bytes| Ok(super::bgf_to_sgf(bytes)?.into()),
            Self::Ogxm => |bytes| Ok(super::ogxm_to_sgf(bytes)?.into()),
        };
        Ok(tokio::task::spawn_blocking(move || read(bytes.as_ref())).await?)
    }

    /// The message for a file with this extension that [`Self::convert`] rejects.
    pub(crate) fn conversion_error(self, extension: &str, error: &anyhow::Error) -> String {
        match (self, extension) {
            (Self::Sgf, _) => error.to_string(),
            (Self::Mat, "txt") => {
                format!("TXT import failed; expected a MAT match transcript: {error:#}")
            }
            _ => format!("{} import failed: {error:#}", self.id().to_uppercase()),
        }
    }

    /// Reads the canonical SGF again from the stored original. The stored SGF of
    /// an older XG or BGF import can lack analyses and outcomes that the
    /// original holds; the SGF of every other format is complete.
    pub(crate) fn rereader(self) -> Option<Read<String>> {
        match self {
            Self::Xg => Some(super::xg_to_sgf),
            Self::Bgf => Some(super::bgf_to_sgf),
            Self::Mat | Self::Sgf | Self::Ogxm => None,
        }
    }

    /// Date, event, round and place recorded in the original file.
    pub(crate) fn match_context(self, original: &[u8]) -> Result<MatchContext> {
        match self {
            Self::Xg => super::xg::read_match_context(original),
            Self::Bgf => super::bgf::read_match_context(original),
            Self::Mat | Self::Sgf | Self::Ogxm => Ok(MatchContext::default()),
        }
    }

    pub(crate) fn beaver_rule(self) -> BeaverRule {
        match self {
            // These public importers reject the beaver rule, including its RU form.
            Self::Sgf | Self::Bgf | Self::Ogxm => BeaverRule::Off,
            Self::Xg => BeaverRule::InOriginal(super::xg_beaver_rule),
            // MAT has no reliable money-session beaver declaration.
            Self::Mat => BeaverRule::Unknown,
        }
    }

    /// Whether an imported evaluation of this kind counts as a decision of the
    /// match: its equities and units are known for this format.
    pub(crate) fn supplies_decisions(self, kind: &str) -> bool {
        match self {
            Self::Xg | Self::Bgf => true,
            Self::Sgf => kind == "checker",
            Self::Mat | Self::Ogxm => false,
        }
    }
}

/// Where a money session's beaver rule is recorded.
pub(crate) enum BeaverRule {
    Off,
    InOriginal(Read<bool>),
    Unknown,
}

impl From<String> for ConvertedMatch {
    fn from(sgf: String) -> Self {
        Self {
            sgf,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formats a list like `IN ('xg', 'bgf')` names.
    fn named(sql: &str) -> Vec<&str> {
        let mut named: Vec<_> = sql
            .split(['\'', '"'])
            .filter(|part| MatchFormat::ALL.iter().any(|format| format.id() == *part))
            .collect();
        named.sort_unstable();
        named
    }

    fn ids(keep: impl Fn(MatchFormat) -> bool) -> Vec<&'static str> {
        let mut ids: Vec<_> = MatchFormat::ALL
            .into_iter()
            .filter(|format| keep(*format))
            .map(MatchFormat::id)
            .collect();
        ids.sort_unstable();
        ids
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_database_accepts_and_indexes_the_same_formats(pool: sqlx::PgPool) {
        let check: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conname='matches_format_check'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(named(&check), ids(|_| true));
        let decisions =
            ids(|format| format.supplies_decisions("checker") || format.supplies_decisions("cube"));
        assert_eq!(named(supplies_decisions_sql!("m")), decisions);
        let index: String = sqlx::query_scalar(
            "SELECT pg_get_indexdef('matches_pending_imported_catalog'::regclass)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(named(&index), decisions);
        for format in MatchFormat::ALL {
            let stored: MatchFormat = sqlx::query_scalar("SELECT $1::text")
                .bind(format.id())
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(stored, format);
        }
    }

    #[test]
    fn extensions_are_unique_and_shared_with_the_web_client() {
        let shared: Vec<String> =
            serde_json::from_value(crate::configuration::OPTIONS["match_file_extensions"].clone())
                .unwrap();
        assert_eq!(shared.join(", "), supported_extensions());
        for (index, (extension, format)) in EXTENSIONS.iter().enumerate() {
            assert!(
                !EXTENSIONS[..index]
                    .iter()
                    .any(|(other, _)| other == extension)
            );
            assert_eq!(MatchFormat::from_extension(extension), Some(*format));
        }
        for format in MatchFormat::ALL {
            assert_eq!(MatchFormat::from_extension(format.id()), Some(format));
        }
        for unknown in ["", "pdf", "MAT", ".mat"] {
            assert_eq!(MatchFormat::from_extension(unknown), None);
        }
    }

    #[tokio::test]
    async fn rejected_files_name_their_format() {
        for (format, extension, expected) in [
            (MatchFormat::Sgf, "sgf", "SGF must be UTF-8"),
            (MatchFormat::Xg, "xg", "XG import failed: "),
            (MatchFormat::Bgf, "bgf", "BGF import failed: "),
            (MatchFormat::Ogxm, "ogxm", "OGXM import failed: "),
        ] {
            let error = format
                .convert(b"\xff\xfe".as_slice(), async {
                    panic!("only MAT needs GNUbg")
                })
                .await
                .unwrap()
                .unwrap_err();
            let message = format.conversion_error(extension, &error);
            assert!(message.starts_with(expected), "{message}");
            assert!(message.len() > expected.len() || format == MatchFormat::Sgf);
        }
        let error = anyhow::anyhow!("reason");
        assert_eq!(
            MatchFormat::Mat.conversion_error("mat", &error),
            "MAT import failed: reason"
        );
        assert_eq!(
            MatchFormat::Mat.conversion_error("txt", &error),
            "TXT import failed; expected a MAT match transcript: reason"
        );
    }
}
