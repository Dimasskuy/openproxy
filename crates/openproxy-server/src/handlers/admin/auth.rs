use super::{ApiError, AppState, CoreError, HeaderMap, IntoResponse};
use openproxy_core::api_keys as core_api_keys;
use std::net::SocketAddr;
use std::sync::Arc;

/// Who is calling an admin endpoint, as resolved by [`admin_auth_middleware`],
/// stored in request extensions so handlers can bind derived credentials (WS
/// tickets) to the key and write attributable audit records for secret reads.
/// `key` is `None` only under the debug-build dev bypass.
#[derive(Clone, Debug)]
pub struct AdminIdentity {
    pub key: Option<Arc<core_api_keys::ApiKey>>,
    pub remote_addr: Option<SocketAddr>,
}

impl AdminIdentity {
    /// Numeric key id for audit logs (`None` under dev bypass).
    pub fn key_id(&self) -> Option<i64> {
        self.key.as_ref().map(|k| k.id.0)
    }
}

/// Extractor alias for handlers: the identity resolved by
/// [`admin_auth_middleware`]. `Option` because a handler may be mounted outside
/// the middleware in tests; production admin routes always have it.
pub(crate) type Identity = Option<axum::Extension<AdminIdentity>>;

/// Audit record written whenever an admin endpoint hands out (or writes to disk) a
/// decrypted secret. WARN on the dedicated `openproxy::security::audit` target so
/// operators can route it to a separate sink and alert on unexpected
/// `key_id`/`ip` pairs.
pub(crate) fn audit_secret_read(identity: &Identity, secret_kind: &str, subject: &str) {
    let id = identity.as_ref().map(|axum::Extension(i)| i);
    tracing::warn!(
        target: "openproxy::security::audit",
        key_id = id.and_then(AdminIdentity::key_id),
        key_prefix = id
            .and_then(|i| i.key.as_ref())
            .and_then(|k| k.key_prefix.as_deref()),
        ip = id.and_then(|i| i.remote_addr).map(|a| a.ip().to_string()),
        secret = secret_kind,
        subject,
        "secret disclosed via admin api"
    );
}

#[cfg(debug_assertions)]
fn check_dev_auth_bypass(
    headers: &HeaderMap,
    remote_addr: Option<&SocketAddr>,
) -> Result<bool, ApiError> {
    let Ok(bypass) = std::env::var("OPENPROXY_DASHBOARD_AUTH_BYPASS") else {
        return Ok(false);
    };
    if bypass != "1" {
        return Ok(false);
    }
    // The bypass is a local-development convenience. Refuse it outright
    // when the request came through a reverse proxy: behind a proxy on
    // the same host every remote client looks like loopback at the TCP
    // layer, so the peer-address check below would be meaningless.
    if headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded") {
        tracing::error!(
            target: "openproxy::security",
            "OPENPROXY_DASHBOARD_AUTH_BYPASS refused: request carries proxy forwarding headers"
        );
        return Err(ApiError(CoreError::Auth(
            "dev bypass not available behind a proxy".into(),
        )));
    }
    if let Some(addr) = remote_addr
        && !addr.ip().is_loopback()
    {
        tracing::error!(
            target: "openproxy::security",
            ip = %addr.ip(),
            "attempted to use OPENPROXY_DASHBOARD_AUTH_BYPASS from non-loopback IP"
        );
        return Err(ApiError(CoreError::Auth(
            "unauthorized IP for dev bypass".into(),
        )));
    }
    tracing::warn!(
        target: "openproxy::security",
        path = ?headers.get("x-original-uri").and_then(|v| v.to_str().ok()),
        method = ?headers.get("x-original-method").and_then(|v| v.to_str().ok()),
        "admin auth bypassed via OPENPROXY_DASHBOARD_AUTH_BYPASS=1 — \
         every admin endpoint is open. Remove this env var to restore auth."
    );
    Ok(true)
}

/// Pull the Bearer token out of the `Authorization` header.
///
/// Credentials are accepted from headers ONLY — never query-string tokens, not
/// even for WebSocket upgrades, because every reverse proxy logs the request line
/// and a long-lived key would land in plaintext in access logs. Browsers that
/// cannot set headers on `new WebSocket()` use the single-use ticket flow
/// ([`authenticate_admin_ws`], `state::WsTicketStore`).
fn extract_bearer_token(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    let Some(raw) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::trim)
    else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Err(ApiError(CoreError::Auth("invalid token".into())));
    }
    Ok(Some(raw))
}

/// Authenticate an admin REST request from its headers.
///
/// Returns the resolved identity; `key` is `None` only under the
/// debug-build dev bypass.
pub(crate) fn authenticate_admin(
    state: &AppState,
    headers: &HeaderMap,
    remote_addr: Option<&SocketAddr>,
) -> Result<AdminIdentity, ApiError> {
    #[cfg(debug_assertions)]
    if check_dev_auth_bypass(headers, remote_addr)? {
        return Ok(AdminIdentity {
            key: None,
            remote_addr: remote_addr.copied(),
        });
    }

    let token = extract_bearer_token(headers)?
        .ok_or_else(|| ApiError(CoreError::Auth("missing authorization header".into())))?;
    let key = crate::middleware::auth::verify_key_credentials(state, token, "manage")?;
    Ok(AdminIdentity {
        key: Some(key),
        remote_addr: remote_addr.copied(),
    })
}

/// Redeem a single-use WS ticket and re-validate the key it was bound to.
fn authenticate_ws_ticket(
    state: &AppState,
    ticket: &str,
) -> Result<Arc<core_api_keys::ApiKey>, ApiError> {
    let key_id = state
        .ws_tickets()
        .consume(ticket)
        .ok_or_else(|| ApiError(CoreError::Auth("invalid or expired ws ticket".into())))?;
    let key = {
        let r = state.db_pool().reader();
        core_api_keys::get_by_id(&r, key_id)
            .map_err(|e| {
                tracing::error!(%e, "db error resolving ws ticket key");
                ApiError(CoreError::Auth("invalid api key".into()))
            })?
            .ok_or_else(|| ApiError(CoreError::Auth("invalid api key".into())))?
    };
    crate::middleware::auth::validate_key_record(&key, "manage")?;
    Ok(Arc::new(key))
}

/// Authenticate the `/admin/ws` upgrade (and any handler that wants the
/// same contract). Accepts EITHER `Authorization: Bearer <key>` (CLI /
/// non-browser clients) OR a single-use `?ticket=` minted by
/// `POST /admin/api/ws-ticket` (browsers). Never a raw key in the URL.
pub(crate) fn authenticate_admin_ws(
    state: &AppState,
    headers: &HeaderMap,
    ticket: Option<&str>,
    remote_addr: Option<&SocketAddr>,
) -> Result<AdminIdentity, ApiError> {
    #[cfg(debug_assertions)]
    if check_dev_auth_bypass(headers, remote_addr)? {
        return Ok(AdminIdentity {
            key: None,
            remote_addr: remote_addr.copied(),
        });
    }

    let key = match (extract_bearer_token(headers)?, ticket) {
        (Some(token), _) => {
            crate::middleware::auth::verify_key_credentials(state, token, "manage")?
        }
        (None, Some(t)) if !t.is_empty() => authenticate_ws_ticket(state, t)?,
        _ => {
            return Err(ApiError(CoreError::Auth(
                "missing authorization header or ws ticket".into(),
            )));
        }
    };
    Ok(AdminIdentity {
        key: Some(key),
        remote_addr: remote_addr.copied(),
    })
}

pub async fn admin_auth_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    match authenticate_admin(&state, req.headers(), Some(&addr)) {
        Ok(identity) => {
            req.extensions_mut().insert(identity);
        }
        Err(e) => return e.into_response(),
    }
    next.run(req).await
}
