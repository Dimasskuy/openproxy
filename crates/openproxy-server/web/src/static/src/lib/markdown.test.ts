import { describe, expect, it } from 'vitest';
import { isSafeLinkHref, renderMarkdownAndMath } from './markdown.js';

describe('renderMarkdownAndMath — HTML injection hardening', () => {
  it('escapes HTML inside display math ($$…$$)', () => {
    const out = renderMarkdownAndMath('$$<img src=x onerror=alert(1)>$$');
    expect(out).not.toContain('<img');
    expect(out).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(out).toContain('class="math-block"');
  });

  it('escapes HTML inside \\[…\\] display math', () => {
    const out = renderMarkdownAndMath('\\[<style>body{display:none}</style>\\]');
    expect(out).not.toContain('<style');
    expect(out).toContain('&lt;style&gt;');
  });

  it('escapes HTML inside inline math ($…$ and \\(…\\))', () => {
    const a = renderMarkdownAndMath('total $x<b onmouseover=alert(1)>y$ done');
    expect(a).not.toContain('<b ');
    expect(a).toContain('class="math-inline"');

    const b = renderMarkdownAndMath('\\(<svg onload=alert(1)>\\)');
    expect(b).not.toContain('<svg');
    expect(b).toContain('&lt;svg onload=alert(1)&gt;');
  });

  it('still renders legitimate LaTeX after escaping', () => {
    const out = renderMarkdownAndMath('$$\\frac{a}{b} \\le x^{2} \\text{ok}$$');
    expect(out).toContain('<span class="math-frac">');
    expect(out).toContain('≤');
    expect(out).toContain('<sup>2</sup>');
    expect(out).toContain('<span class="math-text">ok</span>');
  });

  it('escapes HTML in prose, code blocks and inline code', () => {
    const out = renderMarkdownAndMath('hi <script>alert(1)</script> `<b>` \n```js\n<i>x</i>\n```');
    expect(out).not.toContain('<script>');
    expect(out).not.toContain('<b>');
    expect(out).not.toContain('<i>');
    expect(out).toContain('&lt;script&gt;');
  });

  it('emits no inline event handlers (CSP-compatible copy button)', () => {
    const out = renderMarkdownAndMath('```ts\nconst a = 1;\n```');
    expect(out).toContain('class="md-copy-btn"');
    expect(out).not.toMatch(/\son[a-z]+=/i);
    expect(out).toContain('data-code="const%20a%20%3D%201%3B"');
  });
});

describe('renderMarkdownAndMath — link scheme allowlist', () => {
  it('renders http/https/mailto links as anchors', () => {
    expect(renderMarkdownAndMath('[a](https://example.com/x?y=1&z=2)')).toContain(
      '<a href="https://example.com/x?y=1&amp;z=2" target="_blank" rel="noopener noreferrer" class="md-link">a</a>',
    );
    expect(renderMarkdownAndMath('[m](mailto:me@example.com)')).toContain('href="mailto:me@example.com"');
    expect(renderMarkdownAndMath('[r](/admin/docs)')).toContain('href="/admin/docs"');
  });

  it('refuses javascript:, data: and vbscript: links', () => {
    for (const bad of [
      '[x](javascript:alert%281%29)',
      '[x](javascript:alert(1))',
      '[x](JavaScript:alert(1))',
      '[x]( javascript:alert(1))',
      '[x](java\nscript:alert(1))',
      '[x](data:text/html,<script>alert(1)</script>)',
      '[x](vbscript:msgbox)',
    ]) {
      const out = renderMarkdownAndMath(bad);
      expect(out, bad).not.toContain('<a ');
      expect(out, bad).not.toMatch(/href=/);
    }
  });

  it('isSafeLinkHref validates protocols', () => {
    expect(isSafeLinkHref('https://a.b')).toBe(true);
    expect(isSafeLinkHref('http://a.b')).toBe(true);
    expect(isSafeLinkHref('mailto:a@b.c')).toBe(true);
    expect(isSafeLinkHref('relative/path')).toBe(true);
    expect(isSafeLinkHref('javascript:alert(1)')).toBe(false);
    expect(isSafeLinkHref('data:text/plain,hi')).toBe(false);
    expect(isSafeLinkHref('file:///etc/passwd')).toBe(false);
  });
});
