// views/config/backup.ts — Export and restore database state & configuration.

import { html, type TemplateResult } from "lit-html";
import { api } from "../../state/api.js";
import { requestUpdate } from "../../state/reactive.js";
import { showToast } from "../../components/toast.js";
import { showConfirm } from "../../lib/show-confirm.js";
import { rerenderCurrentView } from "../../state/router.js";
import { t } from "../../i18n/index.js";
import { card, errStr } from "./shared.js";

// ── State ───────────────────────────────────────────────────────────

let exportPassphrase = "";
let isExporting = false;

let restoreFile: File | null = null;
let restoreBundle: unknown = null;
let restorePassphrase = "";
let isEncryptedBundle = false;
let isValidating = false;
let isRestoring = false;

export interface ValidationSummary {
  version: number;
  encrypted: boolean;
  exported_at: string;
  openproxy_version: string;
  providers_count: number;
  accounts_count: number;
  models_count: number;
  combos_count: number;
  combo_targets_count: number;
  proxy_sources_count: number;
  api_keys_count: number;
  app_config_count: number;
  warnings: string[];
}

let validationSummary: ValidationSummary | null = null;

// ── Handlers ────────────────────────────────────────────────────────

export async function downloadBackup(): Promise<void> {
  if (isExporting) return;
  isExporting = true;
  requestUpdate();

  try {
    const qs = exportPassphrase.trim()
      ? `?passphrase=${encodeURIComponent(exportPassphrase.trim())}`
      : "";
    const res = await api(`/backup/export${qs}`);
    const jsonStr = typeof res === "string" ? res : JSON.stringify(res, null, 2);

    const blob = new Blob([jsonStr], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "openproxy-backup.json";
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    showToast(t("config.backup.toast.export_success"), "success");
    exportPassphrase = "";
  } catch (e: unknown) {
    showToast(t("config.toast.error", { message: errStr(e) }), "error");
  } finally {
    isExporting = false;
    requestUpdate();
  }
}

export function onBackupFileSelected(event: Event): void {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0];
  if (!file) return;

  restoreFile = file;
  validationSummary = null;

  const reader = new FileReader();
  reader.onload = (e) => {
    try {
      const parsed = JSON.parse(e.target?.result as string) as { encrypted?: boolean };
      restoreBundle = parsed;
      isEncryptedBundle = Boolean(parsed.encrypted);
      requestUpdate();
    } catch {
      showToast(t("config.backup.toast.invalid_json"), "error");
      restoreFile = null;
      restoreBundle = null;
      requestUpdate();
    }
  };
  reader.readAsText(file);
}

export async function validateBackup(): Promise<void> {
  if (!restoreBundle || isValidating) return;
  isValidating = true;
  requestUpdate();

  try {
    const res = await api("/backup/validate", {
      method: "POST",
      body: JSON.stringify({
        passphrase: restorePassphrase.trim() || undefined,
        bundle: restoreBundle,
      }),
    }) as ValidationSummary;

    validationSummary = res;
    showToast(t("config.backup.toast.validate_success"), "info");
  } catch (e: unknown) {
    showToast(t("config.backup.toast.validate_failed", { message: errStr(e) }), "error");
  } finally {
    isValidating = false;
    requestUpdate();
  }
}

export async function executeRestore(): Promise<void> {
  if (!restoreBundle || isRestoring) return;

  const confirmed = await showConfirm({
    title: t("config.backup.confirm.title"),
    message: t("config.backup.confirm.body"),
    danger: true,
    confirmLabel: t("config.backup.confirm.button"),
  });

  if (!confirmed) return;

  isRestoring = true;
  requestUpdate();

  try {
    const res = await api("/backup/restore", {
      method: "POST",
      body: JSON.stringify({
        passphrase: restorePassphrase.trim() || undefined,
        bundle: restoreBundle,
      }),
    }) as { message: string };

    showToast(res.message || t("config.backup.toast.restore_success"), "success");
    restoreFile = null;
    restoreBundle = null;
    restorePassphrase = "";
    validationSummary = null;
    isEncryptedBundle = false;

    rerenderCurrentView();
  } catch (e: unknown) {
    showToast(t("config.backup.toast.restore_failed", { message: errStr(e) }), "error");
  } finally {
    isRestoring = false;
    requestUpdate();
  }
}

// ── Template ────────────────────────────────────────────────────────

export function renderBackupCard(): TemplateResult {
  return card(t("config.backup.title"), html`
    <div class="config-rows-list">
      <div class="banner banner-warning backup-warning-banner">
        <div class="banner-body-wrap">
          <strong class="banner-title">${t("config.backup.warning_title")}</strong>
          <span class="banner-text">${t("config.backup.warning_body")}</span>
        </div>
      </div>

      <!-- Export Section -->
      <div class="config-section-title">
        <h4>${t("config.backup.export_subtitle")}</h4>
      </div>

      <div class="config-inline-setting">
        <div class="config-inline-info">
          <span class="config-label">${t("config.backup.passphrase_label")}</span>
          <span class="config-help">${t("config.backup.passphrase_help")}</span>
        </div>
        <div class="config-inline-action">
          <input
            type="password"
            class="config-input"
            placeholder=${t("config.backup.passphrase_placeholder")}
            .value=${exportPassphrase}
            @input=${(e: Event) => {
              exportPassphrase = (e.target as HTMLInputElement).value;
            }}
          />
        </div>
      </div>

      <div class="backup-action-row">
        <button
          type="button"
          class="primary"
          ?disabled=${isExporting}
          @click=${() => void downloadBackup()}
        >
          ${isExporting ? t("config.backup.downloading") : t("config.backup.download_button")}
        </button>
      </div>

      <hr class="config-divider" />

      <!-- Restore Section -->
      <div class="config-section-title">
        <h4>${t("config.backup.restore_subtitle")}</h4>
      </div>

      <div class="config-inline-setting">
        <div class="config-inline-info">
          <span class="config-label">${t("config.backup.select_file_label")}</span>
          <span class="config-help">${t("config.backup.select_file_help")}</span>
        </div>
        <div class="config-inline-action">
          <input
            type="file"
            id="backup-file-input"
            accept=".json,application/json"
            @change=${onBackupFileSelected}
          />
        </div>
      </div>

      ${restoreFile ? html`
        <div class="backup-file-info">
          <span class="backup-file-name">📄 ${restoreFile.name} (${Math.round(restoreFile.size / 1024)} KB)</span>
          ${isEncryptedBundle ? html`
            <span class="status-pill inactive">${t("config.backup.badge_encrypted")}</span>
          ` : html`
            <span class="status-pill active">${t("config.backup.badge_plain")}</span>
          `}
        </div>

        ${isEncryptedBundle ? html`
          <div class="config-inline-setting">
            <div class="config-inline-info">
              <span class="config-label">${t("config.backup.restore_passphrase_label")}</span>
              <span class="config-help">${t("config.backup.restore_passphrase_help")}</span>
            </div>
            <div class="config-inline-action">
              <input
                type="password"
                class="config-input"
                placeholder=${t("config.backup.restore_passphrase_placeholder")}
                .value=${restorePassphrase}
                @input=${(e: Event) => {
                  restorePassphrase = (e.target as HTMLInputElement).value;
                }}
              />
            </div>
          </div>
        ` : ""}

        <div class="backup-action-row">
          <button
            type="button"
            class="secondary"
            ?disabled=${isValidating}
            @click=${() => void validateBackup()}
          >
            ${isValidating ? t("config.backup.validating") : t("config.backup.validate_button")}
          </button>

          <button
            type="button"
            class="primary danger"
            ?disabled=${isRestoring}
            @click=${() => void executeRestore()}
          >
            ${isRestoring ? t("config.backup.restoring") : t("config.backup.restore_button")}
          </button>
        </div>

        ${validationSummary ? html`
          <div class="backup-validation-box">
            <h5>${t("config.backup.summary_title")} (v${validationSummary.version})</h5>
            <div class="backup-summary-grid">
              <span class="chip">${t("config.backup.providers")}: <strong>${validationSummary.providers_count}</strong></span>
              <span class="chip">${t("config.backup.accounts")}: <strong>${validationSummary.accounts_count}</strong></span>
              <span class="chip">${t("config.backup.models")}: <strong>${validationSummary.models_count}</strong></span>
              <span class="chip">${t("config.backup.combos")}: <strong>${validationSummary.combos_count}</strong></span>
              <span class="chip">${t("config.backup.proxy_sources")}: <strong>${validationSummary.proxy_sources_count}</strong></span>
              <span class="chip">${t("config.backup.api_keys")}: <strong>${validationSummary.api_keys_count}</strong></span>
            </div>
            ${validationSummary.warnings.length > 0 ? html`
              <div class="backup-warnings-list">
                ${validationSummary.warnings.map(w => html`<div>⚠️ ${w}</div>`)}
              </div>
            ` : ""}
          </div>
        ` : ""}
      ` : ""}
    </div>
  `);
}
