// components/theme-toggle.ts — circular button at the sidebar's bottom-left; clicking it
// toggles `data-theme` between light and dark. Exposes `mountThemeToggle(): void` for
// the sidebar to call after rendering.
//

import { html, render } from "lit-html";
import { toggleTheme, getTheme } from "../state/theme.js";
import { icons } from "../lib/icons.js";

function renderThemeToggle(slot: HTMLElement): void {
  const isDark = getTheme() === "dark";
  render(
    html`<button
      id="theme-toggle"
      class="theme-toggle"
      type="button"
      title="Toggle light / dark theme"
      aria-label="Toggle theme"
      @click=${(): void => {
        toggleTheme();
        // Re-render so the icon swaps to the new theme.
        renderThemeToggle(slot);
      }}
    >
      ${isDark ? icons.sun() : icons.moon()}
    </button>`,
    slot,
  );
}

export function mountThemeToggle(): void {
  const slot: HTMLElement | null = document.getElementById("theme-toggle-slot");
  if (!slot) return;
  // lit-html's `render()` diffs against existing children, so re-rendering a slot that
  // already holds a button updates it instead of appending a duplicate.
  renderThemeToggle(slot);
}
