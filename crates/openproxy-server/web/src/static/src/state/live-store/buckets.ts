import type { RecentUsageRow } from "../../lib/types/api.js";
import type { Bucket, SnapshotWindow } from "./types.js";

export function emptyBucket(bucketStartSec = -1): Bucket {
  return {
    bucket_start_sec: bucketStartSec,
    rows: new Map<string, RecentUsageRow>(),
    count: 0,
    tokens_in: 0,
    tokens_out: 0,
    cost_usd: 0,
    status_2xx: 0,
    status_4xx: 0,
    status_5xx: 0,
    latencies: [],
    race_wins: 0,
    race_total: 0,
  };
}

function resetBucketMetrics(b: Bucket): void {
  b.count = 0;
  b.tokens_in = 0;
  b.tokens_out = 0;
  b.cost_usd = 0;
  b.status_2xx = 0;
  b.status_4xx = 0;
  b.status_5xx = 0;
  b.latencies.length = 0;
  b.race_wins = 0;
  b.race_total = 0;
}

export function resetBucketInPlace(b: Bucket): void {
  b.bucket_start_sec = -1;
  b.rows.clear();
  resetBucketMetrics(b);
}

export const WINDOW_1S = 300;   // 5 min
export const WINDOW_5S = 360;   // 30 min
export const WINDOW_1M = 1440;  // 24h
export const MAX_LATENCIES_PER_BUCKET = 1000;
export const MAX_RECENT_ROWS = 1000;
const MAX_TRACKED_ROWS = 10_000;

export const buckets1s: Bucket[] = [];
export const buckets5s: Bucket[] = [];
export const buckets1m: Bucket[] = [];
for (let i = 0; i < WINDOW_1S; i++) buckets1s.push(emptyBucket());
for (let i = 0; i < WINDOW_5S; i++) buckets5s.push(emptyBucket());
for (let i = 0; i < WINDOW_1M; i++) buckets1m.push(emptyBucket());

function bucketIndexFromSec(bucketStartSec: number, bucketSecs: number, totalBuckets: number): number {
  return ((Math.floor(bucketStartSec / bucketSecs) % totalBuckets) + totalBuckets) % totalBuckets;
}

function rowTimestampSec(row: RecentUsageRow): number {
  const timestampMs = Date.parse(row.created_at);
  return Number.isFinite(timestampMs) ? Math.floor(timestampMs / 1000) : Math.floor(Date.now() / 1000);
}

function requestKey(row: RecentUsageRow): string {
  if (row.request_id) return row.request_id;
  return `row:${row.id}:${row.created_at}`;
}

function isSuccessful(row: RecentUsageRow): boolean {
  return row.status_code >= 200 && row.status_code < 400;
}

function shouldReplace(existing: RecentUsageRow, incoming: RecentUsageRow): boolean {
  const existingSuccess = isSuccessful(existing);
  const incomingSuccess = isSuccessful(incoming);
  if (existingSuccess !== incomingSuccess) return incomingSuccess;
  const existingMs = Date.parse(existing.created_at);
  const incomingMs = Date.parse(incoming.created_at);
  if (existingMs !== incomingMs) return incomingMs > existingMs;
  return (incoming.id ?? 0) >= (existing.id ?? 0);
}

function addRowMetrics(b: Bucket, row: RecentUsageRow): void {
  b.count++;
  if (isSuccessful(row)) {
    b.tokens_in += row.prompt_tokens ?? 0;
    b.tokens_out += row.completion_tokens ?? 0;
    b.cost_usd += row.cost_usd ?? 0;
    b.status_2xx++;
  } else if (row.status_code >= 400 && row.status_code < 500) {
    b.status_4xx++;
  } else if (row.status_code >= 500) {
    b.status_5xx++;
  }
  if (b.latencies.length < MAX_LATENCIES_PER_BUCKET) {
    b.latencies.push(row.total_ms || 0);
  }
  const raceSize: number = row.race_total ?? 0;
  if (raceSize > 1) {
    b.race_total++;
    if (!row.race_lost) b.race_wins++;
  }
}

function rebuildBucket(b: Bucket): void {
  resetBucketMetrics(b);
  for (const row of b.rows.values()) addRowMetrics(b, row);
}

function removeTrackedRow(key: string): void {
  for (const collection of [buckets1s, buckets5s, buckets1m]) {
    for (const bucket of collection) {
      if (bucket.rows.delete(key)) rebuildBucket(bucket);
    }
  }
}

function bucketFor(collection: Bucket[], bucketSecs: number, rowSec: number): Bucket {
  const bucketStartSec = Math.floor(rowSec / bucketSecs) * bucketSecs;
  const index = bucketIndexFromSec(bucketStartSec, bucketSecs, collection.length);
  const bucket = collection[index]!;
  if (bucket.bucket_start_sec !== bucketStartSec) {
    resetBucketInPlace(bucket);
    bucket.bucket_start_sec = bucketStartSec;
  }
  return bucket;
}

const trackedRows = new Map<string, RecentUsageRow>();

export function clearBucketsForTest(): void {
  for (const collection of [buckets1s, buckets5s, buckets1m]) {
    for (const bucket of collection) resetBucketInPlace(bucket);
  }
  trackedRows.clear();
}

function pruneTrackedRows(nowSec: number): void {
  if (trackedRows.size <= MAX_TRACKED_ROWS) return;
  const cutoffSec = nowSec - WINDOW_1M * 60;
  for (const [key, row] of trackedRows) {
    if (rowTimestampSec(row) < cutoffSec) trackedRows.delete(key);
  }
}

export function writeRowToBuckets(row: RecentUsageRow): void {
  const key = requestKey(row);
  const existing = trackedRows.get(key);
  if (existing && !shouldReplace(existing, row)) return;
  if (existing) removeTrackedRow(key);
  trackedRows.set(key, row);

  const rowSec = rowTimestampSec(row);
  for (const [collection, bucketSecs] of [[buckets1s, 1], [buckets5s, 5], [buckets1m, 60]] as const) {
    const bucket = bucketFor(collection, bucketSecs, rowSec);
    bucket.rows.set(key, row);
    rebuildBucket(bucket);
  }
  pruneTrackedRows(Math.floor(Date.now() / 1000));
}

export interface WindowBuckets {
  buckets: Bucket[];
  bucketSecs: number;
  count: number;
}

export interface CollectedWindow {
  buckets: Bucket[];
  bucketSecs: number;
  startMs: number;
}

export function getWindowBuckets(windowSecs: SnapshotWindow): WindowBuckets {
  if (windowSecs === 1800) {
    return { buckets: buckets5s, bucketSecs: 5, count: WINDOW_5S };
  }
  const count = windowSecs === 60 ? 60 : WINDOW_1S;
  return { buckets: buckets1s, bucketSecs: 1, count };
}

export function collectWindow(windowSecs: SnapshotWindow): CollectedWindow {
  const { buckets, bucketSecs, count } = getWindowBuckets(windowSecs);
  const totalBuckets = buckets.length;
  const nowSec = Math.floor(Date.now() / 1000);
  const currentBucketStartSec = Math.floor(nowSec / bucketSecs) * bucketSecs;
  const currentIdx = bucketIndexFromSec(currentBucketStartSec, bucketSecs, totalBuckets);
  const out: Bucket[] = [];
  for (let i = count - 1; i >= 0; i--) {
    const expectedStartSec = currentBucketStartSec - i * bucketSecs;
    const idx = (((currentIdx - i) % totalBuckets) + totalBuckets) % totalBuckets;
    const bucket = buckets[idx]!;
    out.push(bucket.bucket_start_sec === expectedStartSec ? bucket : emptyBucket(expectedStartSec));
  }
  const startMs = (currentBucketStartSec - (count - 1) * bucketSecs) * 1000;
  return { buckets: out, bucketSecs, startMs };
}

export function percentileOfSorted(sortedAsc: number[], p: number): number {
  const n = sortedAsc.length;
  if (n === 0) return 0;
  const idx = Math.min(n - 1, Math.max(0, Math.floor(p * n)));
  return sortedAsc[idx] ?? 0;
}

export function windowPercentile(windowBuckets: Bucket[], p: number): number {
  let total = 0;
  for (const b of windowBuckets) total += b.latencies.length;
  if (total === 0) return 0;
  const all: number[] = new Array<number>(total);
  let i = 0;
  for (const b of windowBuckets) {
    for (const lat of b.latencies) {
      all[i] = lat;
      i++;
    }
  }
  all.sort((a, c) => a - c);
  return percentileOfSorted(all, p);
}

export function windowAvgLatency(windowBuckets: Bucket[]): number {
  let sum = 0;
  let n = 0;
  for (const b of windowBuckets) {
    for (const lat of b.latencies) {
      sum += lat;
      n++;
    }
  }
  return n === 0 ? 0 : sum / n;
}
