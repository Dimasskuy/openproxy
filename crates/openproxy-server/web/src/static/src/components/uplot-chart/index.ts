// components/uplot-chart/index.ts — facade re-exporting the public API of colors / lifecycle
// / builders so consumers keep importing from "components/uplot-chart.js". The public
// surface is identical to the pre-split monolithic file — no consumer-facing changes.

export { CHART_COLORS, cssVar, type ChartColors } from "./colors.js";

export {
  createLiveChart,
  createSparkline,
  resizeChart,
  observeResize,
  smoothSpline,
  smoothPath,
  type ChartData,
  type LiveChartOpts,
} from "./lifecycle.js";

export {
  buildThroughputChart,
  buildStatusCodesChart,
  buildLatencyChart,
  buildDailyUsageChart,
} from "./builders.js";
