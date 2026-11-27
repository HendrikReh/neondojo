//! ICU placeholder interpolation and plural-form evaluation.
//!
//! Matches `web/src/shared/i18n/i18n-core.ts`:
//! - Simple placeholders: `{name}` where `name` matches `[A-Za-z][A-Za-z0-9_]*`.
//! - ICU plural blocks: `{count, plural, one {# card} other {# cards}}`,
//!   supporting nested braces, multiple plural blocks per template, `#`
//!   localized count substitution inside plural arms, and non-recursive
//!   single-pass parameter substitution so parameter values like `"{dice}"`
//!   are never re-interpreted.

use crate::catalog::Locale;
use crate::format::{format_int, format_number_default, format_uint};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

/// Typed parameter passed to ICU message interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Param {
    Text(String),
    Int(i64),
    Uint(u64),
    Float(f64),
}

impl Param {
    /// Formats this parameter in `locale`'s number convention (or verbatim for text).
    #[must_use]
    pub fn format_value(&self, locale: Locale) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Int(value) => format_int(locale, *value),
            Self::Uint(value) => format_uint(locale, *value),
            Self::Float(value) => format_number_default(locale, *value),
        }
    }

    fn plural_category_and_text(&self, locale: Locale) -> Option<(PluralCategory, String)> {
        match self {
            Self::Text(_) => None,
            Self::Int(value) => Some((plural_category(locale, *value), format_int(locale, *value))),
            Self::Uint(value) => {
                let category = if *value == 1 {
                    PluralCategory::One
                } else {
                    PluralCategory::Other
                };
                Some((category, format_uint(locale, *value)))
            }
            Self::Float(value) => Some((
                plural_category_f64(locale, *value),
                format_number_default(locale, *value),
            )),
        }
    }
}

impl From<&str> for Param {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for Param {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Cow<'_, str>> for Param {
    fn from(value: Cow<'_, str>) -> Self {
        Self::Text(value.into_owned())
    }
}

impl From<i8> for Param {
    fn from(value: i8) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<i16> for Param {
    fn from(value: i16) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<i32> for Param {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<i64> for Param {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<isize> for Param {
    fn from(value: isize) -> Self {
        Self::Int(value as i64)
    }
}

impl From<u8> for Param {
    fn from(value: u8) -> Self {
        Self::Uint(u64::from(value))
    }
}

impl From<u16> for Param {
    fn from(value: u16) -> Self {
        Self::Uint(u64::from(value))
    }
}

impl From<u32> for Param {
    fn from(value: u32) -> Self {
        Self::Uint(u64::from(value))
    }
}

impl From<u64> for Param {
    fn from(value: u64) -> Self {
        Self::Uint(value)
    }
}

impl From<usize> for Param {
    fn from(value: usize) -> Self {
        Self::Uint(value as u64)
    }
}

impl From<f32> for Param {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

impl From<f64> for Param {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// CLDR LDML cardinal plural categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// Parses a CLDR plural rule name.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "zero" => Some(Self::Zero),
            "one" => Some(Self::One),
            "two" => Some(Self::Two),
            "few" => Some(Self::Few),
            "many" => Some(Self::Many),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }
}

/// Returns the CLDR cardinal plural category for an integer count in `locale`.
///
/// Both English (`en`) and German (`de`) use `one` for exact integer `1` (and `-1`)
/// and `other` for all other counts.
#[must_use]
pub const fn plural_category(locale: Locale, count: i64) -> PluralCategory {
    match locale {
        Locale::En | Locale::De => {
            if count == 1 || count == -1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

/// Returns the CLDR cardinal plural category for a floating-point count in `locale`.
#[must_use]
pub fn plural_category_f64(locale: Locale, count: f64) -> PluralCategory {
    match locale {
        Locale::En | Locale::De => {
            if count.is_finite() && count.fract() == 0.0 && (count == 1.0 || count == -1.0) {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

/// Errors produced when parsing, validating, or formatting an ICU template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcuError {
    UnclosedBrace {
        template: String,
    },
    InvalidPluralBlock {
        template: String,
    },
    UnknownPluralCategory {
        category: String,
        template: String,
    },
    MissingPluralForm {
        locale: Locale,
        category: &'static str,
        template: String,
    },
    MissingParam {
        name: String,
    },
    NonNumericPluralParam {
        name: String,
    },
}

impl fmt::Display for IcuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedBrace { template } => write!(f, "unclosed brace in template: {template}"),
            Self::InvalidPluralBlock { template } => {
                write!(f, "invalid plural block in template: {template}")
            }
            Self::UnknownPluralCategory { category, template } => {
                write!(
                    f,
                    "unknown plural category `{category}` in template: {template}"
                )
            }
            Self::MissingPluralForm {
                locale,
                category,
                template,
            } => write!(
                f,
                "missing plural form `{category}` for locale `{locale}` in template: {template}"
            ),
            Self::MissingParam { name } => write!(f, "missing translation parameter: {name}"),
            Self::NonNumericPluralParam { name } => {
                write!(f, "plural parameter must be numeric: {name}")
            }
        }
    }
}

impl std::error::Error for IcuError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Literal(String),
    Hash,
    Placeholder(String),
    Plural {
        name: String,
        forms: Vec<(PluralCategory, Vec<Node>)>,
    },
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Returns the byte index of the matching `}` for the `{` at `open`.
fn find_closing_brace(text: &str, open: usize, full_template: &str) -> Result<usize, IcuError> {
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
    }
    Err(IcuError::UnclosedBrace {
        template: full_template.to_owned(),
    })
}

fn parse_nodes(input: &str, in_plural: bool, full_template: &str) -> Result<Vec<Node>, IcuError> {
    let mut nodes = Vec::new();
    let mut literal = String::new();
    let mut cursor = 0usize;

    while cursor < input.len() {
        let rest = &input[cursor..];
        let mut chars = rest.char_indices();
        let Some((_, ch)) = chars.next() else {
            break;
        };

        match ch {
            '#' if in_plural => {
                if !literal.is_empty() {
                    nodes.push(Node::Literal(std::mem::take(&mut literal)));
                }
                nodes.push(Node::Hash);
                cursor += 1;
            }
            '{' => {
                let close = find_closing_brace(input, cursor, full_template)?;
                let inner = &input[cursor + 1..close];
                if is_identifier(inner) {
                    if !literal.is_empty() {
                        nodes.push(Node::Literal(std::mem::take(&mut literal)));
                    }
                    nodes.push(Node::Placeholder(inner.to_owned()));
                } else if let Some(plural) = try_parse_plural(inner, full_template)? {
                    if !literal.is_empty() {
                        nodes.push(Node::Literal(std::mem::take(&mut literal)));
                    }
                    nodes.push(plural);
                } else {
                    // Not a placeholder or plural block: preserve verbatim braces.
                    literal.push_str(&input[cursor..=close]);
                }
                cursor = close + 1;
            }
            _ => {
                literal.push(ch);
                cursor += ch.len_utf8();
            }
        }
    }

    if !literal.is_empty() {
        nodes.push(Node::Literal(literal));
    }
    Ok(nodes)
}

fn try_parse_plural(inner: &str, full_template: &str) -> Result<Option<Node>, IcuError> {
    let Some((raw_name, after_name)) = inner.split_once(',') else {
        return Ok(None);
    };
    let name = raw_name.trim();
    let after_name = after_name.trim_start();
    let Some(after_kind) = after_name.strip_prefix("plural") else {
        return Ok(None);
    };
    let after_kind = after_kind.trim_start();
    let Some(mut body) = after_kind.strip_prefix(',') else {
        return Err(IcuError::InvalidPluralBlock {
            template: full_template.to_owned(),
        });
    };
    if !is_identifier(name) {
        return Err(IcuError::InvalidPluralBlock {
            template: full_template.to_owned(),
        });
    }

    let mut forms = Vec::new();
    while !body.trim().is_empty() {
        let Some(open) = body.find('{') else {
            return Err(IcuError::InvalidPluralBlock {
                template: full_template.to_owned(),
            });
        };
        let category_raw = body[..open].trim();
        let Some(category) = PluralCategory::parse(category_raw) else {
            return Err(IcuError::UnknownPluralCategory {
                category: category_raw.to_owned(),
                template: full_template.to_owned(),
            });
        };
        let close = find_closing_brace(body, open, full_template)?;
        let arm_body = &body[open + 1..close];
        let arm_nodes = parse_nodes(arm_body, true, full_template)?;
        if let Some((_, existing)) = forms.iter_mut().find(|(cat, _)| *cat == category) {
            *existing = arm_nodes;
        } else {
            forms.push((category, arm_nodes));
        }
        body = &body[close + 1..];
    }

    if forms.is_empty() {
        return Err(IcuError::InvalidPluralBlock {
            template: full_template.to_owned(),
        });
    }

    Ok(Some(Node::Plural {
        name: name.to_owned(),
        forms,
    }))
}

fn collect_placeholders(nodes: &[Node], names: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Literal(_) | Node::Hash => {}
            Node::Placeholder(name) => {
                names.insert(name.clone());
            }
            Node::Plural { name, forms } => {
                names.insert(name.clone());
                for (_, body) in forms {
                    collect_placeholders(body, names);
                }
            }
        }
    }
}

fn collect_plural_names(nodes: &[Node], names: &mut BTreeSet<String>) {
    for node in nodes {
        if let Node::Plural { name, forms } = node {
            names.insert(name.clone());
            for (_, body) in forms {
                collect_plural_names(body, names);
            }
        }
    }
}

fn validate_nodes(nodes: &[Node], locale: Locale, full_template: &str) -> Result<(), IcuError> {
    for node in nodes {
        if let Node::Plural { forms, .. } = node {
            for required in [PluralCategory::One, PluralCategory::Other] {
                if !forms.iter().any(|(cat, _)| *cat == required) {
                    return Err(IcuError::MissingPluralForm {
                        locale,
                        category: required.as_str(),
                        template: full_template.to_owned(),
                    });
                }
            }
            for (_, body) in forms {
                validate_nodes(body, locale, full_template)?;
            }
        }
    }
    Ok(())
}

fn eval_nodes(
    nodes: &[Node],
    locale: Locale,
    params: &[(&str, Param)],
    current_plural_count: Option<&str>,
    full_template: &str,
    out: &mut String,
) -> Result<(), IcuError> {
    for node in nodes {
        match node {
            Node::Literal(text) => out.push_str(text),
            Node::Hash => out.push_str(current_plural_count.unwrap_or("#")),
            Node::Placeholder(name) => {
                let param = find_param(params, name)
                    .ok_or_else(|| IcuError::MissingParam { name: name.clone() })?;
                out.push_str(&param.format_value(locale));
            }
            Node::Plural { name, forms } => {
                let param = find_param(params, name)
                    .ok_or_else(|| IcuError::MissingParam { name: name.clone() })?;
                let (category, formatted_count) = param
                    .plural_category_and_text(locale)
                    .ok_or_else(|| IcuError::NonNumericPluralParam { name: name.clone() })?;
                let selected = forms
                    .iter()
                    .find(|(cat, _)| *cat == category)
                    .or_else(|| forms.iter().find(|(cat, _)| *cat == PluralCategory::Other))
                    .ok_or_else(|| IcuError::MissingPluralForm {
                        locale,
                        category: PluralCategory::Other.as_str(),
                        template: full_template.to_owned(),
                    })?;
                eval_nodes(
                    &selected.1,
                    locale,
                    params,
                    Some(&formatted_count),
                    full_template,
                    out,
                )?;
            }
        }
    }
    Ok(())
}

fn find_param<'a>(params: &'a [(&str, Param)], name: &str) -> Option<&'a Param> {
    params
        .iter()
        .rev()
        .find_map(|(key, value)| (*key == name).then_some(value))
}

/// Extracts every sorted, deduplicated parameter name referenced by `template`
/// (both `{name}` placeholders and `{name, plural, ...}` selectors).
pub fn extract_placeholders(template: &str) -> Result<Vec<String>, IcuError> {
    let nodes = parse_nodes(template, false, template)?;
    let mut names = BTreeSet::new();
    collect_placeholders(&nodes, &mut names);
    Ok(names.into_iter().collect())
}

/// Extracts every sorted, deduplicated plural selector parameter name in `template`.
pub fn extract_plural_params(template: &str) -> Result<Vec<String>, IcuError> {
    let nodes = parse_nodes(template, false, template)?;
    let mut names = BTreeSet::new();
    collect_plural_names(&nodes, &mut names);
    Ok(names.into_iter().collect())
}

/// Validates that every plural block in `template` is well-formed and supplies
/// all plural forms required by `locale` (`one` and `other` for `En`/`De`).
pub fn validate_plural_forms(locale: Locale, template: &str) -> Result<(), IcuError> {
    let nodes = parse_nodes(template, false, template)?;
    validate_nodes(&nodes, locale, template)
}

/// Formats an ICU message `template` for `locale` with `params`.
pub fn format_template(
    locale: Locale,
    template: &str,
    params: &[(&str, Param)],
) -> Result<String, IcuError> {
    let nodes = parse_nodes(template, false, template)?;
    let mut out = String::with_capacity(template.len());
    eval_nodes(&nodes, locale, params, None, template, &mut out)?;
    Ok(out)
}
