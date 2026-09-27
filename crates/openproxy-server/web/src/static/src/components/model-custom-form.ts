// components/model-custom-form.ts — the "Add custom model" modal. Defaults the format
// selector to whatever protocol the provider already speaks (anthropic for anthropic
// providers, openai otherwise) so the user only overrides it when the model differs.
//
// `createCustomModel` is imported from `handlers/model-handlers.ts` directly instead of
// going through the `data-action` registry. The cycle is safe: the binding is only
// referenced inside an `@submit` closure, never at module top level.
//
// Rendered into a fresh wrapper `<div>` under `#modal-root` via `render()`; closing
// removes the wrapper.

import { html, render, type TemplateResult } from "lit-html";
import { state } from "../state/index.js";
import { createCustomModel } from "../handlers/model-handlers/index.js";
import { ensureModalRoot } from "../lib/ui-utils.js";
import type { Provider } from "../lib/types/api.js";

function customModelFormTemplate(providerId: string): TemplateResult {
  const provider: Provider | undefined = state.providers.find((p) => p.id === providerId);
  const defaultFormat: string = provider && provider.format && provider.format !== "mixed" ? provider.format : "openai";
  return html`
    <div class="modal-bg" id="custom-model-modal" @click=${(e: Event) => { if (e.target === e.currentTarget) closeCustomModelForm(); }}>
      <div class="modal">
        <div class="modal-header">
          <h2>Custom model for ${providerId}</h2>
          <button type="button" class="close-btn" @click=${closeCustomModelForm} aria-label="Close">&times;</button>
        </div>
        <form @submit=${(e: Event) => { e.preventDefault(); createCustomModel(providerId, e); }}>
          <div class="modal-body">
            <div class="field">
              <label for="custom-model-id">Model ID</label>
              <input id="custom-model-id" name="model_id" type="text" required placeholder="my-custom-model">
            </div>
            <div class="field">
              <label for="custom-model-display">Display name</label>
              <input id="custom-model-display" name="display_name" type="text" placeholder="My custom model">
            </div>
            <div class="field">
              <label for="custom-model-type">Modality / Model Type</label>
              <select id="custom-model-type" name="model_type">
                <option value="chat">Chat / LLM</option>
                <option value="image">Image Generation</option>
                <option value="embedding">Text Embeddings</option>
                <option value="audio">Audio Transcription</option>
                <option value="rerank">Reranker</option>
                <option value="decision">Decision</option>
              </select>
            </div>
            <div class="field">
              <label for="custom-model-format">Target format</label>
              <select id="custom-model-format" name="target_format">
                <option value="openai" ?selected=${defaultFormat === "openai"}>OpenAI Chat Completions (/v1/chat/completions)</option>
                <option value="responses" ?selected=${defaultFormat === "responses"}>OpenAI Responses (/v1/responses)</option>
                <option value="anthropic" ?selected=${defaultFormat === "anthropic"}>Anthropic Messages (/v1/messages)</option>
                <option value="gemini" ?selected=${defaultFormat === "gemini"}>Google Gemini (generateContent)</option>
                <option value="systemone" ?selected=${defaultFormat === "systemone"}>SystemOne (Decision / Fast Engine)</option>
                <option value="atomesus" ?selected=${defaultFormat === "atomesus"}>Atomesus</option>
                <option value="commandcodego" ?selected=${defaultFormat === "commandcodego"}>CommandCodeGo</option>
              </select>
            </div>
            <div class="field">
              <label for="custom-model-ttl">TTL (seconds, 0 = never expires)</label>
              <input id="custom-model-ttl" name="ttl_seconds" type="number" value="0">
            </div>
          </div>
          <div class="modal-footer">
            <button type="button" @click=${closeCustomModelForm}>Cancel</button>
            <button type="submit" class="primary">Create</button>
          </div>
        </form>
      </div>
    </div>
  `;
}

export function showCustomModelForm(providerId: string): void {
  const root = ensureModalRoot();
  // Fresh wrapper so a future re-render diffs cleanly; removed by closeCustomModelForm
  // via the closest `.modal-bg` lookup.
  const wrapper = document.createElement("div");
  root.appendChild(wrapper);
  render(customModelFormTemplate(providerId), wrapper);
}

export function closeCustomModelForm(): void {
  const m: HTMLElement | null = document.getElementById("custom-model-modal");
  if (!m) return;
  // The modal lives inside the wrapper created above; drop it too so #modal-root stays clean.
  const wrapper = m.parentElement;
  m.remove();
  if (wrapper && wrapper.children.length === 0 && wrapper.parentElement?.id === "modal-root") {
    wrapper.remove();
  }
}
