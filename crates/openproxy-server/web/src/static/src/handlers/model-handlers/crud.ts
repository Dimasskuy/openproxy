// handlers/model-handlers/crud.ts — single-model CRUD: the legacy edit modal, update, delete and
// the custom-model form.
//
// The legacy "Edit model" modal is kept for backwards compatibility (older UI surfaces still call
// it); new code should use the in-table Enable/Disable buttons.

import { state } from "../../state/index.js";
import { api } from "../../state/api.js";
import { html, render } from "lit-html";
import type { Model } from "../../lib/types/api.js";
import { requestUpdate } from "../../state/reactive.js";
import { showToast } from "../../components/toast.js";
import { ensureModalRoot, showApiError } from "../../lib/ui-utils.js";
import { showConfirm } from "../../lib/show-confirm.js";
import { mutateAndRefresh } from "../../lib/mutate.js";

// ── Edit model (legacy) ────────────────────────────────────────────────────

export async function showEditModel(rowId: number): Promise<void> {
  if (!state.modelsComplete) {
    state.models = await api("/models") as Model[];
    state.modelsComplete = true;
  }
  const m = (state.models || []).find((x) => x.row_id === rowId);
  if (!m) { showToast("Model row not found", "error"); return; }
  const wrapper = document.createElement("div");
  ensureModalRoot().appendChild(wrapper);
  // Mount on <body> via #modal-root (not #main) so the 3s background poll can't destroy the
  // form mid-edit. lit-html auto-escapes the model id / display name, so no `escapeAttr`.
  render(html`
    <div class="modal-bg" id="edit-model-modal"
         @click=${(e: Event) => { if (e.target === e.currentTarget) wrapper.remove(); }}>
      <div class="modal">
        <div class="modal-header">
          <h2>Edit model row #${rowId}</h2>
          <button type="button" class="close-btn" @click=${() => wrapper.remove()} aria-label="Close">&times;</button>
        </div>
        <form @submit=${(e: Event) => { e.preventDefault(); void updateModel(rowId, e, wrapper); }}>
          <div class="modal-body">
            <div class="field">
              <label>Model id</label>
              <input name="model_id" type="text" .value=${m.model_id || ""} required>
            </div>
            <div class="field">
              <label>Display name</label>
              <input name="display_name" type="text" .value=${m.display_name || ""}>
            </div>
            <div class="field">
              <label>Active</label>
              <select name="active">
                <option value="true" ?selected=${!!m.active}>yes</option>
                <option value="false" ?selected=${!m.active}>no</option>
              </select>
            </div>
          </div>
          <div class="modal-footer">
            <button type="button" @click=${() => wrapper.remove()}>Cancel</button>
            <button type="submit" class="primary">Save</button>
          </div>
        </form>
      </div>
    </div>
  `, wrapper);
}

export async function updateModel(rowId: number, e: Event, wrapper?: HTMLElement): Promise<void> {
  const target = e.target;
  if (!(target instanceof HTMLFormElement)) return;
  const f = new FormData(target);
  const body = {
    model_id: f.get("model_id"),
    display_name: f.get("display_name") || null,
    active: f.get("active") === "true",
  };
  try {
    await api("/models/" + rowId, { method: "PATCH", body: JSON.stringify(body) });
    state.models = await api("/models") as Model[];
    if (wrapper) wrapper.remove();
    else {
      const modalBg = target.closest(".modal-bg");
      if (modalBg) modalBg.remove();
    }
    requestUpdate();
  } catch (err: unknown) {
    showApiError(err, "Error");
  }
}

export async function deleteModel(rowId: number): Promise<void> {
  if (!(await showConfirm({
    title: "Delete model",
    message: "Delete this model? Combo targets referencing it will be removed too.",
    danger: true,
    confirmLabel: "Delete",
  }))) return;
  await mutateAndRefresh({
    apiCall: async () => {
      await api(`/models/${rowId}`, { method: "DELETE" });
      state.models = state.models.filter((m) => m.row_id !== rowId);
    },
  });
}

// ── Custom model form ──────────────────────────────────────────────────────

// Re-exported from components/model-custom-form.js so the data-action shim has one place to
// find them.
export { showCustomModelForm, closeCustomModelForm } from "../../components/model-custom-form.js";

// POST /admin/models/custom — hand-create a model row. The server stamps it `custom = 1`,
// `active = 1` so it is routable as soon as the modal closes. Close-then-refetch order avoids the
// parent's re-render clobbering the modal mid-close.
export async function createCustomModel(providerId: string, e: Event): Promise<void> {
  const target = e.target;
  if (!(target instanceof HTMLFormElement)) return;
  const f = new FormData(target);
  const body = {
    provider_id: providerId,
    model_id: f.get("model_id"),
    display_name: f.get("display_name") || null,
    model_type: f.get("model_type") || "chat",
    target_format: f.get("target_format"),
    ttl_seconds: parseInt(String(f.get("ttl_seconds"))) || 0,
  };
  try {
    await api("/models/custom", { method: "POST", body: JSON.stringify(body) });
    const modalBg = target.closest(".modal-bg");
    if (modalBg) modalBg.remove();
    state.models = await api("/models") as Model[];
    state.modelsComplete = true;
    requestUpdate();
  } catch (err: unknown) {
    showApiError(err, "Error");
  }
}
