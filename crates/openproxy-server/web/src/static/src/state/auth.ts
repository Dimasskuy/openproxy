// state/auth.ts — admin token management for the dashboard.
//
// DASHBOARD-FIX (Bug 2): the server's `admin_auth_middleware`
// (handlers/admin/auth.rs) requires a `Bearer <token>` Authorization
// header on every `/admin/api/*` request. The `/admin/ws` upgrade
// cannot carry that header from a browser, so `state/ws.ts` first
// exchanges the Bearer token for a single-use, 30-second ticket
// (`POST /admin/api/ws-ticket`) and connects with `?ticket=`. The
// API key itself is NEVER placed in a URL (reverse proxies log the
// request line).
//
// This module owns the manage-scope API key string. It is:
//   - Entered once via the login view (views/login.ts).
//   - Persisted to localStorage so it survives reloads.
//   - Seeded into module-local memory on first read so subsequent
//     `getToken()` calls don't hit localStorage on every fetch.
//   - Attached as a Bearer header to every `fetch()` by `lib/api.ts` /
//     `state/api.ts`; `state/ws.ts` exchanges it for a WS ticket.
//
// Security notes:
//   - The token lives in localStorage, NOT in a cookie. Cookies
//     would be sent automatically on every same-origin request,
//     including the (intentionally unauthenticated) `/admin/health`
//     and `/admin/i18n/*` routes — that's fine for an auth token
//     (the server ignores it on those routes) but the bigger issue
//     is that the dashboard has no CSRF protection, so a cookie
//     would be replayable cross-site. localStorage is only readable
//     by same-origin JS, which is the same trust boundary the
//     dashboard itself runs under.
//   - The token is the user's manage-scope API key, identical to
//     what a CLI client would use. If the user logs out (or the
//     key is revoked server-side), they have to re-enter it.
//   - We do NOT add session expiry, "remember me", or password
//     manager integration beyond what the browser does by default
//     for `<input type="password">`. A future iteration could add
//     a session timer that calls `clearToken()` after N hours.

const STORAGE_KEY = "openproxy_admin_token";

// Module-local cached token. Seeded from localStorage on first
// access. We keep this in memory (not just localStorage) so the
// `Authorization` header construction in the hot `api()` path
// doesn't pay a synchronous localStorage read on every fetch.
let currentToken: string | null = null;

/** Read the token from localStorage. Returns null if not stored,
 *  if localStorage is disabled (e.g. private mode in some browsers),
 *  or if the stored value is empty. */
function load(): string | null {
  try {
    const v: string | null = localStorage.getItem(STORAGE_KEY);
    if (!v) return null;
    return v;
  } catch {
    // localStorage can throw under SecurityError (private mode in
    // some browsers, or when cookies are blocked). Treat as "no
    // token stored" — the user will have to log in again next
    // reload, which is the safe default.
    return null;
  }
}

/** Persist the token to localStorage + cache it in memory. Trims
 *  surrounding whitespace because users paste keys from CLI output
 *  that may include trailing newlines. */
export function setToken(token: string): void {
  currentToken = token.trim();
  try {
    localStorage.setItem(STORAGE_KEY, currentToken);
  } catch (e: unknown) {
    // localStorage may be full or disabled. The token is still in
    // memory for the current session; we warn so the user knows
    // they'll have to re-enter it on the next reload.
    console.warn("Could not persist admin token to localStorage:", e);
  }
}

/** Clear the token (logout). Removes it from localStorage and from
 *  the in-memory cache. The next `api()` call will send no
 *  Authorization header → the server returns 401 → the router's
 *  auth gate redirects to the login view. */
export function clearToken(): void {
  currentToken = null;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // localStorage may be disabled; nothing to remove. The
    // in-memory copy is already null, so the behaviour is correct.
  }
}

/** Get the current token, seeding the in-memory cache from
 *  localStorage on first access. Returns null if the user is not
 *  logged in. Callers (`lib/api.ts`, `state/api.ts`, `state/ws.ts`)
 *  use the return value to build the `Authorization` header; a null
 *  return means "send no auth" — the server will respond with 401
 *  and the caller can surface that to the user. */
export function getToken(): string | null {
  if (currentToken === null) {
    currentToken = load();
  }
  return currentToken;
}

/** Is the user logged in (has a token)? Used by the router's auth
 *  gate to decide whether to redirect to the login view. */
export function isLoggedIn(): boolean {
  return getToken() !== null;
}
