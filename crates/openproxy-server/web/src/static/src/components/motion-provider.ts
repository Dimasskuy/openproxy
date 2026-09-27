// components/motion-provider.ts — respects the `prefers-reduced-motion` system preference by
// toggling a `reduced-motion` class on `<html>`, and listens for live changes so a mid-
// session OS toggle applies without a reload.
//
// The CSS counterpart (`.reduced-motion *` zeroing animation/transition duration and
// scroll-behavior) lives in `styles/base.css` and overrides per-element rules by cascade
// order; the `!important` is intentional and scoped to users who opted in.

type MediaQueryListChangeHandler = (e: MediaQueryListEvent) => void;

function apply(reduce: boolean): void {
  document.documentElement.classList.toggle("reduced-motion", reduce);
}

let installed: boolean = false;

/** Install the motion provider. Idempotent — safe from boot and later re-renders. */
export function installMotionProvider(): void {
  if (installed) return;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
  const mql: MediaQueryList = window.matchMedia("(prefers-reduced-motion: reduce)");
  apply(mql.matches);
  // `addEventListener` (the legacy `addListener` shim is Safari < 14 only) and
  // `MediaQueryListEvent` carries the new `.matches` value.
  const handler: MediaQueryListChangeHandler = (e: MediaQueryListEvent): void => {
    apply(e.matches);
  };
  mql.addEventListener("change", handler);
  installed = true;
}
