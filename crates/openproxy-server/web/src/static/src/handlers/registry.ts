// handlers/registry.ts — central map from the `data-action` attribute
import { showToast } from "../components/toast.js";
import { copyToClipboard } from "../lib/clipboard.js";
import { navigate, rerenderCurrentView, forceRerenderCurrentView } from "../state/router.js";
// installs a single document-level listener that dispatches clicks / changes / submits based on
// data-action / data-arg-* attrs.
//
// Why: spec §3 + §13.8 forbid window.foo = fn global bridges and inline onclick="window.foo()"
// handlers. A single shim keeps the HTML tidy (data-action="X" data-arg1="...") without
// re-wiring every modal.
//
// Conventions:
//   * `action` is the function name in this map.
//   * `arg1`, `arg2`, … are positional string args collected from data-arg-* attrs in numeric order.
//   * The trailing `e` is always the DOM event, so handlers can call e.preventDefault() and reach
//     e.target.
//   * Form handlers that take an event (createKey, updateKey, updateModel, addTarget,
//     createAccount, createCombo) receive it as the LAST argument, matching how they used to be
//     invoked from `onsubmit=`; the submit listener calls preventDefault() before dispatching.
//   * Self-only closures (closeKeyForm, etc.) get the bound element (data-arg1="self") as the arg.

import { showCreateAccount, createAccount, closeCreateAccount, deleteAccount, showUpdateAccountKey, updateAccountKey, closeUpdateAccountKey, showReauthAccount } from "./account-handlers.js";
import {
  showCreateCombo, createCombo, closeCreateCombo, deleteCombo, updateRaceSize, updateContextWindow, testAllTargets,
  onCreatePriorityModeChange, onCreateCooldownModeChange,
  updatePriorityMode, updateCooldownMode,
  updateCooldownBase, updateCooldownFactor, updateCooldownMax,
  updateLkgpExplorationRate, updateSelectionWindow,
} from "./combo-handlers.js";
import {
  showAddTarget, closeAddTarget, onTargetKindChange,
  addTarget, deleteTarget, resetCooldown, changePriority,
  toggleTargetSelection, toggleSelectAllTargets, clearTargetSelection, bulkDeleteSelectedTargets,
  onModelCheckboxChange,
  selectAllModelsInModal,
  deselectAllModelsInModal,
  onTargetModelSearch,
  updateTargetWeight,
} from "./combo-target-handlers/index.js";
import { showCreateKey, showEditKey, closeKeyForm, toggleExpiryAmount, createKey, updateKey, regenerateKey, revokeKey, viewKeyUsage, deleteKey } from "./key-handlers.js";
import {
  showEditModel, updateModel,
  toggleModel, testModel, deleteModel,
  toggleModelSelection, toggleSelectAllModels, clearModelSelection,
  bulkEnableSelected, bulkDisableSelected, bulkTestSelected, bulkDeleteSelected,
  updateProviderFilter, updateAutoActivate, createCustomModel, showCustomModelForm, closeCustomModelForm,
  cycleProviderSort,
} from "./model-handlers/index.js";
import {
  refreshProvider, refreshAllProviders,
  showCreateProvider, closeCreateProvider, createProvider,
  confirmDeleteProvider, deleteProvider,
  toggleProviderActive, renameProviderPrompt, editProviderEndpointPrompt, bulkToggleModels,
  setHealth, refreshAccountQuota, refreshAllQuotas,
} from "./provider-handlers/index.js";
import { exportConfig } from "./config-handlers.js";
import { exportLogsCSV } from "./log-handlers.js";
import {
  openModelPickerModal, closeModelPickerModal, clearModelPicker,
  toggleModelPicker, filterModelPicker, removeModelFromKey,
} from "../components/model-picker.js";
import { mountThemeToggle } from "../components/theme-toggle.js";
import { toggleSidebar, toggleMobileNav, closeMobileNav, logout } from "../components/sidebar.js";
import { OAuthLogin } from "./oauth-handlers.js";
import { logsPrevPage, logsNextPage, logsGoPage, logsSetFollow, toggleColumnsMenu, toggleColumn } from "../views/logs.js";
import { configSaveTimeouts, configSaveRecordingTtl, configSaveIdleChunkRetryable, configSaveCompression } from "../views/config/index.js";
import { closeLogDetailModal, copyDebugBundle } from "../components/log-detail/index.js";
import { syncProxies, testProxy, testAllProxies, deleteProxy, showAddCustomProxy } from "./proxy-handlers.js";
import { showAddProxySource, showEditProxySource, deleteProxySource, testProxySource, toggleProxySourceActive } from "./proxy-source-handlers.js";

// -- Action registry --------------------------------------------------------
// Keys are the data-action values; each maps to the function to invoke. Positional args come
// from data-arg1, data-arg2, …; the DOM event is always the last argument.
//
// Handlers have widely different shapes (string id + event, boolean + event, event alone, no
// args), so a single narrow type would force every handler into one shape. The value is
// therefore a callable accepting any positional args plus an event; app.ts passes
// `collectArgs(el)` then the DOM event and each handler validates its own contract.
//
// Declared as a *function type* (not an interface) because an `interface { (a: string): void }`
// is contravariant in its parameters, so typed handlers are not assignable to it. Accepting
// `any` — the only top type that doesn't create that incompatibility — is what lets both typed
// handlers and untyped closures coexist; runtime validation is the caller's job.
export type ActionHandler = (...args: unknown[]) => unknown;

export const HANDLERS = {
  // Accounts
  showCreateAccount,
  createAccount,        // signature: (providerId, e)  — submit handler
  closeCreateAccount,
  deleteAccount,
  showUpdateAccountKey,
  updateAccountKey,     // signature: (id, e)          — submit handler
  closeUpdateAccountKey,
  showReauthAccount,

  // Combos
  showCreateCombo,
  createCombo,          // signature: (e)              — submit handler
  closeCreateCombo,
  deleteCombo,
  updateRaceSize,
  updateContextWindow,  // signature: (comboId, e)     — input change
  testAllTargets,       // signature: (comboId, e)     — button click
  onCreatePriorityModeChange,   // signature: ()        — select change (create modal)
  onCreateCooldownModeChange,   // signature: ()        — select change (create modal)
  updatePriorityMode,           // signature: (comboId, e) — select change
  updateCooldownMode,           // signature: (comboId, e) — select change
  updateCooldownBase,           // signature: (comboId, e) — input change (filtered)
  updateCooldownFactor,         // signature: (comboId, e) — input change (filtered)
  updateCooldownMax,            // signature: (comboId, e) — input change (filtered)
  updateLkgpExplorationRate,    // signature: (comboId, e) — input change (filtered)
  updateSelectionWindow,        // signature: (comboId, e) — input change (filtered)

  // Combo targets
  showAddTarget,
  closeAddTarget,
  onTargetKindChange,
  onModelCheckboxChange,
  selectAllModelsInModal,
  deselectAllModelsInModal,
  onTargetModelSearch,
  addTarget,            // signature: (comboId, e)     — submit handler
  deleteTarget,
  resetCooldown,
  changePriority,
  toggleTargetSelection,
  toggleSelectAllTargets,
  clearTargetSelection,
  bulkDeleteSelectedTargets,
  updateTargetWeight,   // signature: (comboId, targetId, e) — input change (filtered)

  // Keys
  showCreateKey,
  showEditKey,
  closeKeyForm,
  toggleExpiryAmount,
  createKey,            // signature: (e)              — submit handler
  updateKey,            // signature: (id, e)          — submit handler
  regenerateKey,
  revokeKey,
  viewKeyUsage,
  deleteKey,

  // Models (provider-detail)
  showEditModel,
  updateModel,          // signature: (rowId, e)       — submit handler
  toggleModel,          // (rowId, newActive, e)
  testModel,            // (rowId, modelId, e)
  deleteModel,          // (rowId)
  toggleModelSelection, // (rowId, e)
  toggleSelectAllModels,
  clearModelSelection,
  bulkEnableSelected,
  bulkDisableSelected,
  bulkTestSelected,
  bulkDeleteSelected,
  updateProviderFilter, // (providerId, key, value)
  updateAutoActivate,   // (providerId, e)
  createCustomModel,    // (providerId, e)              — submit handler
  showCustomModelForm,
  closeCustomModelForm,
  cycleProviderSort,    // (providerId, sortKey, e)    — click on sortable <th>

  // Providers (per-provider actions)
  refreshProvider,        // (providerId, e)
  refreshAllProviders,
  showCreateProvider,
  closeCreateProvider,
  createProvider,         // signature: (e)              — submit handler
  confirmDeleteProvider,
  deleteProvider,
  toggleProviderActive,   // (providerId, newActive)
  renameProviderPrompt,   // (providerId, currentName)
  editProviderEndpointPrompt, // (providerId, currentBaseUrl)
  bulkToggleModels,       // (providerId, active)

  // Account health / quota (exposed on the provider detail view)
  setHealth,              // (id, e)
  refreshAccountQuota,    // (accountId, e)
  refreshAllQuotas,       // (providerId)

  // Config
  configSaveTimeouts,
  configSaveRecordingTtl,
  configSaveIdleChunkRetryable,
  configSaveCompression,
  toggleIdleChunkRetryable(e: Event | null): void {
    if (!e || !e.target) return;
    const btn = (e.target as Element).closest('button[data-action="toggleIdleChunkRetryable"]') as HTMLButtonElement | null;
    if (!btn) return;
    const input = btn.parentElement?.querySelector('input[name="idle_chunk_retryable"]') as HTMLInputElement | null;
    if (!input) return;
    input.checked = !input.checked;
    btn.classList.toggle("on", input.checked);
    btn.classList.toggle("off", !input.checked);
    btn.setAttribute("aria-checked", String(input.checked));
    const help = btn.parentElement?.querySelector(".config-help");
    if (help) {
      help.textContent = input.checked
        ? "ON — idle chunk timeouts allow retry via next target"
        : "OFF — idle chunk timeouts return error immediately (default)";
    }
  },
  exportConfig,

  // Logs
  logsPrevPage,
  logsNextPage,
  logsGoPage,
  logsSetFollow,
  exportLogsCSV,
  // Columns visibility (logs view)
  toggleColumnsMenu,
  toggleColumn,

  // Model picker (singleton)
  openModelPickerModal,
  closeModelPickerModal,
  clearModelPicker,
  toggleModelPicker,
  filterModelPicker,
  removeModelFromKey,

  // Log detail modal
  closeLogDetailModal,
  // Copy a Markdown debug bundle for the open log row; the button is in the modal header.
  copyDebugBundle,

  // Generic modal-bg closer: removes the click target's closest .modal-bg. For modals
  // without a stable ID.
  closeModalBg(e: Event | null): void {
    if (!e || !e.target) return;
    const target = e.target as Element;
    const el = target.closest ? target.closest(".modal-bg") : null;
    if (el) el.remove();
  },

  // Copy #oauth-auth-url; used by the OAuth "Copy" button in views/providers.js.
  copyAuthUrl(): void {
    const el = document.getElementById("oauth-auth-url") as HTMLInputElement | null;
    if (el) {
      copyToClipboard(el.value || "").catch(() => { /* ignore — silent best-effort */ });
    }
  },

  // Plaintext-key modal "I've saved it": close the modal-bg and re-navigate so the key list
  // repaints.
  closeAndNavigate(e: Event | null): void {
    if (e && e.target) {
      const target = e.target as Element;
      const el = target.closest ? target.closest(".modal-bg") : null;
      if (el) el.remove();
    }
    navigate();
  },

  // Toggle which log-detail modal section is visible. The `.active` tab indicator is set by an
  // inline listener registered in showLogDetail().
  logDetailTab(which: string): void {
    document.querySelectorAll("#log-detail-content [data-log-tab]").forEach((sec) => {
      const el = sec as HTMLElement;
      el.style.display = (sec.getAttribute("data-log-tab") === which) ? "" : "none";
    });
    document.querySelectorAll(".log-detail-tabs .detail-tab").forEach((btn) => {
      const b = btn as HTMLElement;
      b.classList.toggle("active", b.getAttribute("data-arg1") === which);
    });
  },

  // Theme toggle (sidebar calls it through addEventListener; exposed as an action too).
  mountThemeToggle,

  // Sidebar collapse toggle; persists the choice to localStorage.
  toggleSidebar,
  toggleMobileNav,
  closeMobileNav,

  // Sidebar Logout button: wipes the stored admin token and navigates to the login route.
  // See `components/sidebar.ts::logout` for why bg-poll / WS are not stopped here.
  logout,

  // Router utilities (data-action friendly). The router also keeps window.navigate /
  // window.rerenderCurrentView aliases for internal callers (bg-poll, hand-written handlers).
  navigate,
  rerenderCurrentView,
  forceRerenderCurrentView,

  // OAuth (OAuthLogin's methods are exposed under flat names so the HTML stays simple,
  // e.g. data-action="oauthStartPKCE").
  oauthStartPKCE:        (provider: string) => OAuthLogin.startPKCE(provider),
  oauthStartDeviceCode:  (provider: string) => OAuthLogin.startDeviceCode(provider),
  oauthSubmitManualCallback: () => OAuthLogin.submitManualCallback(),

  // Toast — ad-hoc console debugging.
  showToast,

  // Free Proxies
  syncProxies,
  testProxy,
  testAllProxies,
  deleteProxy,
  showAddCustomProxy,

  // Proxy Sources
  showAddProxySource,
  showEditProxySource,
  deleteProxySource,
  testProxySource,
  toggleProxySourceActive,
};

// Collect positional data-arg-N attrs from an element, skipping the "action" key, in arg1..argN
// order. Finite-integer values come back as Numbers and the literal strings "true"/"false" are
// coerced, so handlers don't re-parse.
export function collectArgs(el: HTMLElement): unknown[] {
  const args: unknown[] = [];
  for (const key in el.dataset) {
    if (key === "action") continue;
    const m = key.match(/^arg(\d+)$/);
    if (!m) continue;
    const n = parseInt(m[1] || "0", 10) - 1;
    const v = el.dataset[key];
    // Auto-coerce numbers (only when the whole value is a JSON number).
    if (v !== undefined && /^-?\d+(\.\d+)?$/.test(v)) {
      args[n] = Number(v);
    }
    // Auto-coerce booleans "true"/"false".
    else if (v === "true") {
      args[n] = true;
    } else if (v === "false") {
      args[n] = false;
    }
    // Everything else stays a string.
    else {
      args[n] = v;
    }
  }
  return args;
}
