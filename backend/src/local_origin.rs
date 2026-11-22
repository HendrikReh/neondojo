//! Request guard for the loopback-only API.
//!
//! NeonDojo listens on a loopback address and has no authentication, so the
//! browser is the only party that can be tricked into calling it. This guard
//! rejects the two ways a foreign web page can reach the API:
//!
//! - **DNS rebinding:** a page on an attacker's domain re-resolves that domain
//!   to `127.0.0.1` and then reads the API as "same origin". Its requests carry
//!   the attacker's domain in `Host`, so every request whose `Host` (or HTTP/2
//!   authority) is not a loopback name is rejected.
//! - **Cross-site requests:** a page can send form-encoded, multipart or
//!   `text/plain` POSTs without a CORS preflight. Unsafe methods therefore
//!   need a loopback or Tauri `Origin`; without `Origin`, a browser's
//!   `Sec-Fetch-Site: cross-site` is rejected. `Origin` is checked first
//!   because Tauri's WebView reports its custom-scheme requests as cross-site.
//!
//! Requests without these headers (command-line clients, in-process tests)
//! pass: only browsers send them, and only browsers need the protection.
use crate::api::ApiError;
use axum::{
    extract::Request,
    http::{HeaderMap, Method, StatusCode, Uri, header},
    middleware::Next,
    response::Response,
};

/// Loopback host names accepted in `Host`, compared without the port.
/// `*.localhost` is deliberately not accepted: not every resolver pins those
/// names to loopback, so a rebinding domain could use them.
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// Origins of the Tauri shell's WebView (v1 on macOS, v2 on Windows/Linux).
const TAURI_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// Axum middleware that rejects requests from a foreign site with HTTP 403.
pub(crate) async fn guard(request: Request, next: Next) -> Result<Response, ApiError> {
    check(request.method(), request.uri(), request.headers())?;
    Ok(next.run(request).await)
}

fn check(method: &Method, uri: &Uri, headers: &HeaderMap) -> Result<(), ApiError> {
    let host = headers.get(header::HOST).map(|value| value.to_str().ok());
    let authority = uri.authority().map(|authority| Some(authority.as_str()));
    for name in [host, authority].into_iter().flatten() {
        if !name.is_some_and(is_loopback_authority) {
            return Err(forbidden(
                "Requests must address NeonDojo via a loopback host",
            ));
        }
    }
    if is_safe(method) {
        return Ok(());
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        return if origin.to_str().is_ok_and(is_allowed_origin) {
            Ok(())
        } else {
            Err(forbidden(
                "Cross-origin requests to NeonDojo are not allowed",
            ))
        };
    }
    let cross_site = headers
        .get("sec-fetch-site")
        .is_some_and(|site| site.as_bytes().eq_ignore_ascii_case(b"cross-site"));
    if cross_site {
        return Err(forbidden("Cross-site requests to NeonDojo are not allowed"));
    }
    Ok(())
}

fn forbidden(message: &str) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, message.into())
}

/// Methods that must not change state, so a cross-site page may send them.
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// `host[:port]` with a loopback host and an optional numeric port.
fn is_loopback_authority(authority: &str) -> bool {
    let (host, port) = split_port(authority);
    port.is_none_or(is_port)
        && LOOPBACK_HOSTS
            .iter()
            .any(|loopback| host.eq_ignore_ascii_case(loopback))
}

/// `http://` plus a loopback authority, or one of the Tauri origins. `null`
/// (sandboxed frames, `file:` pages) and every other origin are refused.
fn is_allowed_origin(origin: &str) -> bool {
    if TAURI_ORIGINS
        .iter()
        .any(|tauri| origin.eq_ignore_ascii_case(tauri))
    {
        return true;
    }
    origin
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("http://"))
        .is_some_and(|_| is_loopback_authority(&origin[7..]))
}

/// Splits `host:port`, keeping an IPv6 literal's brackets in the host.
fn split_port(authority: &str) -> (&str, Option<&str>) {
    let separator = if authority.starts_with('[') {
        authority
            .find(']')
            .and_then(|end| authority[end + 1..].starts_with(':').then_some(end + 1))
    } else {
        authority.rfind(':')
    };
    match separator {
        Some(index) => (&authority[..index], Some(&authority[index + 1..])),
        None => (authority, None),
    }
}

fn is_port(port: &str) -> bool {
    !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn status(method: Method, uri: &str, headers: &[(&str, &str)]) -> Option<StatusCode> {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.append(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        check(&method, &uri.parse().unwrap(), &map)
            .err()
            .map(|error| error.0)
    }

    #[test]
    fn host_must_be_loopback() {
        for host in [
            "localhost",
            "LOCALHOST:8787",
            "127.0.0.1",
            "127.0.0.1:54321",
            "[::1]",
            "[::1]:5173",
        ] {
            assert_eq!(
                status(Method::GET, "/api/health", &[("host", host)]),
                None,
                "{host}"
            );
        }
        for host in [
            "rebind.example",
            "rebind.example:8787",
            "127.0.0.1.nip.io",
            "localhost.example.com",
            "app.localhost",
            "127.0.0.2",
            "0.0.0.0:8787",
            "[::2]:8787",
            "[::1]8787",
            "localhost:",
            "localhost:port",
            "",
        ] {
            assert_eq!(
                status(Method::GET, "/api/health", &[("host", host)]),
                Some(StatusCode::FORBIDDEN),
                "{host}"
            );
        }
        assert_eq!(status(Method::GET, "/api/health", &[]), None);
        assert_eq!(
            status(Method::GET, "http://rebind.example/api/health", &[]),
            Some(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            status(Method::GET, "http://[::1]:8787/api/health", &[]),
            None
        );
    }

    #[test]
    fn unsafe_methods_need_a_local_origin() {
        for origin in [
            "http://localhost",
            "http://localhost:5173",
            "http://127.0.0.1:61234",
            "http://[::1]:8787",
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
        ] {
            for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
                assert_eq!(
                    status(method, "/api/jobs", &[("origin", origin)]),
                    None,
                    "{origin}"
                );
            }
        }
        for origin in [
            "null",
            "https://example.com",
            "http://localhost.example.com",
            "https://localhost:8787",
            "http://app.localhost:8787",
            "file://",
            "http://127.0.0.1:8787/",
            "tauri://evil",
            "",
        ] {
            for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
                assert_eq!(
                    status(method, "/api/jobs", &[("origin", origin)]),
                    Some(StatusCode::FORBIDDEN),
                    "{origin}"
                );
            }
            // Reads are never blocked by Origin; CORS keeps their responses private.
            assert_eq!(
                status(Method::GET, "/api/jobs", &[("origin", origin)]),
                None
            );
        }
    }

    #[test]
    fn fetch_metadata_is_checked_only_without_origin() {
        let cross = ("sec-fetch-site", "cross-site");
        assert_eq!(
            status(Method::POST, "/api/imports", &[cross]),
            Some(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            status(
                Method::POST,
                "/api/imports",
                &[("sec-fetch-site", "Cross-Site")]
            ),
            Some(StatusCode::FORBIDDEN)
        );
        for site in ["same-origin", "same-site", "none"] {
            assert_eq!(
                status(Method::POST, "/api/imports", &[("sec-fetch-site", site)]),
                None
            );
        }
        // Tauri's WebView reports its own requests as cross-site.
        assert_eq!(
            status(
                Method::POST,
                "/api/imports",
                &[cross, ("origin", "tauri://localhost")]
            ),
            None
        );
        assert_eq!(status(Method::GET, "/api/health", &[cross]), None);
        assert_eq!(status(Method::POST, "/api/imports", &[]), None);
    }
}
