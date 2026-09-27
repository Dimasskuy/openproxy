// Minimal i18n client. Loads a single language pack from the backend at boot and exposes
// `t(key, params?)` with {{param}} interpolation.
//
// Pluralization: keys ending in `_one` / `_other` are picked from a `count` param (English rule:
// count === 1 → _one, else _other). Languages with more plural forms (Arabic, Russian) would need
// the plural rule function extended — see the `make-plural` library if you need it later.

export type Lang = 'en'; // | 'es' | 'fr' | ... — add when we ship more

interface Strings {
  [key: string]: string;
}

let currentLang: Lang = 'en';
let strings: Strings = {};
let loadPromise: Promise<void> | null = null;

/** Fetch a language pack from the backend and set it as the active language. */
export async function loadLang(lang: Lang): Promise<void> {
  if (loadPromise) return loadPromise; // dedupe concurrent calls
  loadPromise = (async () => {
    try {
      const res = await fetch(`/admin/i18n/${lang}.json`, {
        // TRIPLE-FIX (Bug 3): was `cache: 'force-cache'`, which made the browser serve a stale
        // en.json indefinitely — even after a server upgrade added new keys (e.g. analytics.*). The
        // stale pack had none of them, so `t()` fell back to returning the raw key string (e.g.
        // "analytics.chart.daily_usage" appeared verbatim in the UI). `no-cache` revalidates with the
        // server on every fetch (sends `If-Modified-Since` / `If-None-Match` and accepts 304, so the
        // payload is only re-downloaded when it actually changes) — the right strategy for a
        // versioned, server-served JSON asset whose lifetime is decoupled from the JS bundle hash.
        cache: 'no-cache',
      });
      if (!res.ok) {
        console.error(`i18n: failed to load ${lang}:`, res.status);
        if (lang !== 'en') {
          // Fall back to English silently. The branch is unreachable while
          // Lang = 'en'; the F3 worklog entry records a latent self-reference
          // bug to fix before adding another language.
          return loadLang('en');
        }
        return;
      }
      strings = await res.json();
      currentLang = lang;
    } catch (e) {
      console.error('i18n: network error loading', lang, e);
    } finally {
      loadPromise = null;
    }
  })();
  return loadPromise;
}

/** Synchronous translate. Returns the key itself if not found. */
export function t(key: string, params?: Record<string, string | number>): string {
  // Pluralization: when `count` is in params, try the _one/_other variant FIRST, so keys like
  // "notifications.unread_count" can exist ONLY as plural variants. The previous logic looked up
  // the base key first and returned the raw key when it didn't exist, never reaching the check.
  let template: string | undefined;
  if (params && typeof params['count'] === 'number') {
    const pluralKey = params['count'] === 1 ? `${key}_one` : `${key}_other`;
    template = strings[pluralKey];
  }
  // Fall back to the base key when no plural variant matched (or there is no `count` param).
  if (template == null) {
    template = strings[key];
  }
  if (template == null) {
    // Fall back to the key itself, which stays visible in the UI so a missing
    // string is obvious during development.
    return key;
  }
  // {{param}} interpolation
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      template = template.replace(new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, 'g'), String(v));
    }
  }
  return template;
}

/** Get the current language code. */
export function getLang(): Lang {
  return currentLang;
}

/** Check if a string is loaded (for conditional UI). */
export function isLoaded(): boolean {
  return Object.keys(strings).length > 0;
}
