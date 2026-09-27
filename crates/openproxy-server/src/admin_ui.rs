//! Dashboard SPA embedded in the server binary.
//!
//! The frontend is built by `pnpm build` in `crates/openproxy-server/web/` into
//! `web/src/static/dist/`; `rust-embed` embeds the whole `web/src/static/` tree
//! at compile time, so the binary ships API + dashboard on one port.
//!
//! Routes mounted at `/admin/*` (not `/admin/api/*` or `/admin/ws` — those are
//! served by other handlers; see `router.rs::build_router` for the nesting):
//!
//! - `GET /admin`            → SPA shell (`index_html`)
//! - `GET /admin/`           → SPA shell (`index_html`)
//! - `GET /admin/callback.html` → OAuth callback page (`callback_html`)
//! - `GET /admin/dist/*`     → embedded built bundle
//! - `GET /admin/styles/*`   → embedded CSS
//! - `GET /admin/fonts/*`    → embedded fonts
//! - any other `/admin/*`    → SPA fallback to `index.html`
//!
//! `index.html` and `callback.html` use `include_str!` rather than
//! `RustEmbed::get` so the handler returns `Html<&'static str>` with no owned
//! buffer.

use axum::{
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use mime_guess::from_path;
use rust_embed::RustEmbed;

/// Embedded copy of `crates/openproxy-server/web/src/static/`. The `#[folder]`
/// path resolves relative to this crate's `Cargo.toml`; the whole tree (not
/// just `dist/`) is embedded so `index.html` can reference `/admin/dist/app.js`,
/// `/admin/styles/index.css` and `/admin/fonts/...` from one namespace.
///
/// `dist/` is esbuild output produced by `pnpm build` and gitignored, so a fresh
/// checkout has none; `rust-embed` still embeds the rest (HTML, CSS, fonts,
/// i18n JSON). Release builds run `pnpm build` before `cargo build` (see
/// `Dockerfile`, `.github/workflows/ci.yml`) to ship the full bundle.
#[derive(RustEmbed)]
#[folder = "web/src/static/"]
struct DashboardAssets;

/// Embedded per-language JSON string packs consumed by the frontend's
/// `i18n/index.ts` `loadLang()`, served by [`serve_i18n`]. Folder path is
/// relative to this crate's `Cargo.toml` (same convention as [`DashboardAssets`]).
///
/// Only files present at compile time exist: a new `es.json` must land in
/// `web/src/static/src/i18n/` before rebuilding. Serving from disk at runtime is
/// deliberately not supported — the dashboard string contract is part of the
/// binary, not runtime config.
#[derive(RustEmbed)]
#[folder = "web/src/static/src/i18n/"]
struct I18nAssets;

/// Serve the SPA shell. `include_str!` keeps this allocation-free
/// (`Html<&'static str>`).
pub async fn index_html() -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (headers, Html(include_str!("../web/src/static/index.html"))).into_response()
}

/// Serve the OAuth callback page (static HTML that grabs the `code` query param
/// and `postMessage`s it back to the opener). Same `include_str!` as `index_html`.
pub async fn callback_html() -> Html<&'static str> {
    Html(include_str!("../web/src/static/callback.html"))
}

/// Serve a static asset from the embedded `src/static/` tree.
///
/// The router mounts this handler as the `fallback` for `/admin/*`,
/// so the URI we receive is the full request path (e.g.
/// `/admin/dist/app.js`). We strip the leading `/admin/` (or
/// `/admin`) segment, then look the rest up in the embedded tree.
///
/// Immutable caching is applied to content-addressed chunks (`dist/chunks/*`)
/// and binary fonts (`fonts/*`), while entry bundles (`dist/app.js`, `dist/app.css`)
/// are validated using SHA-256 ETags and HTTP 304 Not Modified responses.
pub async fn serve_asset(uri: Uri, req_headers: axum::http::HeaderMap) -> Response {
    let raw = uri.path();
    // `/admin/dist/app.js` → `dist/app.js`; a bare `/admin` (no slash) becomes
    // the empty path and falls through to the SPA shell below.
    let path = raw
        .strip_prefix("/admin")
        .unwrap_or(raw)
        .trim_start_matches('/');

    if path.is_empty() || path.contains("..") {
        return index_html().await;
    }

    let Some(file) = DashboardAssets::get(path) else {
        // SPA fallback for unknown `/admin/*` paths (e.g. client-side routes
        // like `/admin/combos/42/edit`): the hash-router takes over.
        return index_html().await;
    };

    let hash = file.metadata.sha256_hash();
    let mut etag = String::with_capacity(66);
    etag.push('"');
    for byte in hash {
        use std::fmt::Write;
        let _ = write!(etag, "{byte:02x}");
    }
    etag.push('"');

    if req_headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|m| m.as_bytes() == etag.as_bytes())
    {
        let mut headers = axum::http::HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&etag) {
            headers.insert(header::ETAG, val);
        }
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    let mime = from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("fonts/") || path.starts_with("dist/chunks/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    let mut headers = axum::http::HeaderMap::new();
    if let Ok(ct) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(header::CONTENT_TYPE, ct);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(val) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, val);
    }
    let body = Body::from(file.data);
    (StatusCode::OK, headers, body).into_response()
}

/// `GET /admin/i18n/{lang}` — serve a language pack.
///
/// `i18n/index.ts::loadLang()` calls this at boot with `/admin/i18n/en.json`.
/// The route is registered as `/i18n/{lang}` (axum 0.8 rejects literal-suffix
/// path params, see `router.rs`), so the captured value may be `en` or
/// `en.json`; the optional `.json` is stripped so both work.
///
/// Response is the raw embedded JSON with `Content-Type: application/json;
/// charset=utf-8` and `Cache-Control: public, max-age=86400`: the pack is
/// content-addressed in the binary, so a server upgrade also re-ships
/// `app.js` (no-cache, [`serve_asset`]). 24h is long enough to keep the boot
/// path off the network on same-day reloads and short enough to refresh after
/// an upgrade; the frontend's `force-cache` makes repeat hits free.
///
/// `404 language not found` when no matching `.json` is embedded — the
/// frontend's `loadLang` then falls back to `en`.
///
/// Path traversal: `Path<String>` captures a single segment (no `/`), so `..`
/// and `/` are unreachable. `lang` is still validated against
/// `[a-zA-Z0-9_-]+` after stripping `.json` — `pt-BR` is the most exotic shape
/// we would ship, and the guard keeps the lookup table closed.
pub async fn serve_i18n(lang: Path<String>) -> Response {
    let lang = lang.0.strip_suffix(".json").unwrap_or(&lang.0);
    // Letters, digits, hyphen, underscore: every ISO 639-1 code plus regional
    // variants (`pt-BR`, `zh-Hans`). Anything else is rejected so the
    // embedded-tree lookup cannot be probed with a crafted path.
    if !lang
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || lang.is_empty()
    {
        return (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            "language not found",
        )
            .into_response();
    }
    let filename = format!("{lang}.json");
    let Some(file) = I18nAssets::get(&filename) else {
        return (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            )],
            Body::from(r#"{"error":"Language pack not found"}"#),
        )
            .into_response();
    };

    let body = Body::from(file.data);
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
        ],
        body,
    )
        .into_response()
}
