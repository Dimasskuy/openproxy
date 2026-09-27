// Typed pub/sub over `WsEnvelope.type`, so modules other than the live-logs
// view can react to WS traffic without re-routing through it.
//
// `state/ws.ts` calls `dispatchWs(msg)` right after `messageHandler(event)`
// returns, so the logs handler always sees a message first and subscribers
// observe the state it already updated.
//
// Subscribers must tolerate repeat delivery: the logs handler and the bus both
// receive every message. The bus is process-global with no "dispose all", so a
// subscriber has to unsubscribe on unmount or its closure keeps the view alive.

import type { WsEnvelope } from "../views/logs.js";

/** Receives the full envelope; the subscriber narrows `data` per `type`. */
export type WsHandler = (msg: WsEnvelope) => void;

// A Set, not an Array: unsubscribe is O(1) and re-subscribing the same
// function is a no-op.
const handlers = new Map<string, Set<WsHandler>>();

/** Subscribe to WS envelopes of a `type`. Returns an unsubscribe function.
 *
 *  `type` is `string`, not the `WsEnvelope["type"]` union, so subscribers can
 *  listen for types that do not exist yet. The bus does not validate the type;
 *  unknown types simply never fire. */
export function subscribeWs(type: string, fn: WsHandler): () => void {
  let set = handlers.get(type);
  if (!set) {
    set = new Set<WsHandler>();
    handlers.set(type, set);
  }
  set.add(fn);
  return () => {
    const s = handlers.get(type);
    if (s) {
      s.delete(fn);
      // Keep the empty set in the map: a later subscribe reuses it, and
      // removing it would mutate the map on every unsubscribe.
    }
  };
}

/** Dispatch an envelope to every subscriber registered for `msg.type`. One
 *  failing subscriber does not block the rest: each call is wrapped in
 *  try/catch and logged. */
export function dispatchWs(msg: WsEnvelope): void {
  const set = handlers.get(msg.type);
  if (!set) return;
  for (const fn of set) {
    try {
      fn(msg);
    } catch (err) {
      // A broken subscriber must not break the bus for the rest.
      console.error(
        "[openproxy] ws-bus subscriber for type",
        msg.type,
        "threw:",
        err,
      );
    }
  }
}

/** Test-only: drops every subscriber. */
export function _clearWsBusForTests(): void {
  handlers.clear();
}
