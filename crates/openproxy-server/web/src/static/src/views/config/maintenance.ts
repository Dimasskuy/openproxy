// views/config/maintenance.ts — auto-VACUUM settings, retention, manual
// VACUUM trigger, and the vacuum status poller. A failed VACUUM shows
// repair instructions as a toast and, when the message mentions disk
// I/O or integrity, queries the recover endpoint for diagnostics.

import { html, type TemplateResult } from "lit-html";
import { api } from "../../state/api.js";
import { requestUpdate } from "../../state/reactive.js";
import { showToast } from "../../components/toast.js";
import { t } from "../../i18n/index.js";
import { card, errStr, type VacuumStatus } from "./shared.js";

// ── Maintenance / VACUUM state ──────────────────────────────────────

let liveAutoVacuum = true;
let liveVacuumIntervalHours = 6;
let liveUsageRetentionDays = 7;
let vacuumStatus: VacuumStatus = {
  last_run: null, last_result: null, in_progress: false, next_scheduled: null,
};

/** Fetch `/config/maintenance` and seed the maintenance state. Called
 *  by mountConfig; a missing endpoint keeps the defaults (non-fatal). */
export async function loadMaintenanceState(): Promise<void> {
  try {
    const maint = await api("/config/maintenance") as {
      auto_vacuum?: boolean; vacuum_interval_hours?: number; usage_retention_days?: number;
      vacuum_status?: { last_run?: string | null; last_result?: string | null; in_progress?: boolean; next_scheduled?: string | null };
    };
    liveAutoVacuum = maint.auto_vacuum ?? true;
    liveVacuumIntervalHours = maint.vacuum_interval_hours ?? 6;
    liveUsageRetentionDays = maint.usage_retention_days ?? 7;
    if (maint.vacuum_status) {
      vacuumStatus = {
        last_run: maint.vacuum_status.last_run ?? null,
        last_result: maint.vacuum_status.last_result ?? null,
        in_progress: maint.vacuum_status.in_progress ?? false,
        next_scheduled: maint.vacuum_status.next_scheduled ?? null,
      };
    }
  } catch {
  }
}

export async function pollVacuumStatus(): Promise<void> {
  try {
    const data = await api("/config/vacuum-status") as { last_run?: string | null; last_result?: string | null; in_progress?: boolean; next_scheduled?: string | null };
    vacuumStatus = {
      last_run: data.last_run ?? null,
      last_result: data.last_result ?? null,
      in_progress: data.in_progress ?? false,
      next_scheduled: data.next_scheduled ?? null,
    };
    requestUpdate();
  } catch {
  }
}

async function patchMaintenance(): Promise<void> {
  try {
    await api("/config/maintenance", {
      method: "PUT",
      body: JSON.stringify({
        auto_vacuum: liveAutoVacuum,
        vacuum_interval_hours: liveVacuumIntervalHours,
        usage_retention_days: liveUsageRetentionDays,
      }),
    });
    showToast(t("config.maintenance.toast.updated"), "success");
    requestUpdate();
  } catch (e: unknown) {
    showToast(t("config.toast.error", { message: errStr(e) }), "error");
  }
}

async function triggerVacuum(): Promise<void> {
  if (vacuumStatus.in_progress) return;
  vacuumStatus.in_progress = true;
  requestUpdate();
  try {
    const result = await api("/debug/vacuum", { method: "POST" }) as { vacuumed?: boolean; partial?: boolean; integrity_check?: string; message?: string };
    if (result.partial) {
      showToast(t("config.maintenance.toast.vacuum_partial", { message: result.message || "see details" }), "warning");
    } else {
      showToast(t("config.maintenance.toast.vacuum_ok"), "success");
    }
  } catch (e: unknown) {
    // A corrupt DB yields repair instructions in the error; toast them, then
    // try the recover endpoint for diagnostics.
    const errMsg = errStr(e);
    showToast(t("config.maintenance.toast.vacuum_failed", { message: errMsg }), "error");
    // Auto-trigger the recover diagnostic so the operator sees those instructions.
    if (errMsg.includes("disk I/O") || errMsg.includes("integrity")) {
      try {
        const recovery = await api("/debug/recover", { method: "POST" }) as { instructions?: string; tables?: unknown[]; needs_manual_repair?: boolean };
        if (recovery.needs_manual_repair && recovery.instructions) {
          // Longer-lived toast: the full instructions are in the console too.
          showToast(t("config.maintenance.toast.repair_needed"), "error");
          console.error("=== DATABASE REPAIR INSTRUCTIONS ===\n" + recovery.instructions + "\n=== END INSTRUCTIONS ===");
        }
      } catch {
        // Non-fatal: the operator already has the VACUUM error message.
      }
    }
  } finally {
    await pollVacuumStatus();
  }
}

// ── Card template ───────────────────────────────────────────────────

export function renderMaintenanceCard(): TemplateResult {
  const vacuumBtnLabel = vacuumStatus.in_progress
    ? t("config.maintenance.vacuum_in_progress")
    : t("config.maintenance.vacuum_run");
  const lastRunText = vacuumStatus.last_run
    ? new Date(vacuumStatus.last_run).toLocaleString()
    : t("config.maintenance.never");
  const isOk = vacuumStatus.last_result === "ok";
  const lastResultText = vacuumStatus.last_result
    ? (isOk ? t("config.maintenance.result_ok") : t("config.maintenance.result_failed", { result: vacuumStatus.last_result }))
    : "—";
  const nextScheduledText = vacuumStatus.next_scheduled
    ? new Date(vacuumStatus.next_scheduled).toLocaleString()
    : (liveAutoVacuum ? t("config.maintenance.next_scheduled_tick") : t("config.maintenance.next_disabled"));

  return card(t("config.maintenance.title"), html`
    <div class="config-rows-list">
      <div class="config-toggle-row">
        <div class="config-toggle-info">
          <span class="config-label">${t("config.maintenance.auto_vacuum")}</span>
          <span class="config-help">${t("config.maintenance.auto_vacuum_desc", { hours: liveVacuumIntervalHours })}</span>
        </div>
        <button type="button" role="switch" aria-checked=${liveAutoVacuum ? "true" : "false"}
          class="toggle-btn ${liveAutoVacuum ? "on" : "off"}"
          @click=${() => { liveAutoVacuum = !liveAutoVacuum; void patchMaintenance(); }}>
          <span class="toggle-thumb"></span>
        </button>
      </div>

      <div class="config-inline-setting">
        <div class="config-inline-info">
          <span class="config-label">${t("config.maintenance.vacuum_interval")}</span>
          <span class="config-help">Interval in hours between auto-vacuum jobs.</span>
        </div>
        <div class="config-inline-action">
          <div class="config-input-group has-unit">
            <input type="number" inputmode="numeric" min="1" max="168"
              .value=${String(liveVacuumIntervalHours)}
              aria-label=${t("config.maintenance.vacuum_interval")}
              @change=${(e: Event) => {
                const v = parseInt((e.target as HTMLInputElement).value, 10);
                if (v >= 1) { liveVacuumIntervalHours = v; void patchMaintenance(); }
              }}>
            <span class="config-input-unit">h</span>
          </div>
        </div>
      </div>

      <div class="config-inline-setting">
        <div class="config-inline-info">
          <span class="config-label">${t("config.maintenance.usage_retention")}</span>
          <span class="config-help">${t("config.maintenance.usage_retention_desc")}</span>
        </div>
        <div class="config-inline-action">
          <div class="config-input-group has-unit">
            <input type="number" inputmode="numeric" min="0" max="365"
              .value=${String(liveUsageRetentionDays)}
              aria-label=${t("config.maintenance.usage_retention")}
              @change=${(e: Event) => {
                const v = parseInt((e.target as HTMLInputElement).value, 10);
                if (v >= 0) { liveUsageRetentionDays = v; void patchMaintenance(); }
              }}>
            <span class="config-input-unit">d</span>
          </div>
        </div>
      </div>
    </div>

    <div class="config-maintenance-telemetry">
      <div class="telemetry-grid">
        <div class="telemetry-item">
          <span class="telemetry-label">${t("config.maintenance.last_run")}</span>
          <span class="telemetry-value">${lastRunText}</span>
        </div>
        <div class="telemetry-item">
          <span class="telemetry-label">${t("config.maintenance.result")}</span>
          <span class="telemetry-value">
            <span class="status-pill ${isOk ? "active" : (vacuumStatus.last_result ? "inactive" : "")}">
              ${lastResultText}
            </span>
          </span>
        </div>
        <div class="telemetry-item">
          <span class="telemetry-label">${t("config.maintenance.next_scheduled")}</span>
          <span class="telemetry-value">${nextScheduledText}</span>
        </div>
      </div>
      <div class="telemetry-action">
        <button class="primary"
          ?disabled=${vacuumStatus.in_progress}
          @click=${() => void triggerVacuum()}>
          ${vacuumBtnLabel}
        </button>
      </div>
    </div>
  `);
}
