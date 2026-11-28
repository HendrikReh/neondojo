//! Locale-aware decimal input mirroring `web/src/shared/format/decimal-input.ts`.
//!
//! Accepts both `,` and `.` as decimal separators, renders with the active
//! language's decimal separator and tabular numerals, exposes dot-decimal
//! canonical strings to callers, and supports `ArrowUp` / `ArrowDown` stepping
//! without binary floating-point rounding noise.

use crate::theme::NeonTypographyExt;
use gpui_kit::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, Subscription, TestSupportExt,
    Window,
    component::input::{Input, InputEvent, InputState},
    div, px,
};
use std::rc::Rc;

type ValueChangeHandler = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;

/// Returns the decimal separator for an app language (`','` for German, `'.'` otherwise).
pub fn decimal_separator_for_locale(locale: &str) -> char {
    let trimmed = locale.trim();
    let base = trimmed.split(['-', '_']).next().unwrap_or(trimmed);
    neondojo_i18n::Locale::parse(base).decimal_separator()
}

/// Converts user-typed decimal text into canonical dot-decimal form.
pub fn canonical_decimal(text: &str) -> String {
    neondojo_i18n::canonical_decimal(text)
}

/// Formats a canonical dot-decimal string using the target locale's decimal separator.
pub fn display_decimal(value: &str, separator: char) -> String {
    if separator == ',' {
        neondojo_i18n::format_localized_decimal(neondojo_i18n::Locale::De, value)
    } else if separator == '.' {
        value.to_string()
    } else {
        value.replace('.', &separator.to_string())
    }
}

/// Computes the next stepped canonical value (`direction = 1` or `-1`), clamped
/// to `[min, max]` without binary float noise (e.g. `0.08 + 0.01 => "0.09"`).
pub fn stepped_decimal(
    value: &str,
    direction: i8,
    step: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> Option<String> {
    if !step.is_finite() || step <= 0.0 {
        return None;
    }
    let dir = if direction >= 0 { 1 } else { -1 };
    neondojo_i18n::stepped_decimal(&canonical_decimal(value), dir, step, min, max)
}

/// Configuration for a [`DecimalInput`] control.
#[derive(Debug, Clone)]
pub struct DecimalInputConfig {
    pub id: SharedString,
    pub step: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub separator: char,
    pub placeholder: SharedString,
    pub width_px: f32,
}

impl DecimalInputConfig {
    pub fn new(id: impl Into<SharedString>, step: f64) -> Self {
        Self {
            id: id.into(),
            step,
            min: None,
            max: None,
            separator: '.',
            placeholder: SharedString::default(),
            width_px: 108.0,
        }
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    pub fn separator(mut self, separator: char) -> Self {
        self.separator = separator;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn width(mut self, width_px: f32) -> Self {
        self.width_px = width_px;
        self
    }
}

/// Interactive GPUI decimal input wrapping `gpui_kit::component::input::Input`
/// with locale separator rendering and `ArrowUp` / `ArrowDown` stepping.
pub struct DecimalInput {
    config: DecimalInputConfig,
    canonical_value: String,
    input_state: Entity<InputState>,
    on_change: Option<ValueChangeHandler>,
    _subscription: Subscription,
}

impl DecimalInput {
    pub fn new(
        config: DecimalInputConfig,
        initial_canonical: impl Into<String>,
        on_change: Option<ValueChangeHandler>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let canonical = canonical_decimal(&initial_canonical.into());
        let displayed = display_decimal(&canonical, config.separator);
        let placeholder = config.placeholder.clone();
        let input_state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            if !placeholder.is_empty() {
                state = state.placeholder(placeholder);
            }
            if !displayed.is_empty() {
                state = state.default_value(displayed);
            }
            state
        });

        let subscription = cx.subscribe_in(
            &input_state,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let raw = state.read(cx).value();
                    let next = canonical_decimal(raw.as_ref());
                    if next != this.canonical_value {
                        this.canonical_value = next.clone();
                        if let Some(cb) = &this.on_change {
                            cb(&next, window, cx);
                        }
                        cx.notify();
                    }
                }
            },
        );

        Self {
            config,
            canonical_value: canonical,
            input_state,
            on_change,
            _subscription: subscription,
        }
    }

    /// Returns the canonical dot-decimal value string.
    pub fn canonical_value(&self) -> &str {
        &self.canonical_value
    }

    /// Programmatically updates the field from a canonical dot-decimal string.
    pub fn set_canonical_value(
        &mut self,
        canonical: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = canonical_decimal(canonical);
        self.canonical_value = next.clone();
        let displayed = display_decimal(&next, self.config.separator);
        self.input_state.update(cx, |state, cx| {
            state.set_value(displayed, window, cx);
        });
        cx.notify();
    }

    /// Updates the displayed decimal separator when the app language changes.
    pub fn set_separator(&mut self, separator: char, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.separator == separator {
            return;
        }
        self.config.separator = separator;
        let displayed = display_decimal(&self.canonical_value, separator);
        self.input_state.update(cx, |state, cx| {
            state.set_value(displayed, window, cx);
        });
        cx.notify();
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let direction = match event.keystroke.key.as_str() {
            "up" => 1,
            "down" => -1,
            _ => return,
        };
        let Some(next) = stepped_decimal(
            &self.canonical_value,
            direction,
            self.config.step,
            self.config.min,
            self.config.max,
        ) else {
            return;
        };
        cx.stop_propagation();
        self.set_canonical_value(&next, window, cx);
        if let Some(cb) = &self.on_change {
            cb(&next, window, cx);
        }
    }
}

impl Focusable for DecimalInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input_state.focus_handle(cx)
    }
}

impl Render for DecimalInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(self.config.id.clone())
            .test_support()
            .w(px(self.config.width_px))
            .tabular_nums()
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(
                Input::new(&self.input_state)
                    .id(SharedString::from(format!("{}-input", self.config.id))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_and_display_decimal_support_comma_and_dot() {
        assert_eq!(decimal_separator_for_locale("de"), ',');
        assert_eq!(decimal_separator_for_locale("de-DE"), ',');
        assert_eq!(decimal_separator_for_locale("en"), '.');
        assert_eq!(canonical_decimal(" 0,080 "), "0.080");
        assert_eq!(canonical_decimal("1.25"), "1.25");
        assert_eq!(display_decimal("0.080", ','), "0,080");
        assert_eq!(display_decimal("0.080", '.'), "0.080");
    }

    #[test]
    fn stepped_decimal_avoids_float_noise_and_clamps() {
        assert_eq!(
            stepped_decimal("0.08", 1, 0.01, Some(0.0), Some(1.0)).as_deref(),
            Some("0.09")
        );
        assert_eq!(
            stepped_decimal("0,1", 1, 0.2, Some(0.0), Some(1.0)).as_deref(),
            Some("0.3")
        );
        assert_eq!(
            stepped_decimal("0.01", -1, 0.05, Some(0.0), Some(1.0)).as_deref(),
            Some("0")
        );
        assert_eq!(
            stepped_decimal("0.98", 1, 0.05, Some(0.0), Some(1.0)).as_deref(),
            Some("1")
        );
        assert_eq!(
            stepped_decimal("", 1, 0.02, Some(0.0), None).as_deref(),
            Some("0.02")
        );
        assert_eq!(stepped_decimal("abc", 1, 0.01, None, None), None);
    }
}
