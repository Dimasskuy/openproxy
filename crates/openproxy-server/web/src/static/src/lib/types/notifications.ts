// lib/types/notifications.ts — TypeScript mirror of
// `crates/openproxy-core/src/notifications.rs` (F1).
//
// Every field name here matches the Rust struct field name (snake_case) because F1 serializes
// through `serde_json::json!` with literal keys. There is no codegen: when the Rust side changes,
// update both sides.
//
// Three groups: `NotificationEvent` (pushed live over the WebSocket, F2), `NotificationRow`
// (returned by `GET /admin/api/notifications`), and the per-kind payload structs that narrow
// either `payload` based on `kind`.
//
// See `lib/types/api.ts` for the conventions (snake_case fields, `Option<T>` → `T | null`,
// `serde_json::Value` → `unknown`-ish).

/** Kind discriminator for notifications. Mirrors the CHECK constraint in migration 000036 and
 *  the `KIND_*` constants in `notifications.rs`.
 *  @see crates/openproxy-core/src/notifications.rs:34 */
export type NotificationKind =
  | "model_new"
  | "model_gone"
  | "model_auto_activated"
  | "system";

/** Real-time event pushed over the WebSocket. The server wraps it as
 *  `{ "type": "notification", "data": <NotificationEvent> }` (F2).
 *  @see crates/openproxy-core/src/notifications.rs:64 */
export interface NotificationEvent {
  id: number;
  kind: NotificationKind;
  /** Free-form JSON payload, always an object in practice (the per-kind payload struct).
   *  Narrow via `is${Kind}Payload` for typed access; `Record<string, unknown>` is tighter than
   *  `unknown` because every payload F1 emits is a JSON object. */
  payload: Record<string, unknown>;
  /** RFC 3339 timestamp set by SQLite `datetime('now')` on insert. */
  created_at: string;
}

/** `model_new` payload — emitted by `models::upsert_many` when a model appears in discovery that
 *  wasn't in the existing snapshot.
 *  @see crates/openproxy-core/src/notifications.rs:75 */
export interface ModelNewPayload {
  provider_id: string;
  model_id: string;
  display_name: string | null;
  target_format: string;
  context_length: number | null;
}

/** `model_gone` payload — emitted by `models::upsert_many` when a model in the existing snapshot
 *  is no longer in discovery. `display_name` is snapshotted BEFORE the DELETE so we can show what
 *  was lost; it may be `null` if it couldn't be read.
 *  @see crates/openproxy-core/src/notifications.rs:84 */
export interface ModelGonePayload {
  provider_id: string;
  model_id: string;
  display_name: string | null;
}

/** `model_auto_activated` payload — emitted by `models::apply_auto_activation` when a newly
 *  discovered model matches the provider's `auto_activate_keyword` and is flipped to `active=1`.
 *  `matched_keyword` is `null` when the provider had no keyword configured (all new models
 *  auto-activate).
 *  @see crates/openproxy-core/src/notifications.rs:93 */
export interface ModelAutoActivatedPayload {
  provider_id: string;
  model_id: string;
  display_name: string | null;
  matched_keyword: string | null;
}

/** `system` payload — emitted by `discovery_scheduler` on error paths (`discovery_failed`,
 *  `account_key_decrypt_failed`) and any future system-level event. `code` is the stable
 *  machine-readable identifier (also the dedup key); `details` is free-form.
 *  @see crates/openproxy-core/src/notifications.rs:103 */
export interface SystemPayload {
  code: string;
  message: string;
  provider_id: string | null;
  details: unknown;
}

/** Notification row as returned by `GET /admin/api/notifications`. `read_at` / `archived_at` are
 *  `null` until the corresponding action is taken. `dedup_key` and `provider_id` are informational
 *  — the dashboard rarely needs them, but they help grouping/debugging.
 *  @see crates/openproxy-core/src/notifications.rs:120 */
export interface NotificationRow {
  id: number;
  kind: NotificationKind;
  payload: Record<string, unknown>;
  read_at: string | null;
  archived_at: string | null;
  created_at: string;
  dedup_key: string | null;
  provider_id: string | null;
}

// ── Payload type-guards ────────────────────────────────────────────────────
// Narrow `payload` once `kind` is known. Defensive by design: a malformed payload (missing
// fields, wrong types) returns `false` rather than raising at runtime.

export function isModelNewPayload(p: unknown): p is ModelNewPayload {
  if (typeof p !== "object" || p === null) return false;
  const o = p as Record<string, unknown>;
  return (
    typeof o["provider_id"] === "string" &&
    typeof o["model_id"] === "string" &&
    (o["display_name"] === null || typeof o["display_name"] === "string") &&
    typeof o["target_format"] === "string" &&
    (o["context_length"] === null || typeof o["context_length"] === "number")
  );
}

export function isModelGonePayload(p: unknown): p is ModelGonePayload {
  if (typeof p !== "object" || p === null) return false;
  const o = p as Record<string, unknown>;
  return (
    typeof o["provider_id"] === "string" &&
    typeof o["model_id"] === "string" &&
    (o["display_name"] === null || typeof o["display_name"] === "string")
  );
}

export function isModelAutoActivatedPayload(p: unknown): p is ModelAutoActivatedPayload {
  if (typeof p !== "object" || p === null) return false;
  const o = p as Record<string, unknown>;
  return (
    typeof o["provider_id"] === "string" &&
    typeof o["model_id"] === "string" &&
    (o["display_name"] === null || typeof o["display_name"] === "string") &&
    (o["matched_keyword"] === null || typeof o["matched_keyword"] === "string")
  );
}

export function isSystemPayload(p: unknown): p is SystemPayload {
  if (typeof p !== "object" || p === null) return false;
  const o = p as Record<string, unknown>;
  return (
    typeof o["code"] === "string" &&
    typeof o["message"] === "string" &&
    (o["provider_id"] === null || typeof o["provider_id"] === "string")
    // `details` is `unknown`, so any value is acceptable.
  );
}
