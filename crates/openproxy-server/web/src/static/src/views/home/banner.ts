// views/home/banner.ts — WS connection status banner + header dot.

import { html, type TemplateResult } from "lit-html";
import { t } from "../../i18n/index.js";
import type { LiveConnectionState } from "./types.js";

/** Connection banner above the KPIs. Hidden when connected (the header dot suffices). */
export function renderConnectionBanner(state: LiveConnectionState): TemplateResult {
  if (state === "connected") return html``;
  if (state === "connecting") {
    return html`<div class="home-banner home-banner-warn">
      <span class="home-banner-icon">↻</span>
      <span>${t("home.connecting")}</span>
    </div>`;
  }
  // disconnected
  return html`<div class="home-banner home-banner-error">
    <span class="home-banner-icon">⚠</span>
    <span>${t("home.disconnected")}</span>
  </div>`;
}

/** Header dot: green connected, yellow connecting, red disconnected. */
export function renderConnectionDot(state: LiveConnectionState): TemplateResult {
  const cls: string = state === "connected"
    ? "home-conn-dot home-conn-dot-ok"
    : state === "connecting"
    ? "home-conn-dot home-conn-dot-warn"
    : "home-conn-dot home-conn-dot-err";
  const title: string = state === "connected"
    ? t("home.connected")
    : state === "connecting"
    ? t("home.connecting")
    : t("home.disconnected");
  return html`<span class="${cls}" title=${title}></span>`;
}
