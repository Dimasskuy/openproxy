// handlers/config-handlers.ts — config view handlers. The "Save" click lives in
// views/config.ts (a single PUT).
import { showToast } from "../components/toast.js";
// is a placeholder for future config actions (toggle, import). Per spec §3 + §13.8 nothing is
// attached to `window.*`; `exportConfig` is registered in handlers/registry.ts so the
// data-action shim can dispatch to it.


export function exportConfig(): void {
  showToast("Config export is not implemented yet.", "info");
}
