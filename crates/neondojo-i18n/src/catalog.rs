//! Embedded EN/DE message catalogs and [`Locale`] representation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

/// Supported interface languages for NeonDojo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    En,
    De,
}

impl Locale {
    /// Default application locale when no supported preference is configured.
    pub const DEFAULT: Self = Self::En;

    /// Supported languages ordered alphabetically by their native display name
    /// (`Deutsch`, `English`), matching the web client's language picker.
    pub const LANGUAGES: [Self; 2] = [Self::De, Self::En];

    /// Parses a locale code, falling back to [`Locale::En`] for unknown values.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "de" | "DE" => Self::De,
            _ => Self::En,
        }
    }

    /// Strictly parses an exact supported locale code (`"en"` or `"de"`).
    #[must_use]
    pub fn try_parse(raw: &str) -> Option<Self> {
        match raw {
            "en" => Some(Self::En),
            "de" => Some(Self::De),
            _ => None,
        }
    }

    /// BCP-47 short language code (`"en"` or `"de"`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
        }
    }

    /// Native display name shown in the language selector.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::De => "Deutsch",
        }
    }

    /// Decimal separator character (`'.'` for English, `','` for German).
    #[must_use]
    pub const fn decimal_separator(self) -> char {
        match self {
            Self::En => '.',
            Self::De => ',',
        }
    }

    /// Thousands grouping separator character (`','` for English, `'.'` for German).
    #[must_use]
    pub const fn thousands_separator(self) -> char {
        match self {
            Self::En => ',',
            Self::De => '.',
        }
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Key-to-template map within a single feature message group.
pub type GroupCatalog = BTreeMap<String, String>;

/// Complete group-to-messages catalog for a single [`Locale`].
pub type LocaleCatalog = BTreeMap<String, GroupCatalog>;

static EN_RAW: &str = include_str!("../locales/en.json");
static DE_RAW: &str = include_str!("../locales/de.json");

static EN_CATALOG: LazyLock<LocaleCatalog> = LazyLock::new(|| parse_embedded_catalog("en", EN_RAW));
static DE_CATALOG: LazyLock<LocaleCatalog> = LazyLock::new(|| parse_embedded_catalog("de", DE_RAW));

fn parse_embedded_catalog(code: &str, raw: &str) -> LocaleCatalog {
    serde_json::from_str(raw)
        .unwrap_or_else(|error| panic!("embedded locale catalog `{code}.json` is invalid: {error}"))
}

/// Returns the complete embedded catalog for `locale`.
#[must_use]
pub fn catalog(locale: Locale) -> &'static LocaleCatalog {
    match locale {
        Locale::En => &EN_CATALOG,
        Locale::De => &DE_CATALOG,
    }
}

/// Looks up a single message template by `(locale, group, key)`.
#[must_use]
pub fn lookup(locale: Locale, group: &str, key: &str) -> Option<&'static str> {
    catalog(locale)
        .get(group)
        .and_then(|entries| entries.get(key))
        .map(String::as_str)
}

/// Lists all group names present in `locale`'s catalog in sorted order.
#[must_use]
pub fn group_names(locale: Locale) -> Vec<&'static str> {
    catalog(locale).keys().map(String::as_str).collect()
}
