// lit-html re-rendering, replacing the old innerHTML-based
// `rerenderCurrentView()`. A diff touches only the nodes that changed, so
// <select> dropdowns stay open, inputs keep focus, and scroll survives.
//
// A view mounts via `mountView(container, renderFn)`; the router captures the
// returned cleanup.

import { render, type TemplateResult } from 'lit-html';

let currentContainer: HTMLElement | null = null;
let currentRenderFn: (() => TemplateResult) | null = null;
let updateScheduled = false;
let renderBroken = false;

/** Mount a view into `container`. Returns a cleanup function. */
export function mountView(
  container: HTMLElement,
  renderFn: () => TemplateResult,
): () => void {
  currentContainer = container;
  currentRenderFn = renderFn;
  renderBroken = false;
  render(renderFn(), container);
  return () => {
    if (currentContainer === container) {
      currentContainer = null;
      currentRenderFn = null;
    }
  };
}

/** Schedule a re-render on the next microtask. Coalesces calls.
 *
 *  Recovery: a throw out of `render()` (lit-html's repeat directive on an
 *  inconsistent DOM) leaves lit-html's internal state corrupted, so later
 *  renders throw too. Clearing the container drops those refs and the next
 *  render starts fresh, equivalent to an unmount plus remount. */
export function requestUpdate(): void {
  if (updateScheduled) return;
  updateScheduled = true;
  queueMicrotask(() => {
    updateScheduled = false;
    if (
      !currentContainer ||
      !currentRenderFn ||
      !currentContainer.isConnected ||
      currentContainer.childNodes.length === 0
    ) {
      return;
    }
    try {
      if (renderBroken) {
        // Reset lit-html's internal refs before re-rendering.
        delete (currentContainer as { _$litPart$?: unknown })._$litPart$;
        currentContainer.innerHTML = '';
        renderBroken = false;
      }
      render(currentRenderFn(), currentContainer);
    } catch (e) {
      console.error("[openproxy] requestUpdate render() threw:", e);
      renderBroken = true;
        // Retry on the next microtask.
      updateScheduled = true;
      queueMicrotask(() => {
        updateScheduled = false;
        if (currentContainer && currentRenderFn && currentContainer.isConnected) {
          try {
            delete (currentContainer as { _$litPart$?: unknown })._$litPart$;
            currentContainer.innerHTML = '';
            render(currentRenderFn(), currentContainer);
            renderBroken = false;
          } catch (e2) {
            console.error("[openproxy] recovery render also failed:", e2);
            renderBroken = true;
          }
        }
      });
    }
  });
}

/** Force an immediate synchronous re-render. */
export function forceUpdate(): void {
  updateScheduled = false;
  if (
    !currentContainer ||
    !currentRenderFn ||
    !currentContainer.isConnected ||
    currentContainer.childNodes.length === 0
  ) {
    return;
  }
  try {
    if (renderBroken) {
      delete (currentContainer as { _$litPart$?: unknown })._$litPart$;
      currentContainer.innerHTML = '';
      renderBroken = false;
    }
    render(currentRenderFn(), currentContainer);
  } catch (e) {
    console.error("[openproxy] forceUpdate render() threw:", e);
    renderBroken = true;
  }
}
