// Re-export of lib/api.ts (auth headers, latency tracking, error handling,
// debug-log helpers) kept for the callers that still import "../state/api.js".

export { api } from "../lib/api.js";
export type { ApiOptions as ApiCallOptions } from "../lib/api.js";
