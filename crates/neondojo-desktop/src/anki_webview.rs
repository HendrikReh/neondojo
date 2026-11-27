//! Scoped macOS `WKWebView` renderer for third-party HTML/JS Anki cards (`card.kind == "anki"`).
//!
//! Replaced in Wave 2 by the full `objc2-web-kit` `WKWebView` + `WKURLSchemeHandler` implementation
//! serving HTML/media directly from [`neondojo::facade::BackendFacade`] with strict CSP.

/// Placeholder configuration for the scoped Anki card face renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCardFaceRequest {
    pub card_id: uuid::Uuid,
    pub side: String,
}
