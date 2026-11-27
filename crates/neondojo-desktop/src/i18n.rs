//! Live `neondojo-i18n` localization state and formatting helpers for GPUI.
//!
//! Synchronizes `LocaleGlobal` with `gpui_kit::component::set_locale` and
//! triggers window refreshes so switching between English (`en`) and German
//! (`de`) updates every visible label, menu item, notice, and `AppError` live.

use gpui_kit::{App, Global};
use neondojo::domain_error::DomainError;
use neondojo_i18n::{AppError, I18n, Locale, Param};

/// Active UI language stored as a GPUI [`Global`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleGlobal(pub Locale);

impl Default for LocaleGlobal {
    fn default() -> Self {
        Self(Locale::DEFAULT)
    }
}

impl Global for LocaleGlobal {}

/// Initializes the active language and synchronizes `gpui-kit`'s internal locale.
pub fn init_locale(locale: Locale, cx: &mut App) {
    cx.set_global(LocaleGlobal(locale));
    gpui_kit::component::set_locale(locale.code());
}

/// Switches the active language live, updating `gpui-kit` and refreshing all windows.
pub fn set_locale(locale: Locale, cx: &mut App) {
    let changed = cx
        .try_global::<LocaleGlobal>()
        .is_none_or(|current| current.0 != locale);
    cx.set_global(LocaleGlobal(locale));
    gpui_kit::component::set_locale(locale.code());
    if changed {
        cx.refresh_windows();
    }
}

/// Returns the active [`Locale`] (defaulting to [`Locale::DEFAULT`]).
pub fn current_locale(cx: &App) -> Locale {
    cx.try_global::<LocaleGlobal>()
        .map(|g| g.0)
        .unwrap_or(Locale::DEFAULT)
}

/// Translates `(group, key)` using the active [`Locale`].
pub fn t<'a>(cx: &App, group: &str, key: &'a str) -> &'a str {
    I18n::t(current_locale(cx), group, key)
}

/// Formats `(group, key)` with ICU parameters using the active [`Locale`].
pub fn fmt(cx: &App, group: &str, key: &str, params: &[(&str, Param)]) -> String {
    I18n::format(current_locale(cx), group, key, params)
}

/// Formats a structured [`AppError`] using the active [`Locale`].
pub fn format_error(cx: &App, error: &AppError) -> String {
    I18n::format_error(current_locale(cx), error)
}

/// Formats a backend [`DomainError`] using NeonDojo's `configuration-error`
/// and `configuration-issue` catalogs when structured codes are present.
pub fn format_domain_error(locale: Locale, error: &DomainError) -> String {
    let base = error
        .body
        .code
        .as_deref()
        .and_then(|code| I18n::try_t(locale, "configuration-error", code))
        .unwrap_or(error.body.error.as_str());

    let Some(issues) = error.body.issues.as_deref() else {
        return base.to_string();
    };
    if issues.is_empty() {
        return base.to_string();
    }

    let details: Vec<String> = issues
        .iter()
        .map(|issue| {
            if let Some(msg) = I18n::try_t(locale, "configuration-issue", &issue.code) {
                if issue.code == "invalid_value" {
                    I18n::format(
                        locale,
                        "configuration-issue",
                        "invalid_value",
                        &[("detail", Param::from(issue.message.as_str()))],
                    )
                } else {
                    msg.to_string()
                }
            } else {
                issue.message.clone()
            }
        })
        .collect();

    format!("{base} {}", details.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use neondojo::domain_error::ErrorKind;

    #[test]
    fn formats_structured_domain_errors_in_en_and_de() {
        let mut err = DomainError::new(
            ErrorKind::Conflict,
            "A profile with this name already exists",
        );
        err.body.code = Some("duplicate_name".into());

        assert_eq!(
            format_domain_error(Locale::En, &err),
            "A profile with this name already exists."
        );
        assert_ne!(
            format_domain_error(Locale::De, &err),
            format_domain_error(Locale::En, &err)
        );
    }
}
