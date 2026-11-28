//! NeonDojo EN/DE localization catalog, ICU message formatting, structured
//! re-translatable errors, and locale-aware formatters.

pub mod catalog;
pub mod format;
pub mod icu;

pub use catalog::{GroupCatalog, Locale, LocaleCatalog, catalog, group_names, lookup};
pub use format::{
    CheckerPositionRef, DecomposedDuration, DurationUnit, canonical_decimal,
    compact_checker_notation, compare_locale_base, decompose_duration, format_benchmark_duration,
    format_benchmark_ms, format_date_medium, format_date_medium_local, format_date_medium_utc,
    format_date_short, format_date_short_local, format_date_short_utc, format_datetime_medium,
    format_datetime_medium_local, format_datetime_medium_utc, format_datetime_medium_with_seconds,
    format_datetime_short, format_datetime_short_local, format_datetime_short_utc, format_decimal,
    format_equity, format_equity_digits, format_int, format_list, format_localized_decimal,
    format_naive_date_medium, format_number_default, format_percent, format_ply, format_pr,
    format_signed_decimal, format_signed_diff, format_uint, format_unit_list, localize_decision,
    localize_decision_in_position, parse_localized_decimal, stepped_decimal,
};
pub use icu::{
    IcuError, Param, PluralCategory, extract_placeholders, extract_plural_params, format_template,
    plural_category, plural_category_f64, validate_plural_forms,
};

use serde::{Deserialize, Serialize};
use std::fmt;

/// Entry point for NeonDojo catalog lookups and ICU formatting.
pub struct I18n;

impl I18n {
    /// Returns the raw localized message for `(locale, group, key)`.
    ///
    /// Unknown keys return `key` unchanged so missing strings degrade safely in UI.
    #[must_use]
    pub fn t<'a>(locale: Locale, group: &str, key: &'a str) -> &'a str {
        lookup(locale, group, key).unwrap_or(key)
    }

    /// Strictly looks up `(locale, group, key)` without falling back to `key`.
    #[must_use]
    pub fn try_t(locale: Locale, group: &str, key: &str) -> Option<&'static str> {
        lookup(locale, group, key)
    }

    /// Looks up `(locale, group, key)` and evaluates ICU placeholders & plurals.
    ///
    /// Unknown keys return `key`; formatting errors fall back to the raw template.
    #[must_use]
    pub fn format(locale: Locale, group: &str, key: &str, params: &[(&str, Param)]) -> String {
        let Some(template) = lookup(locale, group, key) else {
            return key.to_owned();
        };
        format_template(locale, template, params).unwrap_or_else(|_| template.to_owned())
    }

    /// Strict variant of [`I18n::format`] returning an [`IcuError`] on missing
    /// parameters or malformed plural syntax (and `Ok(key.to_owned())` if the
    /// key itself is absent).
    pub fn try_format(
        locale: Locale,
        group: &str,
        key: &str,
        params: &[(&str, Param)],
    ) -> Result<String, IcuError> {
        let Some(template) = lookup(locale, group, key) else {
            return Ok(key.to_owned());
        };
        format_template(locale, template, params)
    }

    /// Formats a structured [`AppError`] in `locale`.
    #[must_use]
    pub fn format_error(locale: Locale, error: &AppError) -> String {
        error.format(locale)
    }

    /// Formats a point count (`"1 point"` / `"7 points"` / `"7 Punkte"`).
    #[must_use]
    pub fn format_points(locale: Locale, count: i64) -> String {
        Self::format(
            locale,
            "app",
            "board.points",
            &[("count", Param::from(count))],
        )
    }
}

/// Structured UI error that stores its translation key and parameters so open
/// error banners re-translate live when the user switches languages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppError {
    pub group: String,
    pub key: String,
    pub params: Vec<(String, Param)>,
    pub diagnostic: String,
}

impl AppError {
    /// Creates an error with no parameters or diagnostic suffix.
    #[must_use]
    pub fn new(group: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            group: group.into(),
            key: key.into(),
            params: Vec::new(),
            diagnostic: String::new(),
        }
    }

    /// Shorthand for errors in the `"app"` message group (e.g. `"error.request"`).
    #[must_use]
    pub fn app(key: impl Into<String>) -> Self {
        Self::new("app", key)
    }

    /// Creates an error with typed interpolation parameters.
    #[must_use]
    pub fn with_params(
        group: impl Into<String>,
        key: impl Into<String>,
        params: impl IntoIterator<Item = (impl Into<String>, Param)>,
    ) -> Self {
        Self {
            group: group.into(),
            key: key.into(),
            params: params
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
            diagnostic: String::new(),
        }
    }

    /// Attaches native diagnostic text (appended verbatim after the localized message).
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = diagnostic.into();
        self
    }

    /// Creates a generic localized error wrapping a raw diagnostic message.
    #[must_use]
    pub fn generic(diagnostic: impl Into<String>) -> Self {
        Self::app("error.generic").with_diagnostic(diagnostic)
    }

    /// Renders this error in `locale`, preserving any native diagnostic suffix.
    #[must_use]
    pub fn format(&self, locale: Locale) -> String {
        let borrowed: Vec<(&str, Param)> = self
            .params
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        let message = I18n::format(locale, &self.group, &self.key, &borrowed);
        if self.diagnostic.is_empty() {
            message
        } else {
            format!("{message} {}", self.diagnostic)
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format(Locale::DEFAULT))
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::cmp::Ordering;
    use std::collections::{BTreeMap, BTreeSet};

    // -----------------------------------------------------------------------
    // Ported from messages.test.ts: full catalog integrity (44 groups, 2,088 keys)
    // -----------------------------------------------------------------------

    #[test]
    fn catalogs_have_identical_44_groups_and_2100_keys() {
        let en = catalog(Locale::En);
        let de = catalog(Locale::De);
        assert_eq!(en.len(), 44);
        assert_eq!(de.len(), 44);
        assert_eq!(group_names(Locale::En), group_names(Locale::De));

        let en_total: usize = en.values().map(BTreeMap::len).sum();
        let de_total: usize = de.values().map(BTreeMap::len).sum();
        assert_eq!(en_total, 2100);
        assert_eq!(de_total, 2100);

        for (group, en_entries) in en {
            let de_entries = de
                .get(group)
                .unwrap_or_else(|| panic!("missing group `{group}` in de"));
            let en_keys: Vec<&str> = en_entries.keys().map(String::as_str).collect();
            let de_keys: Vec<&str> = de_entries.keys().map(String::as_str).collect();
            assert_eq!(en_keys, de_keys, "key mismatch in group `{group}`");
        }
    }

    #[test]
    fn every_message_is_non_empty_has_matching_placeholders_valid_plurals_and_formats() {
        let en = catalog(Locale::En);
        let counts = [0_i64, 1, 2, 5, 22];

        for (group, en_entries) in en {
            for (key, en_template) in en_entries {
                let en_placeholders = extract_placeholders(en_template)
                    .unwrap_or_else(|err| panic!("en {group}.{key} placeholders failed: {err}"));

                for locale in Locale::LANGUAGES {
                    let template = lookup(locale, group, key)
                        .unwrap_or_else(|| panic!("missing {locale} {group}.{key}"));
                    assert!(
                        !template.trim().is_empty(),
                        "empty message at {locale} {group}.{key}"
                    );

                    let loc_placeholders = extract_placeholders(template).unwrap_or_else(|err| {
                        panic!("{locale} {group}.{key} placeholders failed: {err}")
                    });
                    assert_eq!(
                        loc_placeholders, en_placeholders,
                        "placeholder mismatch at {locale} {group}.{key}"
                    );

                    validate_plural_forms(locale, template).unwrap_or_else(|err| {
                        panic!("invalid plural block at {locale} {group}.{key}: {err}")
                    });

                    let plural_names: BTreeSet<String> = extract_plural_params(template)
                        .unwrap()
                        .into_iter()
                        .collect();

                    for count in counts {
                        let owned_params: Vec<(String, Param)> = loc_placeholders
                            .iter()
                            .map(|name| {
                                let value = if plural_names.contains(name) {
                                    Param::Int(count)
                                } else {
                                    Param::Text(format!("‹{name}›"))
                                };
                                (name.clone(), value)
                            })
                            .collect();
                        let borrowed: Vec<(&str, Param)> = owned_params
                            .iter()
                            .map(|(n, v)| (n.as_str(), v.clone()))
                            .collect();
                        let rendered = I18n::try_format(locale, group, key, &borrowed)
                            .unwrap_or_else(|err| {
                                panic!(
                                    "failed formatting {locale} {group}.{key} with count={count}: {err}"
                                )
                            });
                        assert!(
                            !rendered.trim().is_empty(),
                            "empty render at {locale} {group}.{key}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_key_degrades_safely() {
        assert_eq!(
            I18n::t(Locale::De, "workspace", "settings"),
            "Einstellungen"
        );
        assert_eq!(I18n::t(Locale::De, "workspace", "unexpected"), "unexpected");
        assert_eq!(I18n::t(Locale::En, "no-such-group", "foo"), "foo");
        assert_eq!(I18n::try_t(Locale::De, "workspace", "unexpected"), None);
    }

    // -----------------------------------------------------------------------
    // Ported from i18n.test.ts: Locale selection, interpolation, plurals, errors
    // -----------------------------------------------------------------------

    #[test]
    fn locale_parsing_and_language_order() {
        assert_eq!(Locale::default(), Locale::En);
        assert_eq!(Locale::parse("de"), Locale::De);
        assert_eq!(Locale::parse("en"), Locale::En);
        for unsupported in ["es", "fr", "it", "pl", "pt-BR", ""] {
            assert_eq!(Locale::parse(unsupported), Locale::En);
            assert_eq!(Locale::try_parse(unsupported), None);
        }
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::De.code(), "de");
        let names: Vec<&str> = Locale::LANGUAGES.iter().map(|l| l.display_name()).collect();
        assert_eq!(names, vec!["Deutsch", "English"]);
    }

    #[test]
    fn translates_board_descriptions_and_preserves_names() {
        assert_eq!(I18n::format_points(Locale::En, 7), "7 points");
        assert_eq!(
            I18n::format(
                Locale::De,
                "app",
                "board.pipsOff",
                &[("pips", 123.into()), ("off", 2.into())]
            ),
            "123 Pips · 2 aus"
        );
        assert_eq!(
            I18n::format(
                Locale::De,
                "app",
                "board.cube",
                &[("value", 4.into()), ("owner", "Élodie".into())]
            ),
            "Doppler 4, Élodie"
        );
        let label = I18n::format(
            Locale::De,
            "app",
            "board.label",
            &[
                ("player", "Zoë".into()),
                ("first", 3.into()),
                ("second", 2.into()),
                ("white", 100.into()),
                ("black", 101.into()),
            ],
        );
        assert!(label.contains("Zoë"));
        assert_eq!(
            I18n::t(Locale::De, "app", "error.fileSize"),
            "Die Datei darf höchstens 16 MiB groß sein."
        );
    }

    #[test]
    fn interpolation_errors_and_non_recursive_substitution() {
        assert_eq!(
            format_template(
                Locale::En,
                "Hello {player}, roll {dice}",
                &[("player", "Ada".into())]
            ),
            Err(IcuError::MissingParam {
                name: "dice".to_owned()
            })
        );
        assert_eq!(
            format_template(
                Locale::En,
                "{player} / {player}",
                &[("player", "{dice}".into())]
            )
            .unwrap(),
            "{dice} / {dice}"
        );
        assert_eq!(
            format_template(Locale::En, "{count} points", &[("count", 0.into())]).unwrap(),
            "0 points"
        );
    }

    #[test]
    fn plural_rules_multi_plurals_nested_placeholders_and_validation() {
        let en_tpl = "{count, plural, one {# card} other {# cards}}";
        let de_tpl = "{count, plural, one {# Karte} other {# Karten}}";
        let counts = [0, 1, 2, 5, 22];

        let en_out: Vec<String> = counts
            .iter()
            .map(|&c| format_template(Locale::En, en_tpl, &[("count", c.into())]).unwrap())
            .collect();
        assert_eq!(
            en_out,
            vec!["0 cards", "1 card", "2 cards", "5 cards", "22 cards"]
        );

        let de_out: Vec<String> = counts
            .iter()
            .map(|&c| format_template(Locale::De, de_tpl, &[("count", c.into())]).unwrap())
            .collect();
        assert_eq!(
            de_out,
            vec!["0 Karten", "1 Karte", "2 Karten", "5 Karten", "22 Karten"]
        );

        let categories: Vec<PluralCategory> = counts
            .iter()
            .map(|&c| plural_category(Locale::De, c))
            .collect();
        assert_eq!(
            categories,
            vec![
                PluralCategory::Other,
                PluralCategory::One,
                PluralCategory::Other,
                PluralCategory::Other,
                PluralCategory::Other,
            ]
        );
        assert_eq!(plural_category_f64(Locale::En, 1.5), PluralCategory::Other);
        assert_eq!(plural_category_f64(Locale::En, 1.0), PluralCategory::One);

        let moved = "{count, plural, one {# Karte} other {# Karten}} nach „{name}“ verschoben.";
        assert_eq!(
            format_template(
                Locale::De,
                moved,
                &[("count", 1.into()), ("name", "Bearoff".into())]
            )
            .unwrap(),
            "1 Karte nach „Bearoff“ verschoben."
        );
        assert_eq!(
            format_template(
                Locale::De,
                moved,
                &[("count", 12345.into()), ("name", "{count}".into())]
            )
            .unwrap(),
            "12.345 Karten nach „{count}“ verschoben."
        );
        assert_eq!(extract_placeholders(moved).unwrap(), vec!["count", "name"]);

        // Nested placeholder inside plural branch + multi-plural message
        let nested = "{outer, plural, one {# item ({tag})} other {# items ({tag})}} · {other, plural, one {# match} other {# matches}}";
        assert_eq!(
            format_template(
                Locale::En,
                nested,
                &[
                    ("outer", 1.into()),
                    ("tag", "cube".into()),
                    ("other", 2000.into())
                ]
            )
            .unwrap(),
            "1 item (cube) · 2,000 matches"
        );

        // Numeric parameter formatted according to locale outside plural
        assert_eq!(
            format_template(
                Locale::En,
                "{matches, plural, one {# match} other {# matches}} of {total}",
                &[("matches", 1.into()), ("total", 2000.into())]
            )
            .unwrap(),
            "1 match of 2,000"
        );
        assert_eq!(I18n::format_points(Locale::De, 12345), "12.345 Punkte");
        assert_eq!(I18n::format_points(Locale::En, 12345), "12,345 points");

        // Reject invalid / incomplete plurals
        assert!(matches!(
            format_template(Locale::En, en_tpl, &[("count", "1".into())]),
            Err(IcuError::NonNumericPluralParam { .. })
        ));
        assert!(matches!(
            validate_plural_forms(Locale::De, "{count, plural, other {# Karten}}"),
            Err(IcuError::MissingPluralForm {
                category: "one",
                ..
            })
        ));
        assert!(matches!(
            validate_plural_forms(Locale::En, "{count, plural, one {# point}}"),
            Err(IcuError::MissingPluralForm {
                category: "other",
                ..
            })
        ));
        assert!(matches!(
            format_template(
                Locale::En,
                "{count, plural, some {x} other {y}}",
                &[("count", 1.into())]
            ),
            Err(IcuError::UnknownPluralCategory { .. })
        ));
        assert!(matches!(
            format_template(Locale::En, "{count, plural, one {#", &[("count", 1.into())]),
            Err(IcuError::UnclosedBrace { .. })
        ));
    }

    #[test]
    fn app_error_retranslates_live_and_preserves_diagnostic() {
        let err = AppError::with_params("app", "error.request", [("status", 503.into())])
            .with_diagnostic("GNUbg unavailable");
        assert_eq!(
            err.format(Locale::De),
            "Anfrage fehlgeschlagen (HTTP 503). GNUbg unavailable"
        );
        assert_eq!(
            err.format(Locale::En),
            "Request failed (HTTP 503). GNUbg unavailable"
        );
        let generic = AppError::generic("timeout");
        assert_eq!(generic.format(Locale::En), "An error occurred. timeout");
        assert_eq!(
            generic.format(Locale::De),
            "Ein Fehler ist aufgetreten. timeout"
        );
    }

    // -----------------------------------------------------------------------
    // Number, Equity, Percent, PR, Signed Diff, Ply, Duration formatters
    // -----------------------------------------------------------------------

    #[test]
    fn formats_numbers_equity_percent_pr_diff_and_ply() {
        assert_eq!(format_int(Locale::En, 12345), "12,345");
        assert_eq!(format_int(Locale::De, 12345), "12.345");
        assert_eq!(format_int(Locale::En, -1_234_567), "-1,234,567");
        assert_eq!(
            format_int(Locale::De, i64::MIN),
            "-9.223.372.036.854.775.808"
        );

        assert_eq!(format_decimal(Locale::En, 0.05, 3, 3), "0.050");
        assert_eq!(format_decimal(Locale::De, 0.05, 3, 3), "0,050");
        assert_eq!(format_decimal(Locale::De, 1234.5, 0, 3), "1.234,5");
        assert_eq!(format_decimal(Locale::En, -0.0001, 3, 3), "0.000");

        assert_eq!(format_equity(Locale::En, 0.214), "+0.214");
        assert_eq!(format_equity(Locale::De, 0.214), "+0,214");
        assert_eq!(format_equity(Locale::En, -0.214), "-0.214");
        assert_eq!(format_equity(Locale::De, -0.214), "-0,214");
        assert_eq!(format_equity(Locale::En, 0.0), "+0.000");
        assert_eq!(format_equity(Locale::De, -0.0), "+0,000");

        assert_eq!(format_percent(Locale::En, 0.54, 0), "54%");
        assert_eq!(format_percent(Locale::De, 0.54, 0), "54 %");
        assert_eq!(format_percent(Locale::En, 0.543, 1), "54.3%");
        assert_eq!(format_percent(Locale::De, 0.543, 1), "54,3 %");

        assert_eq!(format_pr(Locale::En, 5.23), "5.2");
        assert_eq!(format_pr(Locale::De, 5.23), "5,2");

        assert_eq!(format_signed_diff(Locale::En, 5), "+5");
        assert_eq!(format_signed_diff(Locale::En, -5), "-5");
        assert_eq!(format_signed_diff(Locale::En, 0), "0");
        assert_eq!(format_signed_diff(Locale::De, 1234), "+1.234");
        assert_eq!(format_signed_decimal(Locale::En, 0.0, 3, 3), "0.000");
        assert_eq!(format_signed_decimal(Locale::De, 0.125, 3, 3), "+0,125");
        assert_eq!(format_signed_decimal(Locale::De, -0.125, 3, 3), "-0,125");

        assert_eq!(format_ply(Locale::En, 3), "3-ply");
        assert_eq!(format_ply(Locale::De, 3), "3-ply");
    }

    #[test]
    fn formats_benchmark_durations_matching_web_suite() {
        assert_eq!(format_benchmark_duration(Locale::En, 999.5), "999.5 ms");
        assert_eq!(format_benchmark_duration(Locale::En, 1608.53), "1.61s");
        assert_eq!(format_benchmark_duration(Locale::En, 60_000.0), "1m");
        assert_eq!(
            format_benchmark_duration(Locale::En, 1_933_872.5),
            "32m 14s"
        );
        assert_eq!(format_benchmark_duration(Locale::En, 3_599_600.0), "1h");
        assert_eq!(format_benchmark_duration(Locale::En, 3_900_000.0), "1h 5m");
        assert_eq!(format_benchmark_duration(Locale::En, 7_199_000.0), "2h");

        assert_eq!(
            format_benchmark_duration(Locale::De, 20_225.88),
            "20,23 Sek."
        );
        assert_eq!(
            format_benchmark_duration(Locale::De, 1_933_872.5),
            "32 Min. 14 Sek."
        );
        assert_eq!(
            format_benchmark_duration(Locale::De, 3_900_000.0),
            "1h 5 Min."
        );
        assert_eq!(format_benchmark_ms(Locale::De, 12.3), "12,30 ms");
    }

    #[test]
    fn formats_dates_datetimes_and_lists() {
        let dt = Utc.with_ymd_and_hms(2026, 11, 22, 14, 5, 9).unwrap();
        assert_eq!(format_date_short_utc(Locale::En, dt), "11/22/26");
        assert_eq!(format_date_short_utc(Locale::De, dt), "22.11.26");
        assert_eq!(format_date_medium_utc(Locale::En, dt), "Nov 22, 2026");
        assert_eq!(format_date_medium_utc(Locale::De, dt), "22.11.2026");
        assert_eq!(
            format_datetime_short_utc(Locale::En, dt),
            "11/22/26, 2:05 PM"
        );
        assert_eq!(format_datetime_short_utc(Locale::De, dt), "22.11.26, 14:05");
        assert_eq!(
            format_datetime_medium_utc(Locale::En, dt),
            "Nov 22, 2026, 2:05 PM"
        );
        assert_eq!(
            format_datetime_medium_utc(Locale::De, dt),
            "22.11.2026, 14:05"
        );
        assert_eq!(
            format_datetime_medium_with_seconds(Locale::En, dt),
            "Nov 22, 2026, 2:05:09 PM"
        );
        assert_eq!(
            format_datetime_medium_with_seconds(Locale::De, dt),
            "22.11.2026, 14:05:09"
        );

        let midnight = Utc.with_ymd_and_hms(2026, 1, 5, 0, 7, 0).unwrap();
        assert_eq!(
            format_datetime_medium_utc(Locale::En, midnight),
            "Jan 5, 2026, 12:07 AM"
        );
        assert_eq!(
            format_datetime_medium_utc(Locale::De, midnight),
            "05.01.2026, 00:07"
        );

        let naive = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert_eq!(format_naive_date_medium(Locale::De, naive), "22.11.2026");
        assert_eq!(format_naive_date_medium(Locale::En, naive), "Nov 22, 2026");

        // Local helpers return non-empty formatted timestamps matching host zone.
        assert!(!format_date_short_local(Locale::En, dt).is_empty());
        assert!(!format_date_medium_local(Locale::De, dt).is_empty());
        assert!(!format_datetime_short_local(Locale::En, dt).is_empty());
        assert!(!format_datetime_medium_local(Locale::De, dt).is_empty());

        assert_eq!(format_list(Locale::En, &["A", "B", "C"]), "A, B, and C");
        assert_eq!(format_list(Locale::De, &["A", "B", "C"]), "A, B und C");
        assert_eq!(format_list(Locale::En, &["A", "B"]), "A and B");
        assert_eq!(format_list(Locale::De, &["A", "B"]), "A und B");
        assert_eq!(format_list(Locale::En, &["A"]), "A");
        assert_eq!(format_list(Locale::En, &[] as &[&str]), "");

        assert_eq!(
            format_unit_list(Locale::En, &["1-ply", "2-ply", "3-ply"]),
            "1-ply, 2-ply, 3-ply"
        );
        assert_eq!(
            format_unit_list(Locale::De, &["1-ply", "2-ply", "3-ply"]),
            "1-ply, 2-ply und 3-ply"
        );
    }

    #[test]
    fn base_collation_handles_case_umlauts_diacritics_and_eszett() {
        assert_eq!(
            compare_locale_base(Locale::De, "Über", "uber"),
            Ordering::Equal
        );
        assert_eq!(
            compare_locale_base(Locale::De, "Äpfel", "Banane"),
            Ordering::Less
        );
        assert_eq!(
            compare_locale_base(Locale::De, "Straße", "Strasse"),
            Ordering::Equal
        );
        assert_eq!(
            compare_locale_base(Locale::En, "Élodie", "elodie"),
            Ordering::Equal
        );
        assert_eq!(
            compare_locale_base(Locale::En, "Zoë", "zoe"),
            Ordering::Equal
        );
        assert_eq!(
            compare_locale_base(Locale::En, "alpha", "beta"),
            Ordering::Less
        );
    }

    #[test]
    fn localizes_decisions_and_compacts_checker_notation() {
        assert_eq!(localize_decision(Locale::De, "No double"), "Nicht doppeln");
        assert_eq!(localize_decision(Locale::De, "Take"), "Annehmen");
        assert_eq!(localize_decision(Locale::De, "Pass"), "Passen");
        assert_eq!(localize_decision(Locale::De, "Drop"), "Passen");
        assert_eq!(localize_decision(Locale::De, "Cannot move"), "Aussetzen");
        assert_eq!(localize_decision(Locale::De, "dance"), "Aussetzen");
        assert_eq!(localize_decision(Locale::De, "Double"), "Doppeln");
        assert_eq!(localize_decision(Locale::De, "Resign"), "Aufgabe");
        assert_eq!(
            localize_decision(Locale::De, "24/18(2) 13/7*"),
            "24/18(2) 13/7*"
        );

        let cases = [
            ("24/21/15", "24/15"),
            ("24/21* 21/15", "24/21*/15"),
            ("24/21 21/15*", "24/15*"),
            ("bar/24 24/22", "bar/22"),
            ("7/4 4/off", "7/off"),
            ("13/10(2) 10/7(2)", "13/7(2)"),
            ("13/10 8/5 10/7 5/2", "13/7 8/2"),
            ("24/21 24/18", "24/21 24/18"),
            ("24/21 21/15?", "24/21 21/15?"),
        ];
        for (original, expected) in cases {
            assert_eq!(
                localize_decision(Locale::En, original),
                expected,
                "{original}"
            );
        }

        // Standard starting position 6-3 run
        let start = [
            0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2, 0,
        ];
        assert_eq!(
            localize_decision_in_position(
                Locale::De,
                "24/21 21/15",
                Some(CheckerPositionRef {
                    board: &start,
                    player: 0,
                    dice: [6, 3],
                })
            ),
            "24/15"
        );

        // Retains intermediate hit even when source omitted `*`
        let mut hit_board = [0i8; 26];
        hit_board[24] = 1;
        hit_board[21] = -1;
        assert_eq!(
            localize_decision_in_position(
                Locale::De,
                "24/21 21/15",
                Some(CheckerPositionRef {
                    board: &hit_board,
                    player: 0,
                    dice: [6, 3],
                })
            ),
            "24/21*/15"
        );
    }

    #[test]
    fn decimal_input_parsing_formatting_and_stepping() {
        assert_eq!(format_localized_decimal(Locale::De, "0.05"), "0,05");
        assert_eq!(format_localized_decimal(Locale::En, "0.05"), "0.05");
        assert_eq!(canonical_decimal(" 0,05 "), "0.05");
        assert_eq!(canonical_decimal("0.05"), "0.05");
        assert_eq!(canonical_decimal(""), "");

        assert_eq!(parse_localized_decimal(Locale::De, "0,05"), Some(0.05));
        assert_eq!(parse_localized_decimal(Locale::De, "0.05"), Some(0.05));
        assert_eq!(parse_localized_decimal(Locale::En, "0,05"), Some(0.05));
        assert_eq!(parse_localized_decimal(Locale::De, "1.234,5"), Some(1234.5));
        assert_eq!(parse_localized_decimal(Locale::En, "1,234.5"), Some(1234.5));
        assert_eq!(parse_localized_decimal(Locale::En, ""), None);
        assert_eq!(parse_localized_decimal(Locale::En, "NaN"), None);
        assert_eq!(parse_localized_decimal(Locale::En, "inf"), None);
        assert_eq!(parse_localized_decimal(Locale::En, "1.2.3"), None);

        assert_eq!(
            stepped_decimal("0.1", 1, 0.005, None, None).as_deref(),
            Some("0.105")
        );
        assert_eq!(
            stepped_decimal("0.3", -1, 0.1, None, None).as_deref(),
            Some("0.2")
        );
        assert_eq!(
            stepped_decimal("", 1, 0.01, None, None).as_deref(),
            Some("0.01")
        );
        assert_eq!(
            stepped_decimal("0", -1, 0.01, Some(0.0), None).as_deref(),
            Some("0")
        );
        assert_eq!(
            stepped_decimal("0.999", 1, 0.01, None, Some(1.0)).as_deref(),
            Some("1")
        );
        assert_eq!(
            stepped_decimal("0.0123", 1, 0.005, None, None).as_deref(),
            Some("0.0173")
        );
        assert_eq!(stepped_decimal("abc", 1, 0.01, None, None), None);
    }
}
