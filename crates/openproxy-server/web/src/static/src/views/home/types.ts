// views/home/types.ts — shared types for the home dashboard modules.

import type uPlot from "uplot";
import type { Snapshot, SnapshotWindow, LiveConnectionState } from "../../state/live-store.js";

export type { Snapshot, SnapshotWindow, LiveConnectionState };

/** uPlot instances + resize observers. Null before the first render. */
export interface ChartInstances {
  throughput: uPlot;
  statusCodes: uPlot;
  latency: uPlot;
  sparkRequests: uPlot;
  sparkSuccess: uPlot;
  sparkLatency: uPlot;
  sparkTokens: uPlot;
  sparkCost: uPlot;
  resizeDisposers: Array<() => void>;
}

/** Scroll state saved pre-render, restored in the post-render `requestAnimationFrame`. */
export interface SavedScroll {
  scrollTop: number;
  scrollHeight: number;
}
