// components/model-bulk-actions.ts — the "N selected" bar above the models table,
// shown whenever a checkbox is ticked. Pure function of `state.selectedModels`,
// re-rendered by the parent view on every model-handler state change.
//
// Clicks wire `@click` directly (no `data-action` registry). The cycle with
// `handlers/model-handlers.ts` is safe: the bindings are referenced at click time only,
// never at module top level. The "0 selected" count is patched in place by
// `updateBulkBar` in model-handlers.ts.

import { html, type TemplateResult } from "lit-html";
import {
  bulkEnableSelected,
  bulkDisableSelected,
  bulkTestSelected,
  bulkDeleteSelected,
  clearModelSelection,
} from "../handlers/model-handlers/index.js";

export function renderBulkActionsBar(providerId: string): TemplateResult {
  return html`
    <div class="bulk-actions-bar">
      <span><strong>0</strong> selected</span>
      <button @click=${() => bulkEnableSelected(providerId)}>Enable selected</button>
      <button @click=${() => bulkDisableSelected(providerId)}>Disable selected</button>
      <button @click=${() => bulkTestSelected(providerId)}>Test selected</button>
      <button class="danger" @click=${() => bulkDeleteSelected(providerId)}>Delete selected</button>
      <button class="link" @click=${clearModelSelection}>Clear selection</button>
    </div>
  `;
}
