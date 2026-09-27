# Design — openproxy Dashboard

A locked design system for openproxy admin dashboard, combining the original authentic color palette of openproxy with the tactile micro-crafted physics, sleek scrollbars, and modern-minimal Workbench specification from Hallmark and `listo.click`.

Every page redesign and component reads this system. Consistency across views is strictly enforced.

---

## 1. Genre & Archetype
- **Genre:** `modern-minimal` (high-density dev-tool infrastructure instrument, tactile engineering precision).
- **Macrostructure Family:** `Workbench` (dense workspace, sticky/collapsible side-rail navigation, high SNR telemetry, crisp borders, cards with tactile elevation, 8-state interactive controls, responsive card tables, zero AI-slop).

---

## 2. Palette (Original System Palette Preserved)

### Dark Theme (`:root[data-theme="dark"]`, default)
Warm dark ash canvas with authentic high-contrast surfaces and lifted signal accents:
- `--color-bg`: `#151413` (warm dark ash canvas)
- `--color-surface`: `#1c1b1a` (warm ash surface)
- `--color-surface-2`: `#262523` (elevated panels, table headers, drawers)
- `--color-surface-hover`: `rgba(255, 255, 255, 0.04)`
- `--color-surface-soft`: `var(--color-surface-2)`
- `--color-text`: `#f2efe9` (warm soft white)
- `--color-text-emphasis`: `#ffffff`
- `--color-text-muted`: `#a39e95` (warm ash muted)
- `--color-text-inverse`: `#151413`
- `--color-link`: `#7aa2f7`
- `--color-link-hover`: `#a4bbf9`
- `--color-border`: `#3c3935` (warm ash border)
- `--color-border-soft`: `#2b2926` (warm ash border soft)
- `--color-frame-ink`: `#3c3935`
- `--color-primary`: `#ff4d57` (Dell red lifted ~20% for dark contrast)
- `--color-primary-hover`: `#ff6b73`
- `--color-primary-fg`: `#151413`
- `--color-primary-soft`: `#2a1717`

### Light Theme (`:root[data-theme="light"]`)
Crisp canvas with warm grey elevations and classic signal accents:
- `--color-bg`: `#f5f7fa`
- `--color-surface`: `#ffffff` (canvas)
- `--color-surface-2`: `#f0f3f7`
- `--color-surface-hover`: `rgba(0, 0, 0, 0.04)`
- `--color-surface-soft`: `var(--color-surface-2)`
- `--color-text`: `#17212b`
- `--color-text-emphasis`: `#0d151c`
- `--color-text-muted`: `#667085`
- `--color-text-inverse`: `#ffffff`
- `--color-link`: `#0000ee`
- `--color-link-hover`: `#1d4ed8`
- `--color-border`: `#cfd7e3`
- `--color-border-soft`: `#e5eaf1`
- `--color-frame-ink`: `#c7d0dc`
- `--color-primary`: `#e91d2a` (classic Dell red)
- `--color-primary-hover`: `#c91825`
- `--color-primary-fg`: `#ffffff`
- `--color-primary-soft`: `#f6dada`

### Semantics (Light / Dark)
- **Success:** `#4d7c2a` (light) / `#88b870` (dark, sage desaturated)
- **Warning:** `#a86a00` (light) / `#d4a070` (dark, peach desaturated)
- **Error:** `#b21f1f` (light) / `#e08080` (dark, salmon desaturated)
- **Info:** `#2b5a78` (light) / `#8ab0c5` (dark, sky desaturated)

---

## 3. Typography
- **Display & Headings:** Ubuntu 700 / system sans, letter-spacing `-0.02em`. Strict roman (no italic headers).
- **Body & Controls:** Ubuntu 400 / system sans, letter-spacing `-0.01em`.
- **Code & Telemetry:** Courier New / ui-monospace / monospace. High readability for request IDs, status codes, model IDs, token counts, and latencies.

---

## 4. Tactile Microinteractions & 8-State Component Discipline
Every interactive control (`button`, `a.btn`, `input`, `select`, `textarea`, `.switch`, `.chip`, `.nav-item`) follows tactile physics:
1. **Default:** Hairline border `var(--color-border)`, surface background, subtle shadow.
2. **Hover:** Surface brightness lift to `var(--color-surface-soft)`, border transition to `var(--color-border-strong)`.
3. **:focus-visible:** Crisp non-blurry 2px outline `var(--color-primary)` with 2px offset or inset ring.
4. **:active (Tactile Spring):** Physical depression with elastic scale: `transform: scale(0.97)` via `cubic-bezier(0.32, 0.72, 0, 1)`.
5. **Disabled:** Opacity 0.45, cursor `not-allowed`, zero hover transformations.
6. **Loading:** Spinner or pulse indicator with preserved dimension and disabled pointer events.
7. **Error:** Border tinted with `var(--color-error)` and soft background `var(--color-error-soft)`.
8. **Success:** Temporary affirmative state with `var(--color-success)` border/badge.

---

## 5. Sleek Integrated Scrollbars
Custom dual-theme minimalist scrollbars matching `listo.click`:
```css
html, body, * {
  scrollbar-width: thin;
  scrollbar-color: var(--color-scrollbar-thumb) transparent;
}
::-webkit-scrollbar { width: 6px; height: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background-color: var(--color-scrollbar-thumb);
  border-radius: 9999px;
  transition: background-color 0.2s ease;
}
::-webkit-scrollbar-thumb:hover { background-color: var(--color-scrollbar-thumb-hover); }
```

---

## 6. Elevaciones y Sombras
- **Subtle (Cards & Containers):** `var(--shadow-subtle)` (`var(--shadow-sm)`).
- **Elevated (Dropdowns & Popovers):** `var(--shadow-elevated)` (`var(--shadow-md)`).
- **Modal (Dialogs & Drawers):** `var(--shadow-xl)`.

---

## 7. Invariantes de Ingeniería (AGENTS.md)
- **Límite < 800 LOC:** Mantener todos los archivos CSS por debajo del umbral duro de 800 líneas.
- **Paridad 1:1:** Preservar selectores de vistas, componentes, badges dinámicos, tooltips `abbr[title]`, layouts móviles `@media (max-width: 768px)` y clases móviles `.mobile-*-cell`.
- **Aislamiento de Cascada:** Reglas globales restringidas a `tokens.css`, `base.css` y `components/`. Las reglas en `views/*.css` deben acotarse estrictamente a `#main .view-specific`.
