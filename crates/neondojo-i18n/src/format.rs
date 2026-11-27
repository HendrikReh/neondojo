//! Locale-aware number, duration, date/time, list, collation, decimal input,
//! and backgammon decision formatters matching `web/src/shared/i18n/` and
//! `web/src/shared/format/`.

use crate::catalog::{Locale, lookup};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike, Utc};
use std::cmp::Ordering;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Number, Equity, Percent, PR, Signed Diff, Ply
// ---------------------------------------------------------------------------

fn group_integer_digits(digits: &str, separator: char) -> String {
    let len = digits.len();
    if len <= 3 {
        return digits.to_owned();
    }
    let mut out = String::with_capacity(len + (len - 1) / 3);
    let first = len % 3;
    let mut index = 0usize;
    if first != 0 {
        out.push_str(&digits[..first]);
        index = first;
    }
    while index < len {
        if !out.is_empty() {
            out.push(separator);
        }
        out.push_str(&digits[index..index + 3]);
        index += 3;
    }
    out
}

/// Formats a signed integer with locale thousands grouping (`12,345` / `12.345`).
#[must_use]
pub fn format_int(locale: Locale, value: i64) -> String {
    let grouped = format_uint(locale, value.unsigned_abs());
    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// Formats an unsigned integer with locale thousands grouping (`12,345` / `12.345`).
#[must_use]
pub fn format_uint(locale: Locale, value: u64) -> String {
    let digits = value.to_string();
    group_integer_digits(&digits, locale.thousands_separator())
}

fn format_abs_decimal(locale: Locale, abs: f64, min_digits: usize, max_digits: usize) -> String {
    let max_digits = max_digits.max(min_digits);
    let rounded = format!("{abs:.max_digits$}");
    let (int_part, frac_part) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut frac = frac_part;
    while frac.len() > min_digits && frac.ends_with('0') {
        frac = &frac[..frac.len() - 1];
    }
    let grouped = group_integer_digits(int_part, locale.thousands_separator());
    if frac.is_empty() {
        grouped
    } else {
        format!("{grouped}{}{frac}", locale.decimal_separator())
    }
}

fn rounded_is_zero(abs: f64, max_digits: usize) -> bool {
    let rounded = format!("{abs:.max_digits$}");
    rounded.bytes().all(|b| b == b'0' || b == b'.')
}

/// Formats a floating-point number with `[min_digits, max_digits]` fractional
/// digits and locale-specific grouping and decimal separators.
#[must_use]
pub fn format_decimal(locale: Locale, value: f64, min_digits: usize, max_digits: usize) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-∞".to_owned()
        } else {
            "∞".to_owned()
        };
    }
    let max_digits = max_digits.max(min_digits);
    let abs = value.abs();
    let formatted = format_abs_decimal(locale, abs, min_digits, max_digits);
    if value.is_sign_negative() && !rounded_is_zero(abs, max_digits) {
        format!("-{formatted}")
    } else {
        formatted
    }
}

/// Formats a float with `Intl.NumberFormat(locale)` defaults (up to 3 fractional digits).
#[must_use]
pub fn format_number_default(locale: Locale, value: f64) -> String {
    format_decimal(locale, value, 0, 3)
}

/// Formats a backgammon equity with explicit sign and 3 decimal places
/// (`+0.214` / `+0,214`, `-0.214` / `-0,214`, `+0.000` / `+0,000`).
#[must_use]
pub fn format_equity(locale: Locale, value: f64) -> String {
    format_equity_digits(locale, value, 3)
}

/// Formats a backgammon equity with explicit sign and `digits` decimal places.
#[must_use]
pub fn format_equity_digits(locale: Locale, value: f64, digits: usize) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-∞".to_owned()
        } else {
            "+∞".to_owned()
        };
    }
    let abs = value.abs();
    let formatted = format_abs_decimal(locale, abs, digits, digits);
    if value.is_sign_negative() && !rounded_is_zero(abs, digits) {
        format!("-{formatted}")
    } else {
        format!("+{formatted}")
    }
}

/// Formats a ratio (`0.54` → `"54%"` in English, `"54 %"` in German) with
/// `digits` fixed fractional digits.
#[must_use]
pub fn format_percent(locale: Locale, ratio: f64, digits: usize) -> String {
    let number = format_decimal(locale, ratio * 100.0, digits, digits);
    match locale {
        Locale::En => format!("{number}%"),
        Locale::De => format!("{number} %"),
    }
}

/// Formats a performance rating (PR) with one decimal place (`5.2` / `5,2`),
/// matching `web/src/shared/format/pr-format.ts`.
#[must_use]
pub fn format_pr(locale: Locale, value: f64) -> String {
    format_decimal(locale, value, 1, 1)
}

/// Formats a signed integer difference with `signDisplay: 'exceptZero'`
/// (`+5`, `-5`, `0`), matching backup count diffs.
#[must_use]
pub fn format_signed_diff(locale: Locale, value: i64) -> String {
    match value.cmp(&0) {
        Ordering::Greater => format!("+{}", format_uint(locale, value as u64)),
        Ordering::Less => format!("-{}", format_uint(locale, value.unsigned_abs())),
        Ordering::Equal => "0".to_owned(),
    }
}

/// Formats a signed decimal with `signDisplay: 'exceptZero'`
/// (e.g. engine duel points per game).
#[must_use]
pub fn format_signed_decimal(
    locale: Locale,
    value: f64,
    min_digits: usize,
    max_digits: usize,
) -> String {
    if !value.is_finite() {
        return format_decimal(locale, value, min_digits, max_digits);
    }
    let max_digits = max_digits.max(min_digits);
    let abs = value.abs();
    let formatted = format_abs_decimal(locale, abs, min_digits, max_digits);
    if rounded_is_zero(abs, max_digits) {
        formatted
    } else if value.is_sign_negative() {
        format!("-{formatted}")
    } else {
        format!("+{formatted}")
    }
}

/// Formats a ply depth label consistently across workspaces (`"2-ply"`),
/// matching `web/src/shared/format/ply.ts`.
#[must_use]
pub fn format_ply(locale: Locale, plies: u32) -> String {
    format!("{}-ply", format_uint(locale, u64::from(plies)))
}

// ---------------------------------------------------------------------------
// Benchmark Duration
// ---------------------------------------------------------------------------

/// Units returned by [`decompose_duration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Second,
    Minute,
    Hour,
}

/// Decomposed duration (at most two display units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecomposedDuration {
    pub major: u64,
    pub major_unit: DurationUnit,
    pub minor: u64,
    pub minor_unit: Option<DurationUnit>,
}

/// Decomposes a duration >= 1 second into at most two display units,
/// matching `web/src/shared/format/duration.ts`.
#[must_use]
pub fn decompose_duration(milliseconds: f64) -> DecomposedDuration {
    let clamped = if milliseconds.is_finite() && milliseconds > 0.0 {
        milliseconds
    } else {
        0.0
    };
    let seconds = (clamped / 1000.0).round() as u64;
    if seconds < 60 {
        return DecomposedDuration {
            major: seconds,
            major_unit: DurationUnit::Second,
            minor: 0,
            minor_unit: None,
        };
    }
    if seconds < 3600 {
        return DecomposedDuration {
            major: seconds / 60,
            major_unit: DurationUnit::Minute,
            minor: seconds % 60,
            minor_unit: Some(DurationUnit::Second),
        };
    }
    let minutes = (clamped / 60_000.0).round() as u64;
    DecomposedDuration {
        major: minutes / 60,
        major_unit: DurationUnit::Hour,
        minor: minutes % 60,
        minor_unit: Some(DurationUnit::Minute),
    }
}

fn format_narrow_unit(locale: Locale, value: f64, unit: DurationUnit, max_digits: usize) -> String {
    let num = format_decimal(locale, value, 0, max_digits);
    match (locale, unit) {
        (Locale::En, DurationUnit::Second) => format!("{num}s"),
        (Locale::En, DurationUnit::Minute) => format!("{num}m"),
        (Locale::En | Locale::De, DurationUnit::Hour) => format!("{num}h"),
        (Locale::De, DurationUnit::Second) => format!("{num} Sek."),
        (Locale::De, DurationUnit::Minute) => format!("{num} Min."),
    }
}

/// Formats benchmark/engine durations: exact milliseconds below 1 s, seconds
/// below 60 s, and at most two units (`32m 14s`, `1h 5m`, `2h`) above 60 s.
#[must_use]
pub fn format_benchmark_duration(locale: Locale, milliseconds: f64) -> String {
    if milliseconds < 1000.0 {
        return format!("{} ms", format_decimal(locale, milliseconds, 0, 2));
    }
    if milliseconds < 60_000.0 {
        return format_narrow_unit(locale, milliseconds / 1000.0, DurationUnit::Second, 2);
    }
    let parts = decompose_duration(milliseconds);
    let major = format_narrow_unit(locale, parts.major as f64, parts.major_unit, 0);
    match parts.minor_unit {
        Some(minor_unit) if parts.minor > 0 => {
            let minor = format_narrow_unit(locale, parts.minor as f64, minor_unit, 0);
            format!("{major} {minor}")
        }
        _ => major,
    }
}

/// Formats a benchmark timing cell in milliseconds with fixed 2 decimal places
/// (`12.34 ms` / `12,34 ms`), matching `benchmarkTimeDigits`.
#[must_use]
pub fn format_benchmark_ms(locale: Locale, milliseconds: f64) -> String {
    format!("{} ms", format_decimal(locale, milliseconds, 2, 2))
}

// ---------------------------------------------------------------------------
// Date & DateTime (Local + UTC)
// ---------------------------------------------------------------------------

const EN_SHORT_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_en_time_12h(hour: u32, minute: u32, seconds: Option<u32>) -> String {
    let (is_pm, hour12) = match hour {
        0 => (false, 12),
        1..=11 => (false, hour),
        12 => (true, 12),
        _ => (true, hour - 12),
    };
    let period = if is_pm { "PM" } else { "AM" };
    match seconds {
        Some(sec) => format!("{hour12}:{minute:02}:{sec:02} {period}"),
        None => format!("{hour12}:{minute:02} {period}"),
    }
}

fn format_date_short_parts(locale: Locale, year: i32, month: u32, day: u32) -> String {
    let yy = year.rem_euclid(100);
    match locale {
        Locale::En => format!("{month}/{day}/{yy:02}"),
        Locale::De => format!("{day:02}.{month:02}.{yy:02}"),
    }
}

fn format_date_medium_parts(locale: Locale, year: i32, month: u32, day: u32) -> String {
    match locale {
        Locale::En => {
            let mon = EN_SHORT_MONTHS
                .get(month.saturating_sub(1) as usize)
                .copied()
                .unwrap_or("Jan");
            format!("{mon} {day}, {year}")
        }
        Locale::De => format!("{day:02}.{month:02}.{year}"),
    }
}

/// Formats a date in short locale style (`11/22/26` / `22.11.26`).
#[must_use]
pub fn format_date_short<Tz: TimeZone>(locale: Locale, dt: DateTime<Tz>) -> String {
    format_date_short_parts(locale, dt.year(), dt.month(), dt.day())
}

/// Converts a UTC timestamp to the host local timezone and formats its short date.
#[must_use]
pub fn format_date_short_local(locale: Locale, dt: DateTime<Utc>) -> String {
    format_date_short(locale, dt.with_timezone(&Local))
}

/// Formats a UTC timestamp's short date in UTC.
#[must_use]
pub fn format_date_short_utc(locale: Locale, dt: DateTime<Utc>) -> String {
    format_date_short(locale, dt)
}

/// Formats a date in medium locale style (`Nov 22, 2026` / `22.11.2026`).
#[must_use]
pub fn format_date_medium<Tz: TimeZone>(locale: Locale, dt: DateTime<Tz>) -> String {
    format_date_medium_parts(locale, dt.year(), dt.month(), dt.day())
}

/// Converts a UTC timestamp to the host local timezone and formats its medium date.
#[must_use]
pub fn format_date_medium_local(locale: Locale, dt: DateTime<Utc>) -> String {
    format_date_medium(locale, dt.with_timezone(&Local))
}

/// Formats a UTC timestamp's medium date in UTC.
#[must_use]
pub fn format_date_medium_utc(locale: Locale, dt: DateTime<Utc>) -> String {
    format_date_medium(locale, dt)
}

/// Formats a calendar date (`NaiveDate`) in medium locale style.
#[must_use]
pub fn format_naive_date_medium(locale: Locale, date: NaiveDate) -> String {
    format_date_medium_parts(locale, date.year(), date.month(), date.day())
}

/// Formats a date and time in short locale style
/// (`11/22/26, 2:05 PM` / `22.11.26, 14:05`).
#[must_use]
pub fn format_datetime_short<Tz: TimeZone>(locale: Locale, dt: DateTime<Tz>) -> String {
    let date = format_date_short_parts(locale, dt.year(), dt.month(), dt.day());
    match locale {
        Locale::En => format!(
            "{date}, {}",
            format_en_time_12h(dt.hour(), dt.minute(), None)
        ),
        Locale::De => format!("{date}, {:02}:{:02}", dt.hour(), dt.minute()),
    }
}

/// Converts a UTC timestamp to local time and formats it in short style.
#[must_use]
pub fn format_datetime_short_local(locale: Locale, dt: DateTime<Utc>) -> String {
    format_datetime_short(locale, dt.with_timezone(&Local))
}

/// Formats a UTC timestamp in short date+time style in UTC.
#[must_use]
pub fn format_datetime_short_utc(locale: Locale, dt: DateTime<Utc>) -> String {
    format_datetime_short(locale, dt)
}

/// Formats a date and time in medium date + short time style
/// (`Nov 22, 2026, 2:05 PM` / `22.11.2026, 14:05`).
#[must_use]
pub fn format_datetime_medium<Tz: TimeZone>(locale: Locale, dt: DateTime<Tz>) -> String {
    let date = format_date_medium_parts(locale, dt.year(), dt.month(), dt.day());
    match locale {
        Locale::En => format!(
            "{date}, {}",
            format_en_time_12h(dt.hour(), dt.minute(), None)
        ),
        Locale::De => format!("{date}, {:02}:{:02}", dt.hour(), dt.minute()),
    }
}

/// Converts a UTC timestamp to local time and formats it in medium style.
#[must_use]
pub fn format_datetime_medium_local(locale: Locale, dt: DateTime<Utc>) -> String {
    format_datetime_medium(locale, dt.with_timezone(&Local))
}

/// Formats a UTC timestamp in medium date + short time style in UTC.
#[must_use]
pub fn format_datetime_medium_utc(locale: Locale, dt: DateTime<Utc>) -> String {
    format_datetime_medium(locale, dt)
}

/// Formats a date and time in medium date + medium time (with seconds) style
/// (`Nov 22, 2026, 2:05:09 PM` / `22.11.2026, 14:05:09`).
#[must_use]
pub fn format_datetime_medium_with_seconds<Tz: TimeZone>(
    locale: Locale,
    dt: DateTime<Tz>,
) -> String {
    let date = format_date_medium_parts(locale, dt.year(), dt.month(), dt.day());
    match locale {
        Locale::En => format!(
            "{date}, {}",
            format_en_time_12h(dt.hour(), dt.minute(), Some(dt.second()))
        ),
        Locale::De => format!(
            "{date}, {:02}:{:02}:{:02}",
            dt.hour(),
            dt.minute(),
            dt.second()
        ),
    }
}

// ---------------------------------------------------------------------------
// List Formatting
// ---------------------------------------------------------------------------

/// Formats a conjunction list (`"A, B, and C"` / `"A, B und C"`).
#[must_use]
pub fn format_list<S: AsRef<str>>(locale: Locale, items: &[S]) -> String {
    match items {
        [] => String::new(),
        [only] => only.as_ref().to_owned(),
        [first, second] => match locale {
            Locale::En => format!("{} and {}", first.as_ref(), second.as_ref()),
            Locale::De => format!("{} und {}", first.as_ref(), second.as_ref()),
        },
        [head @ .., last] => {
            let joined = head
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", ");
            match locale {
                Locale::En => format!("{joined}, and {}", last.as_ref()),
                Locale::De => format!("{joined} und {}", last.as_ref()),
            }
        }
    }
}

/// Formats a unit list matching `Intl.ListFormat(locale, { type: 'unit' })`
/// (`"1-ply, 2-ply, 3-ply"` in English, `"1-ply, 2-ply und 3-ply"` in German).
#[must_use]
pub fn format_unit_list<S: AsRef<str>>(locale: Locale, items: &[S]) -> String {
    match items {
        [] => String::new(),
        [only] => only.as_ref().to_owned(),
        [first, second] => format!("{}, {}", first.as_ref(), second.as_ref()),
        [head @ .., last] => {
            let joined = head
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", ");
            match locale {
                Locale::En => format!("{joined}, {}", last.as_ref()),
                Locale::De => format!("{joined} und {}", last.as_ref()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Locale Base Collation
// ---------------------------------------------------------------------------

fn fold_base_char(ch: char, out: &mut String) {
    match ch {
        'ß' | 'ẞ' => out.push_str("ss"),
        'æ' | 'Æ' => out.push_str("ae"),
        'œ' | 'Œ' => out.push_str("oe"),
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å'
        | 'Ā' | 'Ă' | 'Ą' => out.push('a'),
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' | 'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => out.push('c'),
        'ď' | 'đ' | 'Ď' | 'Đ' => out.push('d'),
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ'
        | 'Ė' | 'Ę' | 'Ě' => out.push('e'),
        'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => out.push('g'),
        'ĥ' | 'ħ' | 'Ĥ' | 'Ħ' => out.push('h'),
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī'
        | 'Ĭ' | 'Į' | 'İ' => out.push('i'),
        'ĵ' | 'Ĵ' => out.push('j'),
        'ķ' | 'Ķ' => out.push('k'),
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' | 'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => out.push('l'),
        'ñ' | 'ń' | 'ņ' | 'ň' | 'Ñ' | 'Ń' | 'Ņ' | 'Ň' => out.push('n'),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø'
        | 'Ō' | 'Ŏ' | 'Ő' => out.push('o'),
        'ŕ' | 'ŗ' | 'ř' | 'Ŕ' | 'Ŗ' | 'Ř' => out.push('r'),
        'ś' | 'ŝ' | 'ş' | 'š' | 'Ś' | 'Ŝ' | 'Ş' | 'Š' => out.push('s'),
        'ţ' | 'ť' | 'ŧ' | 'Ţ' | 'Ť' | 'Ŧ' => out.push('t'),
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ'
        | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => out.push('u'),
        'ŵ' | 'Ŵ' => out.push('w'),
        'ý' | 'ÿ' | 'ŷ' | 'Ý' | 'Ÿ' | 'Ŷ' => out.push('y'),
        'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => out.push('z'),
        // Drop common Unicode combining diacritical marks (NFD decomposed input).
        '\u{0300}'..='\u{036f}' => {}
        other => {
            for lower in other.to_lowercase() {
                out.push(lower);
            }
        }
    }
}

fn base_sort_key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        fold_base_char(ch, &mut out);
    }
    out
}

/// Compares two strings at base (case- and diacritic-insensitive) collation
/// sensitivity for `locale`, matching `Intl.Collator(locale, { sensitivity: 'base' })`.
///
/// In both English and standard German dictionary collation (DIN 5007-1),
/// umlauts and Latin diacritics fold to their base letters (`ä == a`, `é == e`,
/// `ß == ss`).
#[must_use]
pub fn compare_locale_base(locale: Locale, a: &str, b: &str) -> Ordering {
    let _ = locale;
    base_sort_key(a).cmp(&base_sort_key(b))
}

// ---------------------------------------------------------------------------
// Localized Decimal Input Helpers
// ---------------------------------------------------------------------------

/// Converts typed decimal text into canonical dot-decimal storage form,
/// matching `web/src/shared/format/decimal-input.ts`.
#[must_use]
pub fn canonical_decimal(text: &str) -> String {
    text.trim().replace(',', ".")
}

/// Formats a canonical dot-decimal string (such as `"0.05"`) for display in
/// `locale` (`"0,05"` in German, `"0.05"` in English).
#[must_use]
pub fn format_localized_decimal(locale: Locale, value: &str) -> String {
    match locale.decimal_separator() {
        '.' => value.to_owned(),
        sep => value.replacen('.', &sep.to_string(), 1),
    }
}

/// Parses user-entered decimal text in `locale`.
///
/// Accepts either `.` or `,` as the decimal separator when unambiguous (matching
/// `DecimalInput`'s forgiving input rule), handles locale thousands grouping
/// when both separators appear (e.g. `"1.234,5"` in `De`, `"1,234.5"` in `En`),
/// and rejects empty or non-finite values.
#[must_use]
pub fn parse_localized_decimal(locale: Locale, text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let has_dot = trimmed.contains('.');
    let has_comma = trimmed.contains(',');
    let normalized = match (has_dot, has_comma) {
        (true, true) => {
            let thousands = locale.thousands_separator();
            let decimal = locale.decimal_separator();
            let last_dot = trimmed.rfind('.');
            let last_comma = trimmed.rfind(',');
            let decimal_pos = if decimal == ',' { last_comma } else { last_dot };
            let thousands_pos = if thousands == '.' {
                last_dot
            } else {
                last_comma
            };
            if thousands_pos >= decimal_pos {
                return None;
            }
            trimmed.replace(thousands, "").replace(decimal, ".")
        }
        (false, true) => {
            if trimmed.matches(',').count() > 1 {
                return None;
            }
            trimmed.replace(',', ".")
        }
        (true, false) => {
            if trimmed.matches('.').count() > 1 {
                return None;
            }
            trimmed.to_owned()
        }
        (false, false) => trimmed.to_owned(),
    };
    let parsed: f64 = normalized.parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

fn decimal_places(text: &str) -> usize {
    text.split_once('.').map_or(0, |(_, frac)| frac.len())
}

fn format_canonical_number(value: f64, places: usize) -> String {
    let mut text = format!("{value:.places$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        return "0".to_owned();
    }
    text
}

/// Steps a canonical dot-decimal string by `direction * step`, clamped to
/// `[min, max]` without binary floating-point rounding noise, matching
/// `steppedDecimal` in `web/src/shared/format/decimal-input.ts`.
#[must_use]
pub fn stepped_decimal(
    value: &str,
    direction: i8,
    step: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> Option<String> {
    if !step.is_finite() || direction == 0 {
        return None;
    }
    let trimmed = value.trim();
    let current = if trimmed.is_empty() {
        0.0
    } else {
        let parsed: f64 = trimmed.parse().ok()?;
        if !parsed.is_finite() {
            return None;
        }
        parsed
    };
    let step_str = format!("{step}");
    let places = decimal_places(&step_str).max(decimal_places(trimmed));
    let delta = if direction > 0 { step } else { -step };
    let factor = 10_f64.powi(places as i32);
    let mut next = ((current + delta) * factor).round() / factor;
    if let Some(low) = min {
        next = next.max(low);
    }
    if let Some(high) = max {
        next = next.min(high);
    }
    Some(format_canonical_number(next, places))
}

// ---------------------------------------------------------------------------
// Decision Localization & Checker Notation Compaction
// ---------------------------------------------------------------------------

/// Minimal checker board context used by [`localize_decision_in_position`] to
/// preserve intermediate hit markers (`24/21*/15`) even when the source omitted `*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckerPositionRef<'a> {
    pub board: &'a [i8; 26],
    /// `0` for White, `1` for Black.
    pub player: u8,
    pub dice: [u8; 2],
}

fn decision_key(notation: &str) -> Option<&'static str> {
    match notation.trim().to_ascii_lowercase().as_str() {
        "no double" => Some("decision.noDouble"),
        "double" => Some("decision.double"),
        "take" => Some("decision.take"),
        "pass" | "drop" => Some("decision.drop"),
        "cannot move" | "dance" => Some("decision.cannotMove"),
        "resign" => Some("decision.resign"),
        _ => None,
    }
}

/// Localizes cube/pass/resign keywords (`"No double"` → `"Nicht doppeln"`) and
/// compacts multi-step checker notation (`"24/21/15"` → `"24/15"`, preserving
/// hits like `"24/21*/15"`).
#[must_use]
pub fn localize_decision(locale: Locale, notation: &str) -> String {
    localize_decision_in_position(locale, notation, None)
}

/// Same as [`localize_decision`], optionally consulting `position` to retain
/// hits on intermediate points even if the source notation omitted `*`.
#[must_use]
pub fn localize_decision_in_position(
    locale: Locale,
    notation: &str,
    position: Option<CheckerPositionRef<'_>>,
) -> String {
    if let Some(key) = decision_key(notation)
        && let Some(translated) = lookup(locale, "app", key)
    {
        return translated.to_owned();
    }
    compact_checker_notation(notation, position)
}

#[derive(Debug, Clone)]
struct Run {
    points: Vec<u8>,
    hits: Vec<bool>,
}

#[derive(Debug, Clone, Copy)]
struct LocalStep {
    from: u8,
    to: u8,
    die: u8,
    hit: bool,
}

fn parse_runs(notation: &str) -> Option<Vec<Run>> {
    if notation.is_empty() || notation.len() > 160 {
        return None;
    }
    let mut runs = Vec::new();
    let mut segments = 0usize;
    for token in notation.split_whitespace() {
        let (path_str, copies) = if let Some(stripped) = token.strip_suffix(')') {
            let (prefix, count_str) = stripped.rsplit_once('(')?;
            if count_str.len() != 1 {
                return None;
            }
            let count = count_str.parse::<usize>().ok()?;
            if !(1..=4).contains(&count) {
                return None;
            }
            (prefix, count)
        } else {
            (token, 1)
        };

        let parts: Vec<&str> = path_str.split('/').collect();
        if parts.len() < 2 {
            return None;
        }
        let mut points = Vec::with_capacity(parts.len());
        let mut hits = Vec::with_capacity(parts.len() - 1);
        for (index, raw_part) in parts.iter().enumerate() {
            let lower = raw_part.to_ascii_lowercase();
            let (base, hit) = if index == 0 {
                if lower.ends_with('*') {
                    return None;
                }
                (lower.as_str(), false)
            } else if let Some(without_star) = lower.strip_suffix('*') {
                (without_star, true)
            } else {
                (lower.as_str(), false)
            };
            let point = match base {
                "bar" if index == 0 => 25,
                "off" if index > 0 && !hit => 0,
                number => {
                    if number.starts_with('0') || !number.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    let val = number.parse::<u8>().ok()?;
                    if !(1..=24).contains(&val) {
                        return None;
                    }
                    val
                }
            };
            if let Some(&prev) = points.last()
                && point >= prev
            {
                return None;
            }
            points.push(point);
            if index > 0 {
                hits.push(hit);
            }
        }

        segments += (points.len() - 1) * copies;
        if segments > 4 {
            return None;
        }
        for _ in 0..copies {
            runs.push(Run {
                points: points.clone(),
                hits: hits.clone(),
            });
        }
    }
    Some(runs)
}

fn join_runs(runs: Vec<Run>) -> Vec<Run> {
    let mut joined: Vec<Run> = Vec::new();
    for run in runs {
        if let Some(previous) = joined
            .iter_mut()
            .find(|path| path.points.last() == run.points.first())
        {
            previous.points.extend_from_slice(&run.points[1..]);
            previous.hits.extend_from_slice(&run.hits);
        } else {
            joined.push(run);
        }
    }
    joined
}

fn legal_step(board: &[i8; 26], from: u8, die: u8) -> Option<([i8; 26], LocalStep)> {
    let from_idx = usize::from(from);
    if board[from_idx] <= 0 || (board[25] > 0 && from != 25) {
        return None;
    }
    let to = from.saturating_sub(die);
    let to_idx = usize::from(to);
    if to > 0 {
        if board[to_idx] < -1 {
            return None;
        }
    } else if from > 6
        || board[7..].iter().any(|&c| c > 0)
        || (die > from && board[from_idx + 1..7].iter().any(|&c| c > 0))
    {
        return None;
    }
    let mut after = *board;
    let hit = to > 0 && after[to_idx] == -1;
    after[from_idx] -= 1;
    if to > 0 {
        if hit {
            after[to_idx] = 0;
            after[0] -= 1;
        }
        after[to_idx] += 1;
    }
    Some((after, LocalStep { from, to, die, hit }))
}

fn any_step(board: &[i8; 26], die: u8) -> bool {
    (1..=25).any(|from| legal_step(board, from, die).is_some())
}

fn maximum_plays(
    board: &[i8; 26],
    dice: &[u8],
    memo: &mut HashMap<([i8; 26], Vec<u8>), usize>,
) -> usize {
    if dice.is_empty() {
        return 0;
    }
    let key = (*board, dice.to_vec());
    if let Some(&cached) = memo.get(&key) {
        return cached;
    }
    let mut best = 0usize;
    for (index, &die) in dice.iter().enumerate() {
        if index > 0 && die == dice[index - 1] {
            continue;
        }
        let mut remaining = Vec::with_capacity(dice.len() - 1);
        remaining.extend_from_slice(&dice[..index]);
        remaining.extend_from_slice(&dice[index + 1..]);
        for from in 1..=25 {
            let Some((after, _)) = legal_step(board, from, die) else {
                continue;
            };
            let plays = 1 + maximum_plays(&after, &remaining, memo);
            if plays > best {
                best = plays;
                if best == dice.len() {
                    memo.insert(key, best);
                    return best;
                }
            }
        }
    }
    memo.insert(key, best);
    best
}

fn expand_runs(
    board: &[i8; 26],
    dice: &[u8],
    runs: &[Run],
    required: usize,
    higher_die: Option<u8>,
    steps: &mut Vec<LocalStep>,
) -> Option<Vec<LocalStep>> {
    if runs.is_empty() {
        let won = !board.iter().any(|&c| c > 0);
        if steps.len() != required && !won {
            return None;
        }
        if steps.len() == 1 && higher_die.is_some_and(|high| steps[0].die != high) {
            return None;
        }
        return Some(steps.clone());
    }
    if dice.is_empty() {
        return None;
    }
    for (run_index, run) in runs.iter().enumerate() {
        let from = run.points[0];
        let target = run.points[1];
        for (die_index, &die) in dice.iter().enumerate() {
            if die_index > 0 && die == dice[die_index - 1] {
                continue;
            }
            let Some((after, step)) = legal_step(board, from, die) else {
                continue;
            };
            if step.to < target {
                continue;
            }
            let reached = step.to == target;
            if (!reached && step.hit) || (reached && run.hits[0] && !step.hit) {
                continue;
            }
            let mut next_runs = runs.to_vec();
            if reached && run.points.len() == 2 {
                next_runs.remove(run_index);
            } else if reached {
                next_runs[run_index] = Run {
                    points: run.points[1..].to_vec(),
                    hits: run.hits[1..].to_vec(),
                };
            } else {
                let mut points = Vec::with_capacity(run.points.len());
                points.push(step.to);
                points.extend_from_slice(&run.points[1..]);
                next_runs[run_index] = Run {
                    points,
                    hits: run.hits.clone(),
                };
            }
            let mut remaining_dice = Vec::with_capacity(dice.len() - 1);
            remaining_dice.extend_from_slice(&dice[..die_index]);
            remaining_dice.extend_from_slice(&dice[die_index + 1..]);
            steps.push(step);
            let found = expand_runs(
                &after,
                &remaining_dice,
                &next_runs,
                required,
                higher_die,
                steps,
            );
            steps.pop();
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

fn project_local_steps(position: CheckerPositionRef<'_>, runs: &[Run]) -> Option<Vec<LocalStep>> {
    let board = position.board;
    if board[0] > 0 || board[25] < 0 || position.player > 1 {
        return None;
    }
    let white_total: i32 = board.iter().map(|&c| i32::from(c.max(0))).sum();
    let black_total: i32 = board.iter().map(|&c| i32::from((-c).max(0))).sum();
    if white_total > 15 || black_total > 15 {
        return None;
    }
    if !(1..=6).contains(&position.dice[0]) || !(1..=6).contains(&position.dice[1]) {
        return None;
    }
    let mut local = [0i8; 26];
    if position.player == 0 {
        local = *board;
    } else {
        for (i, &count) in board.iter().rev().enumerate() {
            local[i] = -count;
        }
    }
    if !local.iter().any(|&c| c > 0) {
        return None;
    }
    let mut available = if position.dice[0] == position.dice[1] {
        vec![position.dice[0]; 4]
    } else {
        vec![position.dice[0], position.dice[1]]
    };
    available.sort_unstable_by(|a, b| b.cmp(a));
    let required = maximum_plays(&local, &available, &mut HashMap::new());
    let high = position.dice[0].max(position.dice[1]);
    let higher_die =
        (position.dice[0] != position.dice[1] && any_step(&local, high)).then_some(high);
    let mut steps = Vec::new();
    expand_runs(&local, &available, runs, required, higher_die, &mut steps)
}

/// Compacts chained checker notation (e.g. `"24/21 21/15"` → `"24/15"`,
/// `"24/21* 21/15"` → `"24/21*/15"`, `"13/10(2) 10/7(2)"` → `"13/7(2)"`),
/// matching `compactCheckerNotation` in `web/src/shared/board/move-preview.ts`.
#[must_use]
pub fn compact_checker_notation(
    notation: &str,
    position: Option<CheckerPositionRef<'_>>,
) -> String {
    let Some(runs) = parse_runs(notation.trim()) else {
        return notation.to_owned();
    };
    let mut paths = join_runs(runs.clone());
    if paths.iter().all(|path| path.points.len() == 2) {
        return notation.to_owned();
    }

    if let Some(pos) = position {
        let Some(steps) = project_local_steps(pos, &runs) else {
            return notation.to_owned();
        };
        paths = join_runs(
            steps
                .into_iter()
                .map(|step| Run {
                    points: vec![step.from, step.to],
                    hits: vec![step.hit],
                })
                .collect(),
        );
    }

    let mut counts: Vec<(String, usize)> = Vec::new();
    for Run { points, hits } in paths {
        let mut parts = Vec::with_capacity(points.len());
        for (index, &point) in points.iter().enumerate() {
            let hit = index > 0 && hits[index - 1];
            if index > 0 && index + 1 < points.len() && !hit {
                continue;
            }
            let name = match point {
                25 => "bar".to_owned(),
                0 => "off".to_owned(),
                p => p.to_string(),
            };
            if hit {
                parts.push(format!("{name}*"));
            } else {
                parts.push(name);
            }
        }
        let path_text = parts.join("/");
        if let Some((_, count)) = counts.iter_mut().find(|(p, _)| p == &path_text) {
            *count += 1;
        } else {
            counts.push((path_text, 1));
        }
    }

    counts
        .into_iter()
        .map(|(path, count)| {
            if count > 1 {
                format!("{path}({count})")
            } else {
                path
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
