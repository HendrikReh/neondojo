//! Scoped macOS `WKWebView` + `WKURLSchemeHandler` renderer strictly for
//! third-party HTML/JS Anki cards (`card.kind == "anki"`).
//!
//! Never used for Markdown, AI assessments, or any native NeonDojo view.
//! Serves HTML card faces and binary media directly from
//! [`neondojo::facade::BackendFacade`] over the custom `neondojo-anki://`
//! scheme with strict Content Security Policy enforcement:
//! - `neondojo-anki://card/{card_id}/{front|back}` → `facade.get_anki_card_face_html(card_id, side)`
//! - `neondojo-anki://media/{package}/{name}` → `facade.get_anki_media_blob(package, name)`
//!
//! In headless tests (`#[gpui_kit::test]`) or when no live `BackendFacade` or
//! macOS `NSWindow` is present, [`AnkiCardFaceView`] degrades gracefully to a
//! testable GPUI fallback container with zero AppKit/WebKit initialization.

use crate::app_context::DesktopAppContext;
use crate::i18n::current_locale;
use crate::theme::{NeonTheme, NeonTypographyExt};
use gpui_kit::{
    App, AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    Styled, TestSupportExt, Window, base::StyledExt, canvas, div, px,
};
use neondojo_i18n::I18n;
use uuid::Uuid;

/// Custom WebKit URL scheme registered exclusively for imported Anki cards.
pub const ANKI_SCHEME: &str = "neondojo-anki";

/// Strict Content Security Policy enforced on every rendered Anki card face.
pub const ANKI_CARD_CSP: &str = "default-src 'none'; img-src 'self' data:; media-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'";

/// Content Security Policy applied to binary Anki media responses.
pub const ANKI_MEDIA_CSP: &str = "default-src 'none'; sandbox";

/// Front or back side of an imported third-party Anki card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnkiCardSide {
    Front,
    Back,
}

impl AnkiCardSide {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            _ => None,
        }
    }
}

/// Request identifying an imported Anki card face to render in [`AnkiCardFaceView`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCardFaceRequest {
    pub card_id: Uuid,
    pub side: String,
}

impl AnkiCardFaceRequest {
    pub fn new(card_id: Uuid, side: AnkiCardSide) -> Self {
        Self {
            card_id,
            side: side.as_str().to_string(),
        }
    }

    pub fn side_enum(&self) -> Option<AnkiCardSide> {
        AnkiCardSide::parse(&self.side)
    }

    /// Returns the canonical custom-scheme URL for this card face.
    pub fn scheme_url(&self) -> Option<String> {
        let side = self.side_enum()?;
        Some(format!(
            "{ANKI_SCHEME}://card/{}/{}",
            self.card_id,
            side.as_str()
        ))
    }
}

/// Parsed route handled by the `neondojo-anki://` `WKURLSchemeHandler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnkiSchemeRoute {
    /// `neondojo-anki://card/{card_id}/{front|back}`
    Card { card_id: Uuid, side: AnkiCardSide },
    /// `neondojo-anki://media/{package}/{name}` where `package` is a 64-char hex SHA-256.
    Media { package: String, name: String },
}

/// Returns `true` if `value` is a valid 64-character ASCII hexadecimal SHA-256 fingerprint.
pub fn is_valid_package_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Decodes RFC-3986 percent-encoded UTF-8 sequences in a single path segment.
fn percent_decode_segment(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[idx + 1])?;
            let lo = hex_nibble(bytes[idx + 2])?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Parses and strictly validates a `neondojo-anki://` URL.
///
/// Accepts only:
/// - `neondojo-anki://card/{uuid}/{front|back}`
/// - `neondojo-anki://media/{64-hex-sha256}/{filename}` (rejecting traversal, slashes, and control characters)
pub fn parse_anki_scheme_url(url: &str) -> Option<AnkiSchemeRoute> {
    let trimmed = url.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    if !scheme.eq_ignore_ascii_case(ANKI_SCHEME) {
        return None;
    }

    // Strip query string and fragment before routing.
    let without_fragment = rest.split('#').next().unwrap_or(rest);
    let authority_and_path = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

    let (host, path) = authority_and_path.split_once('/')?;
    if path.is_empty() {
        return None;
    }

    match host.to_ascii_lowercase().as_str() {
        "card" => {
            let mut parts = path.split('/');
            let id_str = parts.next()?;
            let side_str = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let card_id = Uuid::parse_str(id_str).ok()?;
            let side = AnkiCardSide::parse(side_str)?;
            Some(AnkiSchemeRoute::Card { card_id, side })
        }
        "media" => {
            let (package, raw_name) = path.split_once('/')?;
            if raw_name.is_empty() || raw_name.contains('/') {
                return None;
            }
            if !is_valid_package_hash(package) {
                return None;
            }
            let name = percent_decode_segment(raw_name)?;
            if name.is_empty()
                || name.len() > 255
                || name == "."
                || name == ".."
                || name.contains('/')
                || name.contains('\\')
                || name.contains("..")
                || name.chars().any(char::is_control)
            {
                return None;
            }
            Some(AnkiSchemeRoute::Media {
                package: package.to_ascii_lowercase(),
                name,
            })
        }
        _ => None,
    }
}

/// Rewrites `<base href="/api/anki-media/{package}/" ...>` to the custom
/// `neondojo-anki://media/{package}/` scheme, enforces the strict CSP `<meta>`
/// tag, and synchronizes light/dark color-scheme and Anki `nightMode` styling.
pub fn build_anki_face_document(raw_html: &str, dark_mode: bool) -> String {
    let mut html = rewrite_media_urls(raw_html);
    let csp_meta =
        format!(r#"<meta http-equiv="Content-Security-Policy" content="{ANKI_CARD_CSP}">"#);
    let theme_style = if dark_mode {
        r#"<style data-neondojo-theme="dark">:root{color-scheme:dark;}html,body{background:#1e242d;color:#e5eaf2;margin:0;padding:12px;}</style>"#
    } else {
        r#"<style data-neondojo-theme="light">:root{color-scheme:light;}html,body{background:#ffffff;color:#202833;margin:0;padding:12px;}</style>"#
    };

    if let Some(head_close) = find_ascii_case_insensitive(&html, "</head>") {
        let injection = format!("{csp_meta}{theme_style}");
        if let Some(head_open) = find_ascii_case_insensitive(&html, "<head>") {
            let insert_at = head_open + "<head>".len();
            // Place CSP first inside <head> (immediately after <head>), and theme defaults before </head>.
            html.insert_str(head_close, theme_style);
            html.insert_str(insert_at, &csp_meta);
        } else {
            html.insert_str(head_close, &injection);
        }
    } else {
        let body_class = if dark_mode {
            "card mac nightMode night_mode"
        } else {
            "card mac"
        };
        return format!(
            r#"<!doctype html><html><head><meta charset="utf-8">{csp_meta}{theme_style}</head><body class="{body_class}">{html}</body></html>"#
        );
    }

    if dark_mode {
        html = inject_dark_body_classes(&html);
    }

    html
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

fn rewrite_media_urls(raw_html: &str) -> String {
    let mut out = raw_html.to_string();

    // Rewrite backend's `<base href="/api/anki-media/{package}/" target="_blank">` (or without target)
    // to `<base href="neondojo-anki://media/{package}/">`. Backend only emits one `<base>`.
    if let Some(base_start) = find_ascii_case_insensitive(&out, "<base ")
        && let Some(rel_end) = out[base_start..].find('>')
    {
        let base_end = base_start + rel_end + 1;
        let tag = &out[base_start..base_end];
        if let Some(pkg) = extract_package_from_base_tag(tag) {
            let replacement = format!(r#"<base href="{ANKI_SCHEME}://media/{pkg}/">"#);
            out.replace_range(base_start..base_end, &replacement);
        }
    }

    // Also rewrite any explicit `/api/anki-media/` links inside attributes.
    out = out.replace(
        r#"href="/api/anki-media/"#,
        &format!(r#"href="{ANKI_SCHEME}://media/"#),
    );
    out = out.replace(
        r#"src="/api/anki-media/"#,
        &format!(r#"src="{ANKI_SCHEME}://media/"#),
    );
    out = out.replace(
        "src='/api/anki-media/",
        &format!("src='{ANKI_SCHEME}://media/"),
    );

    out
}

fn extract_package_from_base_tag(tag: &str) -> Option<String> {
    let marker = "/api/anki-media/";
    let start = tag.find(marker)? + marker.len();
    let rest = &tag[start..];
    let pkg = rest.split(['/', '"', '\'']).next()?;
    if is_valid_package_hash(pkg) {
        Some(pkg.to_ascii_lowercase())
    } else {
        None
    }
}

fn inject_dark_body_classes(html: &str) -> String {
    let Some(body_start) = find_ascii_case_insensitive(html, "<body") else {
        return html.to_string();
    };
    let Some(rel_end) = html[body_start..].find('>') else {
        return html.to_string();
    };
    let body_end = body_start + rel_end + 1;
    let body_tag = &html[body_start..body_end];
    if body_tag.contains("nightMode") {
        return html.to_string();
    }

    let mut result = html.to_string();
    if let Some(class_idx) = find_ascii_case_insensitive(body_tag, "class=\"") {
        let insert_pos = body_start + class_idx + "class=\"".len();
        result.insert_str(insert_pos, "nightMode night_mode ");
    } else {
        result.insert_str(body_end - 1, r#" class="nightMode night_mode""#);
    }
    result
}

// ── Native macOS WKWebView + WKURLSchemeHandler implementation ───────────────

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        ANKI_CARD_CSP, ANKI_MEDIA_CSP, ANKI_SCHEME, AnkiSchemeRoute, build_anki_face_document,
        parse_anki_scheme_url,
    };
    use neondojo::facade::BackendFacade;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{
        AnyThread, DeclaredClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    };
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSApplication, NSView,
    };
    use objc2_foundation::{
        NSData, NSDictionary, NSError, NSHTTPURLResponse, NSInteger, NSPoint, NSRect, NSSize,
        NSString, NSURL, NSURLRequest, NSURLResponse,
    };
    use objc2_web_kit::{WKURLSchemeHandler, WKURLSchemeTask, WKWebView, WKWebViewConfiguration};
    use std::collections::HashSet;
    use std::ffi::{c_uchar, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::runtime::Handle;

    unsafe extern "C" {
        static NSApp: *const AnyObject;
        static _dispatch_main_q: c_uchar;
        fn dispatch_async_f(
            queue: *mut c_void,
            context: *mut c_void,
            work: unsafe extern "C" fn(*mut c_void),
        );
    }

    struct SchemeHandlerIvars {
        facade: BackendFacade,
        tokio: Handle,
        dark_mode: Arc<AtomicBool>,
        stopped: Arc<Mutex<HashSet<usize>>>,
    }

    /// Raw task pointer wrapper kept alive until the main-thread callback consumes it.
    struct SendableTask {
        task: Retained<ProtocolObject<dyn WKURLSchemeTask>>,
        url: Retained<NSURL>,
        task_key: usize,
        stopped: Arc<Mutex<HashSet<usize>>>,
    }

    // SAFETY: `task` and `url` are ONLY dereferenced back on the macOS main thread inside
    // `deliver_scheme_response_on_main_thread`, never on the Tokio worker thread.
    unsafe impl Send for SendableTask {}

    enum SchemeFetchOutcome {
        Ok {
            status: NSInteger,
            content_type: String,
            csp: &'static str,
            body: Vec<u8>,
        },
        Err {
            code: NSInteger,
            message: String,
        },
    }

    struct MainThreadDelivery {
        handle: SendableTask,
        outcome: SchemeFetchOutcome,
    }

    unsafe extern "C" fn deliver_scheme_response_on_main_thread(ctx: *mut c_void) {
        if ctx.is_null() {
            return;
        }
        // SAFETY: `ctx` was allocated with `Box::into_raw` in `webView_startURLSchemeTask`.
        let delivery = unsafe { Box::from_raw(ctx.cast::<MainThreadDelivery>()) };
        let MainThreadDelivery { handle, outcome } = *delivery;

        let was_stopped = handle
            .stopped
            .lock()
            .map(|mut set| set.remove(&handle.task_key))
            .unwrap_or(false);
        if was_stopped {
            return;
        }

        match outcome {
            SchemeFetchOutcome::Ok {
                status,
                content_type,
                csp,
                body,
            } => {
                let http_ver = NSString::from_str("HTTP/1.1");
                let k_ct = NSString::from_str("Content-Type");
                let v_ct = NSString::from_str(&content_type);
                let k_csp = NSString::from_str("Content-Security-Policy");
                let v_csp = NSString::from_str(csp);
                let k_nosniff = NSString::from_str("X-Content-Type-Options");
                let v_nosniff = NSString::from_str("nosniff");
                let k_cache = NSString::from_str("Cache-Control");
                let v_cache = NSString::from_str("no-store");

                let headers = NSDictionary::from_slices(
                    &[&*k_ct, &*k_csp, &*k_nosniff, &*k_cache],
                    &[&*v_ct, &*v_csp, &*v_nosniff, &*v_cache],
                );
                let response = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
                    NSHTTPURLResponse::alloc(),
                    &handle.url,
                    status,
                    Some(&http_ver),
                    Some(&headers),
                );
                if let Some(http_resp) = response {
                    let ns_resp: &NSURLResponse = &http_resp;
                    let data = NSData::with_bytes(&body);
                    // SAFETY: Called on main thread for an active, unstopped `WKURLSchemeTask`.
                    unsafe {
                        handle.task.didReceiveResponse(ns_resp);
                        handle.task.didReceiveData(&data);
                        handle.task.didFinish();
                    }
                } else {
                    fail_task(&handle.task, -1, "Failed to build HTTP response");
                }
            }
            SchemeFetchOutcome::Err { code, message } => {
                fail_task(&handle.task, code, &message);
            }
        }
    }

    fn fail_task(task: &ProtocolObject<dyn WKURLSchemeTask>, code: NSInteger, _message: &str) {
        let domain = NSString::from_str("org.neondojo.anki-scheme");
        let err = unsafe { NSError::errorWithDomain_code_userInfo(&domain, code, None) };
        unsafe {
            task.didFailWithError(&err);
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = SchemeHandlerIvars]
        struct NeondojoAnkiSchemeHandler;

        unsafe impl NSObjectProtocol for NeondojoAnkiSchemeHandler {}

        unsafe impl WKURLSchemeHandler for NeondojoAnkiSchemeHandler {
            #[unsafe(method(webView:startURLSchemeTask:))]
            unsafe fn web_view_start_url_scheme_task(
                &self,
                _web_view: &WKWebView,
                url_scheme_task: &ProtocolObject<dyn WKURLSchemeTask>,
            ) {
                let request = unsafe { url_scheme_task.request() };
                let Some(url) = request.URL() else {
                    fail_task(url_scheme_task, 400, "Missing request URL");
                    return;
                };
                let url_str = url
                    .absoluteString()
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let Some(route) = parse_anki_scheme_url(&url_str) else {
                    fail_task(url_scheme_task, 404, "Unsupported neondojo-anki route");
                    return;
                };

                let task_key = std::ptr::from_ref(url_scheme_task) as usize;
                let ivars = self.ivars();
                if let Ok(mut set) = ivars.stopped.lock() {
                    set.remove(&task_key);
                }

                let retained_task: Retained<ProtocolObject<dyn WKURLSchemeTask>> =
                    unsafe { Retained::retain(std::ptr::from_ref(url_scheme_task).cast_mut()) }
                        .expect("retain WKURLSchemeTask");
                let sendable = SendableTask {
                    task: retained_task,
                    url,
                    task_key,
                    stopped: Arc::clone(&ivars.stopped),
                };

                let facade = ivars.facade.clone();
                let dark_mode = Arc::clone(&ivars.dark_mode);
                let tokio_handle = ivars.tokio.clone();

                tokio_handle.clone().spawn(async move {
                    let handle_inner = tokio_handle.clone();
                    let outcome = tokio::task::spawn_blocking(move || {
                        handle_inner.block_on(async move {
                            match route {
                                AnkiSchemeRoute::Card { card_id, side } => {
                                    match facade
                                        .get_anki_card_face_html(card_id, side.as_str())
                                        .await
                                    {
                                        Ok(raw_html) => {
                                            let dark = dark_mode.load(Ordering::Relaxed);
                                            let doc = build_anki_face_document(&raw_html, dark);
                                            SchemeFetchOutcome::Ok {
                                                status: 200,
                                                content_type: "text/html; charset=utf-8".into(),
                                                csp: ANKI_CARD_CSP,
                                                body: doc.into_bytes(),
                                            }
                                        }
                                        Err(err) => SchemeFetchOutcome::Err {
                                            code: 404,
                                            message: err.body.error,
                                        },
                                    }
                                }
                                AnkiSchemeRoute::Media { package, name } => {
                                    match facade.get_anki_media_blob(&package, &name).await {
                                        Ok(blob) => SchemeFetchOutcome::Ok {
                                            status: 200,
                                            content_type: blob.content_type,
                                            csp: ANKI_MEDIA_CSP,
                                            body: blob.bytes,
                                        },
                                        Err(err) => SchemeFetchOutcome::Err {
                                            code: 404,
                                            message: err.body.error,
                                        },
                                    }
                                }
                            }
                        })
                    })
                    .await
                    .unwrap_or_else(|err| SchemeFetchOutcome::Err {
                        code: 500,
                        message: err.to_string(),
                    });

                    let boxed = Box::new(MainThreadDelivery {
                        handle: sendable,
                        outcome,
                    });
                    // SAFETY: `_dispatch_main_q` is Apple's static main GCD queue; `boxed` is
                    // reclaimed once inside `deliver_scheme_response_on_main_thread`.
                    unsafe {
                        dispatch_async_f(
                            (&raw const _dispatch_main_q).cast_mut().cast::<c_void>(),
                            Box::into_raw(boxed).cast::<c_void>(),
                            deliver_scheme_response_on_main_thread,
                        );
                    }
                });
            }

            #[unsafe(method(webView:stopURLSchemeTask:))]
            unsafe fn web_view_stop_url_scheme_task(
                &self,
                _web_view: &WKWebView,
                url_scheme_task: &ProtocolObject<dyn WKURLSchemeTask>,
            ) {
                let task_key = std::ptr::from_ref(url_scheme_task) as usize;
                if let Ok(mut set) = self.ivars().stopped.lock() {
                    set.insert(task_key);
                }
            }
        }
    );

    impl NeondojoAnkiSchemeHandler {
        fn new(
            mtm: MainThreadMarker,
            facade: BackendFacade,
            tokio: Handle,
            dark_mode: Arc<AtomicBool>,
        ) -> Retained<Self> {
            let this = mtm.alloc::<Self>().set_ivars(SchemeHandlerIvars {
                facade,
                tokio,
                dark_mode,
                stopped: Arc::new(Mutex::new(HashSet::new())),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Owns a live macOS `WKWebView` attached as a child `NSView` inside the active GPUI window.
    pub(super) struct NativeAnkiWebViewHost {
        web_view: Retained<WKWebView>,
        _handler: Retained<NeondojoAnkiSchemeHandler>,
        dark_mode: Arc<AtomicBool>,
        attached: bool,
        loaded_url: Option<String>,
        last_frame: Option<(f64, f64, f64, f64)>,
    }

    impl NativeAnkiWebViewHost {
        /// Attempts to create and attach a `WKWebView` backed by `facade` on the macOS main thread.
        ///
        /// Returns `None` in headless tests (when `NSApp` is uninitialized, not on main thread,
        /// or no `NSWindow` content view exists).
        pub(super) fn try_new(
            facade: BackendFacade,
            tokio: Handle,
            dark_mode: bool,
        ) -> Option<Self> {
            // Never initialize AppKit in headless test processes where NSApplication was never launched.
            if unsafe { NSApp.is_null() } {
                return None;
            }
            let mtm = MainThreadMarker::new()?;
            let app = NSApplication::sharedApplication(mtm);
            let window = app.keyWindow().or_else(|| app.mainWindow())?;
            let content_view = window.contentView()?;

            let dark_flag = Arc::new(AtomicBool::new(dark_mode));
            let handler =
                NeondojoAnkiSchemeHandler::new(mtm, facade, tokio, Arc::clone(&dark_flag));

            let config = unsafe { WKWebViewConfiguration::new(mtm) };
            let scheme = NSString::from_str(ANKI_SCHEME);
            let handler_proto = ProtocolObject::from_ref(&*handler);
            unsafe {
                config.setURLSchemeHandler_forURLScheme(Some(handler_proto), &scheme);
            }

            let zero_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
            let web_view = unsafe {
                WKWebView::initWithFrame_configuration(mtm.alloc::<WKWebView>(), zero_rect, &config)
            };
            apply_appearance(&web_view, dark_mode);
            content_view.addSubview(&web_view);

            Some(Self {
                web_view,
                _handler: handler,
                dark_mode: dark_flag,
                attached: true,
                loaded_url: None,
                last_frame: None,
            })
        }

        pub(super) fn load_url(&mut self, url_str: &str) {
            if self.loaded_url.as_deref() == Some(url_str) {
                return;
            }
            let ns_str = NSString::from_str(url_str);
            if let Some(url) = NSURL::URLWithString(&ns_str) {
                let req = NSURLRequest::requestWithURL(&url);
                let _ = unsafe { self.web_view.loadRequest(&req) };
                self.loaded_url = Some(url_str.to_string());
            }
        }

        pub(super) fn set_dark_mode(&mut self, dark_mode: bool) {
            let prev = self.dark_mode.swap(dark_mode, Ordering::Relaxed);
            if prev != dark_mode {
                apply_appearance(&self.web_view, dark_mode);
                if let Some(url) = self.loaded_url.clone() {
                    self.loaded_url = None;
                    self.load_url(&url);
                }
            }
        }

        pub(super) fn set_hidden(&self, hidden: bool) {
            self.web_view.setHidden(hidden);
        }

        pub(super) fn sync_frame(&mut self, x: f64, y: f64, width: f64, height: f64) {
            if !self.attached || width <= 1.0 || height <= 1.0 {
                self.web_view.setHidden(true);
                return;
            }
            let Some(superview) = (unsafe { self.web_view.superview() }) else {
                return;
            };
            let ns_y = if superview.isFlipped() {
                y
            } else {
                let parent_h = superview.bounds().size.height;
                (parent_h - y - height).max(0.0)
            };
            let next_frame = (x, ns_y, width, height);
            if self.last_frame != Some(next_frame) {
                let rect = NSRect::new(NSPoint::new(x, ns_y), NSSize::new(width, height));
                self.web_view.setFrame(rect);
                self.last_frame = Some(next_frame);
            }
            self.web_view.setHidden(false);
        }

        pub(super) fn teardown(&mut self) {
            if self.attached {
                self.web_view.setHidden(true);
                self.web_view.removeFromSuperview();
                self.attached = false;
                self.loaded_url = None;
                self.last_frame = None;
            }
        }
    }

    impl Drop for NativeAnkiWebViewHost {
        fn drop(&mut self) {
            if MainThreadMarker::new().is_some() {
                self.teardown();
            }
        }
    }

    fn apply_appearance(view: &NSView, dark_mode: bool) {
        let name = unsafe {
            if dark_mode {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        let appearance = NSAppearance::appearanceNamed(name);
        view.setAppearance(appearance.as_deref());
    }
}

/// GPUI view hosting the scoped `WKWebView` strictly for third-party HTML/JS
/// Anki cards (`card.kind == "anki"`).
///
/// Manages `WKWebView` lifecycle, bounds synchronization, dark/light theme sync,
/// and clean teardown on unmount or card change. Falls back to a clean,
/// testable GPUI card-face container in headless tests (`#[gpui_kit::test]`) or
/// when disconnected from PostgreSQL.
pub struct AnkiCardFaceView {
    request: Option<AnkiCardFaceRequest>,
    visible: bool,
    dark_mode: bool,
    #[cfg(target_os = "macos")]
    native_host: Option<macos::NativeAnkiWebViewHost>,
}

impl AnkiCardFaceView {
    pub fn new(request: Option<AnkiCardFaceRequest>, cx: &mut Context<Self>) -> Self {
        let dark_mode = NeonTheme::current_palette(cx).mode.is_dark();
        Self {
            request,
            visible: true,
            dark_mode,
            #[cfg(target_os = "macos")]
            native_host: None,
        }
    }

    pub fn request(&self) -> Option<&AnkiCardFaceRequest> {
        self.request.as_ref()
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Switches the displayed Anki card face (`front` or `back`) or tears down when `None`.
    pub fn set_request(&mut self, request: Option<AnkiCardFaceRequest>, cx: &mut Context<Self>) {
        if self.request == request {
            return;
        }
        let card_changed = match (&self.request, &request) {
            (Some(a), Some(b)) => a.card_id != b.card_id,
            _ => true,
        };
        self.request = request;

        #[cfg(target_os = "macos")]
        {
            if self.request.is_none() || card_changed {
                if let Some(mut host) = self.native_host.take() {
                    host.teardown();
                }
            } else if let (Some(req), Some(host)) = (&self.request, &mut self.native_host)
                && let Some(url) = req.scheme_url()
            {
                host.load_url(&url);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = card_changed;

        cx.notify();
    }

    /// Shows or hides the underlying native `WKWebView` so it never bleeds over
    /// sibling tabs, modals, or hidden workspaces.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        #[cfg(target_os = "macos")]
        if let Some(host) = &self.native_host {
            host.set_hidden(!visible);
        }
        cx.notify();
    }

    /// Explicitly removes any attached native `WKWebView` from the window hierarchy.
    pub fn teardown(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(mut host) = self.native_host.take() {
            host.teardown();
        }
    }

    #[cfg(target_os = "macos")]
    fn ensure_native_host(&mut self, cx: &App) {
        if !self.visible {
            return;
        }
        let Some(req) = &self.request else {
            return;
        };
        let Some(url) = req.scheme_url() else {
            return;
        };
        let dark_mode = NeonTheme::current_palette(cx).mode.is_dark();
        self.dark_mode = dark_mode;

        if self.native_host.is_none() {
            let facade = cx
                .try_global::<DesktopAppContext>()
                .and_then(|c| c.facade());
            let tokio = cx
                .try_global::<crate::tokio_bridge::TokioBridge>()
                .map(|_| crate::tokio_bridge::TokioBridge::handle(cx));
            if let (Some(facade), Some(tokio)) = (facade, tokio) {
                self.native_host = macos::NativeAnkiWebViewHost::try_new(facade, tokio, dark_mode);
            }
        }

        if let Some(host) = &mut self.native_host {
            host.set_dark_mode(dark_mode);
            host.load_url(&url);
        }
    }
}

impl Drop for AnkiCardFaceView {
    fn drop(&mut self) {
        self.teardown();
    }
}

impl Render for AnkiCardFaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        self.dark_mode = p.mode.is_dark();

        #[cfg(target_os = "macos")]
        self.ensure_native_host(cx);

        let entity = cx.entity();
        let visible = self.visible;
        let has_native_host = {
            #[cfg(target_os = "macos")]
            {
                self.native_host.is_some()
            }
            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        };

        let side_label = self
            .request
            .as_ref()
            .and_then(AnkiCardFaceRequest::side_enum)
            .map(|side| match side {
                AnkiCardSide::Front => I18n::t(locale, "learning", "question"),
                AnkiCardSide::Back => I18n::t(locale, "learning", "solution"),
            })
            .unwrap_or("Anki");
        let scheme_url = self
            .request
            .as_ref()
            .and_then(AnkiCardFaceRequest::scheme_url)
            .unwrap_or_else(|| format!("{ANKI_SCHEME}://card/none"));

        let bounds_sync = canvas(
            move |bounds, _window, cx| {
                #[cfg(target_os = "macos")]
                {
                    let x = f64::from(f32::from(bounds.origin.x));
                    let y = f64::from(f32::from(bounds.origin.y));
                    let w = f64::from(f32::from(bounds.size.width));
                    let h = f64::from(f32::from(bounds.size.height));
                    cx.update_entity(&entity, |view, _| {
                        if let Some(host) = &mut view.native_host {
                            if visible {
                                host.sync_frame(x, y, w, h);
                            } else {
                                host.set_hidden(true);
                            }
                        }
                    });
                }
                #[cfg(not(target_os = "macos"))]
                let _ = (bounds, &entity, visible, cx);
            },
            |_bounds, (), _window, _cx| {},
        )
        .size_full();

        div()
            .id("anki-card-face-view")
            .test_support()
            .relative()
            .w_full()
            .min_h(px(220.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(
                div()
                    .v_flex()
                    .size_full()
                    .p(px(14.0))
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_label()
                                    .text_color(p.text)
                                    .child(SharedString::from(format!(
                                        "{} · {side_label}",
                                        I18n::t(locale, "card-search", "anki")
                                    ))),
                            )
                            .child(
                                div()
                                    .text_micro()
                                    .mono_font()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(p.surface_alt)
                                    .text_color(p.muted)
                                    .child(if has_native_host {
                                        "WKWebView · CSP"
                                    } else {
                                        "Anki HTML · CSP"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_meta()
                            .mono_font()
                            .text_color(p.muted)
                            .truncate()
                            .child(scheme_url),
                    ),
            )
            .child(div().absolute().inset_0().child(bounds_sync))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure_app;
    use crate::local_state::LocalUiState;
    use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};

    const VALID_PKG: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_valid_card_and_media_scheme_urls() {
        let id = Uuid::new_v4();
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://card/{id}/front")),
            Some(AnkiSchemeRoute::Card {
                card_id: id,
                side: AnkiCardSide::Front,
            })
        );
        assert_eq!(
            parse_anki_scheme_url(&format!("NEONDOJO-ANKI://card/{id}/BACK?v=1#top")),
            Some(AnkiSchemeRoute::Card {
                card_id: id,
                side: AnkiCardSide::Back,
            })
        );
        assert_eq!(
            parse_anki_scheme_url(&format!(
                "neondojo-anki://media/{VALID_PKG}/board%20shot%201.png"
            )),
            Some(AnkiSchemeRoute::Media {
                package: VALID_PKG.to_string(),
                name: "board shot 1.png".to_string(),
            })
        );
    }

    #[test]
    fn rejects_invalid_schemes_uuids_packages_and_path_traversal() {
        let id = Uuid::new_v4();
        assert_eq!(parse_anki_scheme_url("https://example.com/a/b"), None);
        assert_eq!(parse_anki_scheme_url("file:///etc/passwd"), None);
        assert_eq!(
            parse_anki_scheme_url("neondojo-anki://card/not-a-uuid/front"),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://card/{id}/sideways")),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://card/{id}/front/extra")),
            None
        );
        // Package must be exact 64-char hex SHA-256
        assert_eq!(
            parse_anki_scheme_url("neondojo-anki://media/short/img.png"),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!(
                "neondojo-anki://media/{}z/img.png",
                &VALID_PKG[..63]
            )),
            None
        );
        // Path traversal and nested slashes rejected
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://media/{VALID_PKG}/../secret.png")),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!(
                "neondojo-anki://media/{VALID_PKG}/%2e%2e%2fsecret"
            )),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://media/{VALID_PKG}/a/b.png")),
            None
        );
        assert_eq!(
            parse_anki_scheme_url(&format!("neondojo-anki://media/{VALID_PKG}/")),
            None
        );
        assert_eq!(
            parse_anki_scheme_url("neondojo-anki://unknown/foo/bar"),
            None
        );
    }

    #[test]
    fn builds_strict_csp_and_rewrites_base_href_for_light_and_dark_modes() {
        let raw = format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><base href="/api/anki-media/{VALID_PKG}/" target="_blank"><style>.card{{color:black;}}</style></head><body class="card card1 mac"><img src="pos.png"><img src="/api/anki-media/{VALID_PKG}/extra.png"></body></html>"#
        );

        let light = build_anki_face_document(&raw, false);
        assert!(light.contains(&format!(
            r#"<meta http-equiv="Content-Security-Policy" content="{ANKI_CARD_CSP}">"#
        )));
        assert!(light.contains(&format!(
            r#"<base href="neondojo-anki://media/{VALID_PKG}/">"#
        )));
        assert!(!light.contains("/api/anki-media/"));
        assert!(light.contains("color-scheme:light"));
        assert!(!light.contains("nightMode"));

        let dark = build_anki_face_document(&raw, true);
        assert!(dark.contains(&format!(
            r#"<meta http-equiv="Content-Security-Policy" content="{ANKI_CARD_CSP}">"#
        )));
        assert!(dark.contains(&format!(
            r#"<base href="neondojo-anki://media/{VALID_PKG}/">"#
        )));
        assert!(dark.contains("color-scheme:dark"));
        assert!(dark.contains("nightMode night_mode"));

        // Fragment without <head> is wrapped with strict CSP too
        let fragment = build_anki_face_document("<p>Hello</p>", true);
        assert!(fragment.contains(ANKI_CARD_CSP));
        assert!(fragment.contains("nightMode"));
        assert!(fragment.contains("<p>Hello</p>"));
    }

    #[gpui_kit::test]
    async fn anki_card_face_view_renders_and_switches_cleanly_in_headless_mode(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let card_a = Uuid::new_v4();
        let card_b = Uuid::new_v4();

        let (window, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_window, cx| {
                    cx.new(|cx| {
                        AnkiCardFaceView::new(
                            Some(AnkiCardFaceRequest::new(card_a, AnkiCardSide::Front)),
                            cx,
                        )
                    })
                })
            })
            .expect("open headless window for AnkiCardFaceView");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("anki-card-face-view").visible());
            assert_eq!(
                view.read(cx)
                    .request()
                    .and_then(AnkiCardFaceRequest::scheme_url),
                Some(format!("neondojo-anki://card/{card_a}/front"))
            );

            // Switch to back face
            view.update(cx, |v, cx| {
                v.set_request(
                    Some(AnkiCardFaceRequest::new(card_a, AnkiCardSide::Back)),
                    cx,
                );
            });
            window.render_frame(cx);
            assert_eq!(
                view.read(cx)
                    .request()
                    .and_then(AnkiCardFaceRequest::scheme_url),
                Some(format!("neondojo-anki://card/{card_a}/back"))
            );

            // Switch to different card and toggle visibility
            view.update(cx, |v, cx| {
                v.set_request(
                    Some(AnkiCardFaceRequest::new(card_b, AnkiCardSide::Front)),
                    cx,
                );
                v.set_visible(false, cx);
            });
            window.render_frame(cx);
            assert!(!view.read(cx).is_visible());

            // Clear request (teardown)
            view.update(cx, |v, cx| {
                v.set_request(None, cx);
            });
            window.render_frame(cx);
            assert!(view.read(cx).request().is_none());
        })
        .expect("update headless window for AnkiCardFaceView");
    }
}
