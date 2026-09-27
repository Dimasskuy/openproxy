// components/render-responsive-card-table.test.ts — unit tests for the generic responsive
// table renderer: markup shape (table on desktop, data-label hooks for the mobile CSS) and
// the empty state. The responsive switch itself is CSS, not JS.

import { describe, it, expect } from "vitest";
import { html as litHtml, render } from "lit-html";
import { renderResponsiveCardTable } from "./render-responsive-card-table.js";
import type { TemplateResult } from "lit-html";

interface Row {
  id: number;
  host: string;
  status: string;
}

const ROWS: readonly Row[] = [
  { id: 1, host: "1.2.3.4", status: "alive" },
  { id: 2, host: "5.6.7.8", status: "dead" },
];

function toString(tpl: TemplateResult): string {
  const container = document.createElement("div");
  render(tpl, container);
  // Strip lit-html's `<!--?lit$…-->` binding markers so nested-template markup matches contiguously.
  return container.innerHTML.replace(/<!--[\s\S]*?-->/g, "");
}

describe("renderResponsiveCardTable", () => {
  it("renders a table with thead headers from the columns prop", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        { key: "host", label: "Host" },
        { key: "status", label: "Status" },
      ],
      rows: ROWS,
      rowKey: (r) => r.id,
    }));
    // lit-html inserts `<!--?lit$…-->` markers inside <th>, so assert on text+tag, not exact markup.
    expect(html).toContain("<th");
    expect(html).toContain("Host");
    expect(html).toContain("Status");
    expect(html).toContain("responsive-card-table");
  });

  it("renders a row per data row with data-row-key and data-label", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        { key: "host", label: "Host" },
        { key: "status", label: "Status" },
      ],
      rows: ROWS,
      rowKey: (r) => r.id,
    }));
    expect(html).toContain('data-row-key="1"');
    expect(html).toContain('data-row-key="2"');
    expect(html).toContain('data-label="Host"');
    expect(html).toContain('data-label="Status"');
    expect(html).toContain("1.2.3.4");
    expect(html).toContain("alive");
  });

  it("uses the custom `render` callback when provided", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        { key: "host", label: "Host" },
        {
          key: "status",
          label: "Status",
          render: (r) => litHtml`<strong>${r.status.toUpperCase()}</strong>`,
        },
      ],
      rows: ROWS,
      rowKey: (r) => r.id,
    }));
    expect(html).toContain("<strong>ALIVE</strong>");
    expect(html).toContain("<strong>DEAD</strong>");
  });

  it("renders plain-string `render` results as text, never as raw HTML", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        {
          key: "status",
          label: "Status",
          render: (r) => `<img src=x onerror=alert(1)>${r.status}`,
        },
      ],
      rows: ROWS,
      rowKey: (r) => r.id,
    }));
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img src=x onerror=alert(1)&gt;alive");
  });

  it("marks hiddenMobile columns with the hidden-mobile class", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        { key: "host", label: "Host" },
        { key: "status", label: "Status", hiddenMobile: true },
      ],
      rows: ROWS,
      rowKey: (r) => r.id,
    }));
    expect(html).toContain('class="hidden-mobile"');
  });

  it("renders the empty state when rows is empty", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [
        { key: "host", label: "Host" },
        { key: "status", label: "Status" },
      ],
      rows: [],
      rowKey: (r) => r.id,
      emptyMessage: "No proxies found.",
    }));
    expect(html).toContain("No proxies found.");
    expect(html).toContain("empty-row");
  });

  it("falls back to a default empty message", () => {
    const html = toString(renderResponsiveCardTable<Row>({
      columns: [{ key: "host", label: "Host" }],
      rows: [],
      rowKey: (r) => r.id,
    }));
    expect(html).toContain("No items.");
  });
});
