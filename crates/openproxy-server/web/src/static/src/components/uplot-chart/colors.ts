// components/uplot-chart/colors.ts — CHART_COLORS palette + CSS variable resolver.
//
// `cssVar` lives here (not lifecycle.ts) because it is a theme-color helper used by the
// chart builders and by CHART_COLORS' design-token alignment; keeping both in one file
// makes the theme/canvas-color contract self-contained.

/** Resolve a CSS custom property at call time, with a dark-grey fallback when `window` is
 *  unavailable (SSR) or the property is undefined. */
export function cssVar(name: string): string {
  if (typeof window === "undefined") return "#5a5a5a";
  const v: string = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || "#5a5a5a";
}

/** Chart series colors, kept in sync with the design tokens. Read at module load, NOT at
 *  theme-change time, so a live theme switch would need `getComputedStyle` — out of scope
 *  for F6. */
export const CHART_COLORS = {
  blue: "#38bdf8",
  green: "#4ade80",
  orange: "#fb923c",
  red: "#f87171",
  purple: "#a855f7",
  gray: "#94a3b8",
  status2xx: "#4ade80",
  status4xx: "#fbbf24",
  status5xx: "#f87171",
} as const;

/** Re-exported palette type for consumers that constrain their own constants to it. */
export type ChartColors = typeof CHART_COLORS;
