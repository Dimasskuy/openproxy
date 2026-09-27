// Debug Logs viewer.
//
// Polls `/admin/debug/logs` every 2s via chained `setTimeout` (not
// `setInterval`: a slow request cannot pile up because the next timer is
// scheduled only after the previous fetch resolves). A `sinceSeq` cursor
// advances to `latest_seq` after each successful poll so only new entries
// are fetched.
//
// The loop calls `requestUpdate()` rather than rebuilding the tbody, so
// lit-html patches only the changed rows: filter inputs keep their value
// and focus is preserved.
//
// A filter change bumps an epoch counter (discarding any in-flight poll's
// response), resets the cursor and entry list, and starts an immediate
// poll.
//
// The cleanup function cancels the pending poll timer; the router calls it
// before mounting the next view.

import { html, type TemplateResult } from 'lit-html';
import { fetchDebugLogs, clearDebugLogs } from "../lib/api.js";
import { icons } from "../lib/icons.js";
import type { FetchDebugLogsOpts } from "../lib/api.js";
import { showToast } from "../components/toast.js";
import { copyToClipboard } from "../lib/clipboard.js";
import { mountView, requestUpdate } from "../state/reactive.js";
import type { DebugLogEntry } from "../lib/types/api.js";
// Mark entries viewed on every successful poll so the sidebar badge
// (driven by `state/debug-logs-store.ts`) clears while the user is on
// this page. The store's 30s poll resumes accumulating on navigation.
import { markDebugLogsViewed } from "../state/debug-logs-store.js";

// Poll interval. Chained via setTimeout — see `pollNow` below.
const POLL_INTERVAL_MS: number = 2000;

// Server-side ring-buffer capacity, shown in the "Buffer: X / 1000"
// indicator. Mirrors `BUFFER_CAPACITY` in
// `crates/openproxy-server/src/debug_log.rs`.
const BUFFER_CAPACITY: number = 1000;

// Cap on the number of rows we keep in the DOM. The server's ring
// buffer holds 1000; we render at most MAX_ROWS of the most recent
// to keep the table responsive.
const MAX_ROWS: number = 500;

// Levels rendered as filter checkboxes. ERROR-first so the most
// actionable levels sit closest to the label.
const LEVELS: readonly string[] = ["ERROR", "WARN", "INFO", "DEBUG"];

// Map a level string to a CSS color value (a var() reference so the
// view adapts to light/dark themes).
function levelColor(level: string): string {
  const upper: string = level.toUpperCase();
  if (upper === "ERROR") return "var(--color-error)";
  if (upper === "WARN") return "var(--color-warn)";
  if (upper === "INFO") return "var(--color-info)";
  if (upper === "DEBUG") return "var(--color-text-muted)";
  return "var(--color-text-muted)";
}

// Strip the date portion from an ISO-8601 timestamp for compact
// display in the table; the full timestamp is preserved in the
// `title` attribute so hover shows the complete value.
function formatTime(ts: string): string {
  const t: string = ts || "";
  const idx: number = t.indexOf("T");
  if (idx < 0) return t;
  return t.slice(idx + 1).replace(/Z$/, "");
}

// Returns `Promise<boolean>` so call sites can branch on success.
async function copyToClipboardOk(text: string): Promise<boolean> {
  try {
    await copyToClipboard(text);
    return true;
  } catch {
    return false;
  }
}

// Serialize the visible entries as a Markdown table for the "Copy all"
// button. Pipes and newlines in cell values are escaped so the table
// layout is preserved.
function buildMarkdown(rows: DebugLogEntry[]): string {
  const lines: string[] = [];
  lines.push("# Debug Logs");
  lines.push("");
  lines.push(`_Exported ${new Date().toISOString()} — ${rows.length} entries_`);
  lines.push("");
  lines.push("| Time | Level | Target | Request ID | Trace ID | Message |");
  lines.push("| --- | --- | --- | --- | --- | --- |");
  const esc = (s: string): string => s.replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
  for (const r of rows) {
    const time: string = r.timestamp || "";
    const level: string = r.level || "";
    const target: string = esc(r.target || "");
    const rid: string = esc(r.request_id || "");
    const tid: string = esc(r.trace_id || "");
    const msg: string = esc(r.message || "");
    lines.push(`| ${time} | ${level} | ${target} | ${rid} | ${tid} | ${msg} |`);
  }
  return lines.join("\n");
}

// `stopped` lets an in-flight poll short-circuit after cleanup. `epoch`
// is bumped on every filter change; a poll whose epoch no longer matches
// discards its response, since the filter change started a fresh poll.
let entries: DebugLogEntry[] = [];
let sinceSeq: number = 0;
let pollHandle: ReturnType<typeof setTimeout> | null = null;
let stopped: boolean = false;
let epoch: number = 0;
let lastTotalInBuffer: number = 0;
let lastPollFailed: boolean = false;
// Last poll error, shown in the tbody only when the entry list is empty;
// with entries present they stay visible and the toast carries the error.
let pollErrorMessage: string | null = null;

// Filter state. Updated by the @change handlers on the filter
// inputs; read by the poll loop to build the query string.
let filterLevels: Set<string> = new Set();
let filterRequestId: string = "";
let filterTraceId: string = "";


async function onCopyCell(val: string): Promise<void> {
  if (!val) return;
  const ok: boolean = await copyToClipboardOk(val);
  if (ok) showToast(`Copied: ${val}`, "success");
  else showToast("Clipboard write failed (browser blocked it).", "error");
}

async function onCopyAll(): Promise<void> {
  if (entries.length === 0) {
    showToast("No entries to copy.", "info");
    return;
  }
  // Newest-first to match the on-screen order.
  const md: string = buildMarkdown(entries.slice().reverse());
  const ok: boolean = await copyToClipboardOk(md);
  if (ok) showToast(`Copied ${entries.length} entries as Markdown.`, "success");
  else showToast("Clipboard write failed (browser blocked it).", "error");
}

async function onClear(): Promise<void> {
  try {
    await clearDebugLogs();
    entries = [];
    sinceSeq = 0;
    lastTotalInBuffer = 0;
    pollErrorMessage = null;
    // After a server-side clear the ring buffer is empty, so there is
    // nothing left unviewed.
    markDebugLogsViewed();
    requestUpdate();
    showToast("Debug log buffer cleared on the server.", "success");
  } catch (e: unknown) {
    const msg: string = e instanceof Error ? e.message : String(e);
    showToast(`Failed to clear buffer: ${msg}`, "error");
  }
}

function onLevelToggle(lvl: string, e: Event): void {
  const target = e.target;
  const checked: boolean = target instanceof HTMLInputElement ? target.checked : false;
  if (checked) filterLevels.add(lvl);
  else filterLevels.delete(lvl);
  onFilterChange();
}

function onRidChange(e: Event): void {
  const target = e.target;
  filterRequestId = target instanceof HTMLInputElement ? target.value.trim() : "";
  onFilterChange();
}

function onTidChange(e: Event): void {
  const target = e.target;
  filterTraceId = target instanceof HTMLInputElement ? target.value.trim() : "";
  onFilterChange();
}

// A filter change cancels the pending timer, bumps the epoch so an
// in-flight poll's response is discarded, resets the cursor + entries, and
// starts an immediate poll.
function onFilterChange(): void {
  epoch++;
  sinceSeq = 0;
  entries = [];
  pollErrorMessage = null;
  if (pollHandle !== null) {
    clearTimeout(pollHandle);
    pollHandle = null;
  }
  requestUpdate();
  void pollNow();
}

//
// Chained setTimeout: the next poll is scheduled only after the current
// fetch resolves, so a slow server cannot pile requests up. Hoisted so
// onFilterChange can call it above its textual position.
async function pollNow(): Promise<void> {
  if (stopped) return;
  const myEpoch: number = epoch;
  try {
    // Omit absent filters rather than sending `undefined`:
    // `exactOptionalPropertyTypes` rejects it for an optional string.
    const opts: FetchDebugLogsOpts = { since: sinceSeq };
    const lvl: string = Array.from(filterLevels).join(",");
    if (lvl) opts.level = lvl;
    if (filterRequestId) opts.request_id = filterRequestId;
    if (filterTraceId) opts.trace_id = filterTraceId;
    const resp = await fetchDebugLogs(opts);
    // Discard if a filter changed or the view unmounted mid-flight.
    if (stopped || myEpoch !== epoch) return;
    lastTotalInBuffer = resp.total_in_buffer;
    if (resp.entries.length > 0) {
      // Dedupe by seq: an overlapping poll can return the same entries
      // twice after a blip where the server processed the request but the
      // client timed out and retried.
      const seen: Set<number> = new Set(entries.map((e: DebugLogEntry) => e.seq));
      for (const e of resp.entries) {
        if (!seen.has(e.seq)) {
          entries.push(e);
          seen.add(e.seq);
        }
      }
      // Trim to MAX_ROWS (keep the newest).
      if (entries.length > MAX_ROWS) {
        entries = entries.slice(entries.length - MAX_ROWS);
      }
    }
    sinceSeq = resp.latest_seq;
    lastPollFailed = false;
    pollErrorMessage = null;
    // Advance the store's "viewed" cursor to the latest seq fetched,
    // clearing the sidebar badge for unviewed WARN+ERROR entries. Later
    // entries re-trigger it once the user navigates away.
    markDebugLogsViewed();
    requestUpdate();
  } catch (e: unknown) {
    if (stopped || myEpoch !== epoch) return;
    const msg: string = e instanceof Error ? e.message : String(e);
    // Inline error only when the table is empty; otherwise keep the
    // existing rows visible and just toast.
    if (entries.length === 0) {
      pollErrorMessage = msg;
    }
    // Toast only on the first failure of a run, so a down server does
    // not spam the operator every 2s.
    if (!lastPollFailed) {
      showToast(`Debug logs poll failed: ${msg}`, "error");
      lastPollFailed = true;
    }
    requestUpdate();
  } finally {
    // Reschedule only while the epoch still matches; after a filter
    // change the poll started by onFilterChange owns the schedule.
    if (!stopped && myEpoch === epoch) {
      pollHandle = setTimeout(() => { void pollNow(); }, POLL_INTERVAL_MS);
    }
  }
}


// The request_id and trace_id cells are <button> elements so they are
// keyboard-focusable and announce as interactive; @click copies the value.
function renderRow(entry: DebugLogEntry): TemplateResult {
  const lvlColor: string = levelColor(entry.level);
  const rid: string | null = entry.request_id;
  const tid: string | null = entry.trace_id;
  const ridCell: TemplateResult = rid
    ? html`<button type="button" class="debug-copyable" title="Click to copy request ID" @click=${() => onCopyCell(rid)}>${rid}</button>`
    : html`<span class="muted">—</span>`;
  const tidCell: TemplateResult = tid
    ? html`<button type="button" class="debug-copyable" title="Click to copy trace ID" @click=${() => onCopyCell(tid)}>${tid}</button>`
    : html`<span class="muted">—</span>`;
  const spanPath: TemplateResult = entry.span_path
    ? html`<br><small class="muted debug-span-path" title=${entry.span_path}>${entry.span_path}</small>`
    : html``;
  return html`<tr class="debug-log-card-row">
    <td class="debug-time col-debug-time" data-label="Time" title=${entry.timestamp} style="white-space:nowrap;font-family:var(--font-mono);font-size:0.8rem;">${formatTime(entry.timestamp)}</td>
    <td class="debug-level col-debug-level" data-label="Level" style="color:${lvlColor};font-weight:600;white-space:nowrap;">${entry.level}</td>
    <td class="debug-target col-debug-target" data-label="Target" title=${entry.target} style="font-family:var(--font-mono);font-size:0.8rem;">${entry.target}</td>
    <td class="debug-rid col-debug-rid" data-label="Request ID">${ridCell}</td>
    <td class="debug-tid col-debug-tid" data-label="Trace ID">${tidCell}</td>
    <td class="debug-message col-debug-message" data-label="Message" style="word-break:break-word;">${entry.message}${spanPath}</td>
  </tr>`;
}

function renderTbody(): TemplateResult {
  if (entries.length === 0) {
    const msg: string = pollErrorMessage ? `Poll error: ${pollErrorMessage}` : "No debug log entries yet.";
    return html`<tr><td colspan="6" class="empty" style="text-align:center;padding:1rem;color:var(--color-text-muted);">${msg}</td></tr>`;
  }
  // Server returns oldest-first; show newest-first, capped at MAX_ROWS.
  const rows: DebugLogEntry[] = entries.slice().reverse().slice(0, MAX_ROWS);
  return html`${rows.map(renderRow)}`;
}

function renderLevelChecks(): TemplateResult {
  return html`${LEVELS.map((lvl: string) => {
    const color: string = levelColor(lvl);
    const id: string = `debug-level-filter-${lvl.toLowerCase()}`;
    return html`<label class="debug-level-check" style="display:inline-flex;align-items:center;gap:0.25rem;margin-right:0.5rem;cursor:pointer;">
      <input type="checkbox" id=${id} class="debug-level-filter" value=${lvl} ?checked=${filterLevels.has(lvl)} @change=${(e: Event) => onLevelToggle(lvl, e)}>
      <span style="color:${color};font-weight:600;font-size:0.8rem;">${lvl}</span>
    </label>`;
  })}`;
}

function renderDebugLogs(): TemplateResult {
  return html`
    <div class="page-header"><h2>Debug Logs</h2>
      <div class="actions">
        <span class="debug-buffer-indicator" id="debug-buffer-indicator" style="font-size:0.85rem;color:var(--color-text-muted);margin-right:0.5rem;font-family:var(--font-mono);">Buffer: ${lastTotalInBuffer} / ${BUFFER_CAPACITY}</span>
        <button type="button" class="link" title="Copy all visible entries as Markdown to the clipboard" @click=${onCopyAll}>${icons.copy()} Copy all</button>
        <button type="button" class="link" title="Clear the in-memory debug log ring buffer on the server" style="margin-left:0.5rem;" @click=${onClear}>${icons.trash()} Clear</button>
      </div>
    </div>
    <div class="debug-filters" style="display:flex;flex-wrap:wrap;gap:1rem;align-items:flex-end;padding:0.5rem 0 1rem;border-bottom:1px solid var(--color-border-soft);margin-bottom:0.5rem;">
      <div class="debug-filter-group" style="display:flex;flex-direction:column;gap:0.25rem;">
        <span class="debug-filter-label" style="font-size:0.72rem;text-transform:uppercase;color:var(--color-text-muted);">Level</span>
        <div>${renderLevelChecks()}</div>
      </div>
      <div class="debug-filter-group" style="display:flex;flex-direction:column;gap:0.25rem;">
        <label class="debug-filter-label" for="debug-filter-request-id" style="font-size:0.72rem;text-transform:uppercase;color:var(--color-text-muted);">Request ID</label>
        <input type="text" id="debug-filter-request-id" placeholder="req-abc123" autocomplete="off" style="padding:0.25rem 0.5rem;border:1px solid var(--color-border-soft);min-width:12rem;background:var(--color-surface);color:var(--color-text);" @change=${onRidChange}>
      </div>
      <div class="debug-filter-group" style="display:flex;flex-direction:column;gap:0.25rem;">
        <label class="debug-filter-label" for="debug-filter-trace-id" style="font-size:0.72rem;text-transform:uppercase;color:var(--color-text-muted);">Trace ID</label>
        <input type="text" id="debug-filter-trace-id" placeholder="tr-def456" autocomplete="off" style="padding:0.25rem 0.5rem;border:1px solid var(--color-border-soft);min-width:12rem;background:var(--color-surface);color:var(--color-text);" @change=${onTidChange}>
      </div>
    </div>
    <div class="debug-table-wrap table-wrap" style="overflow-x:auto;">
      <table class="debug-logs-table responsive-card-table" style="width:100%;border-collapse:collapse;font-size:0.85rem;">
        <thead>
          <tr style="border-bottom:2px solid var(--color-border);text-align:left;">
            <th class="debug-time" style="padding:0.35rem 0.5rem;white-space:nowrap;">Time</th>
            <th class="debug-level" style="padding:0.35rem 0.5rem;">Level</th>
            <th class="debug-target" style="padding:0.35rem 0.5rem;">Target</th>
            <th class="debug-rid" style="padding:0.35rem 0.5rem;">Request ID</th>
            <th class="debug-tid" style="padding:0.35rem 0.5rem;">Trace ID</th>
            <th class="debug-message" style="padding:0.35rem 0.5rem;">Message</th>
          </tr>
        </thead>
        <tbody id="debug-logs-tbody">${renderTbody()}</tbody>
      </table>
    </div>
  `;
}

//
// Mount the Debug Logs view into `container`: renders the header, filter
// bar and entries table, starts the 2s polling loop, and returns a cleanup
// function. The router calls that cleanup before mounting the next view, so
// the loop does not leak across navigations.
export function mountDebugLogs(container: HTMLElement): () => void {
  // The router runs the previous view's cleanup first, so `stopped`
  // arrives true; flip it back.
  entries = [];
  sinceSeq = 0;
  pollHandle = null;
  stopped = false;
  epoch = 0;
  lastTotalInBuffer = 0;
  lastPollFailed = false;
  pollErrorMessage = null;
  filterLevels = new Set();
  filterRequestId = "";
  filterTraceId = "";

  const cleanupReactive = mountView(container, renderDebugLogs);

  // Kick off the first poll immediately (no 2s delay on mount).
  void pollNow();

  // Cancel the pending poll timer. An in-flight fetch short-circuits on
  // `stopped` when it resolves. Release the lit-html container so the next
  // view's mountView does not race with a late requestUpdate().
  return () => {
    stopped = true;
    if (pollHandle !== null) {
      clearTimeout(pollHandle);
      pollHandle = null;
    }
    cleanupReactive();
  };
}
