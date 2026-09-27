// lib/constants.ts — app-wide constants, kept here so the views and handlers do not litter
// the codebase with magic strings/numbers.

import { t } from "../i18n/index.js";
import type { PriorityMode, CooldownMode } from "./types/api.js";

// Human-readable label for each server-side stage. Server keys are kept in the data-stage
// attribute (and CSS) so styling can target them; the cell body shows the friendlier label.
//
// Resolved via i18n at call time. Keys follow the `stage.<key>` namespace in i18n/en.json (e.g.
// `stage.started`, `stage.connecting`); a missing key falls back to the raw stage key.
export function getStageLabel(stage: string): string {
  return t(`stage.${stage}`) || stage;
}

// Live logs WS reconnect backoff in ms.
//
// TRIPLE-FIX (Bug 1): the first delay was 1000ms, so the live dashboard showed "⚠ Disconnected
// from real-time stream" for ~1s after every transient failure (e.g. the first attempt 401ing
// because the token wasn't yet on the WS upgrade URL, or a network blip). Now 250ms, then
// 250 → 500 → 1s → 2s → 5s → 10s → 30s; the 30s cap is kept so a permanently-down server can't
// spin a tight retry loop.
//
// `connectLogsWebSocket()` is invoked synchronously from `initNotificationsStore()` (itself gated
// by the sidebar's `maybeBootstrapNotifications()` on the first render after login), so the very
// first attempt happens immediately on login — these delays only govern retries AFTER a failure.
export const LOGS_WS_RECONNECT_DELAYS: readonly number[] = [250, 500, 1000, 2000, 5000, 10000, 30000];

// Local-storage key for the user theme choice.
export const THEME_STORAGE_KEY = "openproxy-theme";

// Local-storage key for the visible-column choice on the /logs view. Value is a JSON array of
// column keys (e.g. ["time","phase"]).
export const LOGS_VISIBLE_COLUMNS_STORAGE_KEY = "openproxy:logs:visibleColumns";

// Every log-row column in table order. `key` matches the CSS class `.log-{key}` on the span
// (e.g. "time" → `.log-time`) and `label` is the header text. A new column = an entry here plus
// the matching span in components/log-row.js.
export interface LogColumn {
  readonly key: string;
  readonly label: string;
}

export const LOG_COLUMNS: readonly LogColumn[] = [
  { key: "time",     label: "Time"     },
  { key: "phase",    label: "Phase"    },
  { key: "type",     label: "Endpoint" },
  { key: "client",   label: "Client"   },
  { key: "status",   label: "Status"   },
  { key: "provider", label: "Provider" },
  { key: "model",    label: "Model"    },
  { key: "tokens",   label: "Tokens"   },
  { key: "latency",  label: "Latency"  },
  { key: "cost",     label: "Cost"     },
  { key: "cache",    label: "API Cache"},
  { key: "compression", label: "Compress" },
];

export const PRIORITY_MODE_LABELS = {
  strict: "Strict", lkgp: "LKGP", weighted: "Weighted",
  least_used: "Least Used", p2c: "P2C", decision: "Decision (System One)",
} satisfies Record<PriorityMode, string>;

export const PRIORITY_MODE_TOOLTIPS = {
  strict: "Walk targets in manual priority order. The first healthy target is always tried first.",
  lkgp: "Least Known Good Provider — prefer the target with the most recent successful request. Falls back to priority order for never-tried targets. An exploration rate adds priority-weighted randomness: earlier targets (which the operator positioned first for speed/intelligence) are more likely to be explored than later fallback targets.",
  weighted: "Weighted random selection — each target's probability is proportional to its weight. Set weights in the targets table below.",
  least_used: "Prefer the target with the fewest total requests in the selection window. Useful for distributing load evenly.",
  p2c: "Power of Two Choices — pick two random targets, choose the one with fewer recent failures. Good balance of simplicity and load distribution.",
  decision: "System One Semantic Router — runs ultra-fast prompt classification (TypeSafe Jev or Convai Laya) to select the optimal model based on target descriptions.",
} satisfies Record<PriorityMode, string>;

export const COOLDOWN_MODE_TOOLTIPS = {
  flat: "Fixed cooldown duration after each failure. The target is parked for the same amount of time regardless of how many times it has failed.",
  exponential: "Cooldown grows with each failure: base × factor^(failures-1), capped at max. A flapping target gets progressively longer cooldowns, giving it time to recover.",
  none: "Cooldown is disabled. Targets will not be parked in cooldown on failures.",
} satisfies Record<CooldownMode, string>;

// Localised status -> CSS class for the status-pill component.
export function statusPillClass(code: number | null): string {
  if (code == null) return "lost";
  if (code === 0) return "skipped";
  if (code >= 500) return "err";
  if (code >= 400) return "warn";
  if (code >= 200 && code < 300) return "ok";
  return "lost";
}

// Built-in provider ids and quota-capable lists were removed: the UI now reads
// `provider.metadata` for built-in, deletable and quota support.
