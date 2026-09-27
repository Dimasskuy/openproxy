// Generic responsive table → cards renderer.
//
// Desktop (≥768px): a `<table>` built from the `columns` prop.
// Mobile (<768px): the same `<table>` with class `responsive-card-table`.
// The `@media (max-width: 768px)` rules in components.css read the
// per-cell `data-label` to restyle each row as a stack of cards.
//
// Views with bespoke per-row mobile markup (proxies, keys, providers,
// proxy-sources) must keep that markup: drag/touch handlers and
// focus-preserving in-place updates cannot be expressed through column
// metadata. This helper targets new tables and simple callers.

import { html, nothing, type TemplateResult } from "lit-html";

export interface ResponsiveColumn<T> {
  /** Column key — used for the `data-label` and as the fallback
   *  extractor when `render` is not provided. */
  key: string;
  /** Header text shown on desktop. Used as the `label` prefix on
   *  mobile card cells. */
  label: string;
  /** Optional cell renderer. Receives the row and returns either a
   *  `TemplateResult` (preferred) or a plain string. Plain strings are
   *  rendered as TEXT (lit-html escapes them) — never as raw HTML. Use
   *  a `TemplateResult` when you need markup (e.g. status pills). */
  render?: (row: T) => TemplateResult | string;
  /** If true, this column is hidden on mobile (its `<td>` gets a
   *  `display: none` in the card view via CSS). */
  hiddenMobile?: boolean;
}

export interface ResponsiveTableProps<T> {
  columns: ResponsiveColumn<T>[];
  rows: readonly T[];
  /** Unique key per row. Used as the `<tr>` id and the `data-row-key`
   *  attribute. Required so lit-html can keep `<tr>` identity across
   *  partial re-renders. */
  rowKey: (row: T) => string | number;
  /** Shown when `rows` is empty. Default: "No items." */
  emptyMessage?: string;
  /** Optional class added to the wrapping `<table>`. */
  className?: string;
  /** Optional per-row class (e.g. status colouring: `alive`/`dead`).
   *  The helper always adds `responsive-card-row`; this is appended. */
  rowClass?: (row: T) => string;
}

function renderCell<T>(col: ResponsiveColumn<T>, row: T): TemplateResult {
  if (col.render) {
    const v: TemplateResult | string = col.render(row);
    return typeof v === "string" ? html`${v}` : v;
  }
  const value: unknown = (row as Record<string, unknown>)[col.key];
  return html`${value == null ? "" : String(value)}`;
}

export function renderResponsiveCardTable<T>(props: ResponsiveTableProps<T>): TemplateResult {
  const { columns, rows, rowKey, emptyMessage, className, rowClass } = props;
  const cls: string = "responsive-card-table" + (className ? " " + className : "");
  const empty: string = emptyMessage ?? "No items.";

  if (!rows || rows.length === 0) {
    return html`
      <div class="table-wrap">
        <table class=${cls}>
          <tbody>
            <tr class="empty-row">
              <td colspan=${columns.length} class="empty-row">${empty}</td>
            </tr>
          </tbody>
        </table>
      </div>
    `;
  }

  return html`
    <div class="table-wrap">
      <table class=${cls}>
        <thead>
          <tr>
            ${columns.map((c) => html`<th>${c.label}</th>`)}
          </tr>
        </thead>
        <tbody>
          ${rows.map((row) => html`
            <tr data-row-key=${String(rowKey(row))} class=${("responsive-card-row" + (rowClass ? " " + rowClass(row) : "")).trimEnd()}>
              ${columns.map((c) => html`
                <td data-label=${c.label} class=${c.hiddenMobile ? "hidden-mobile" : ""}>
                  ${renderCell(c, row)}
                </td>
              `)}
            </tr>
          `)}
        </tbody>
      </table>
    </div>
  `;
}

// Re-exported so callers can omit a column value from `render(row)`.
export { nothing };
