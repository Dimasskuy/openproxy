import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecentUsageRow } from "../../lib/types/api.js";
import {
  clearBucketsForTest,
  collectWindow,
  writeRowToBuckets,
} from "./buckets.js";

const nowMs = 1_700_000_000_000;

function makeRow(overrides: Partial<RecentUsageRow> = {}): RecentUsageRow {
  return {
    id: 1,
    request_id: "request-1",
    trace_id: "trace-1",
    provider_id: "provider-1",
    upstream_model_id: "model-1",
    status_code: 200,
    total_ms: 100,
    prompt_tokens: 10,
    completion_tokens: 20,
    cached_tokens: null,
    cost_usd: 0.01,
    connect_ms: null,
    ttft_ms: null,
    request_body_json: null,
    response_body_json: null,
    request_headers: null,
    response_headers: null,
    error_message: null,
    race_total: null,
    race_attempts: null,
    is_streaming: false,
    stream_complete: true,
    race_lost: false,
    stop_reason: null,
    compression_savings_pct: null,
    compression_techniques: null,
    client_response: true,
    prompt_tokens_estimated: false,
    completion_tokens_estimated: false,
    proxy_url: null,
    proxy_status: null,
    is_proxy_rotated: false,
    created_at: new Date(nowMs).toISOString(),
    ...overrides,
  };
}

afterEach(() => {
  vi.useRealTimers();
  clearBucketsForTest();
});

describe("live metrics buckets", () => {
  it("uses each row's created_at instead of hydration time", () => {
    vi.useFakeTimers({ now: nowMs, toFake: ["Date"] });
    writeRowToBuckets(makeRow({ id: 1, request_id: "old", created_at: new Date(nowMs - 120_000).toISOString() }));
    writeRowToBuckets(makeRow({ id: 2, request_id: "recent", created_at: new Date(nowMs - 10_000).toISOString() }));

    const oneMinute = collectWindow(60).buckets;
    const thirtyMinutes = collectWindow(1800).buckets;
    expect(oneMinute.reduce((sum, bucket) => sum + bucket.count, 0)).toBe(1);
    expect(thirtyMinutes.reduce((sum, bucket) => sum + bucket.count, 0)).toBe(2);
  });

  it("deduplicates retries by request_id and keeps the successful result", () => {
    vi.useFakeTimers({ now: nowMs, toFake: ["Date"] });
    writeRowToBuckets(makeRow({ id: 1, request_id: "retry", status_code: 500, created_at: new Date(nowMs - 5_000).toISOString() }));
    writeRowToBuckets(makeRow({ id: 2, request_id: "retry", status_code: 200, created_at: new Date(nowMs - 4_000).toISOString() }));

    const buckets = collectWindow(60).buckets;
    expect(buckets.reduce((sum, bucket) => sum + bucket.count, 0)).toBe(1);
    expect(buckets.reduce((sum, bucket) => sum + bucket.status_2xx, 0)).toBe(1);
    expect(buckets.reduce((sum, bucket) => sum + bucket.status_5xx, 0)).toBe(0);
  });
});
