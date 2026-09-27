import { describe, it, expect, beforeEach } from "vitest";
import { state } from "../../state/index.js";
import { getAccountDisplay } from "./modal.js";
import type { Account } from "../../lib/types/accounts.js";

describe("getAccountDisplay in log detail modal", () => {
  beforeEach(() => {
    state.accounts = [
      {
        id: 205 as any,
        provider_id: "minimax" as any,
        label: "Production Minimax",
        priority: 1,
        health_status: "healthy",
        extra_config_json: null,
        rate_limited_until: null,
        quota_session_used: null,
        quota_session_limit: null,
        quota_session_reset_at: null,
        quota_weekly_used: null,
        quota_weekly_limit: null,
        quota_weekly_reset_at: null,
        quota_plan_name: null,
        quota_last_fetched_at: null,
        quota_fetch_error: null,
        auth_type: "api_key",
        email: null,
        oauth_scope: null,
        expires_at: null,
        created_at: "2026-01-01",
      } as Account,
      {
        id: 300 as any,
        provider_id: "openai" as any,
        label: null,
        priority: 1,
        health_status: "healthy",
        extra_config_json: null,
        rate_limited_until: null,
        quota_session_used: null,
        quota_session_limit: null,
        quota_session_reset_at: null,
        quota_weekly_used: null,
        quota_weekly_limit: null,
        quota_weekly_reset_at: null,
        quota_plan_name: null,
        quota_last_fetched_at: null,
        quota_fetch_error: null,
        auth_type: "api_key",
        email: null,
        oauth_scope: null,
        expires_at: null,
        created_at: "2026-01-01",
      } as Account,
    ];
  });

  it("displays direct account_label when provided by detail endpoint", () => {
    const res = getAccountDisplay(205, "Direct Label From DB");
    expect(res.text).toBe("Direct Label From DB");
    expect(res.title).toBe("Account #205");
  });

  it("displays label from state.accounts matching accountId when direct label is omitted", () => {
    const res = getAccountDisplay(205, null);
    expect(res.text).toBe("Production Minimax");
    expect(res.title).toBe("Account #205");
  });

  it("displays account ID when account exists in state but has no label", () => {
    const res = getAccountDisplay(300, null);
    expect(res.text).toBe("300");
    expect(res.title).toBeUndefined();
  });

  it("displays account ID when account does not exist in state and has no label", () => {
    const res = getAccountDisplay(404, null);
    expect(res.text).toBe("404");
    expect(res.title).toBeUndefined();
  });

  it("displays dash when accountId is null or empty", () => {
    expect(getAccountDisplay(null, null).text).toBe("—");
    expect(getAccountDisplay("", null).text).toBe("—");
    expect(getAccountDisplay("—", null).text).toBe("—");
  });
});
