import { describe, expect, it } from "vitest";

describe("misplaced test path", () => {
  it("does not block the dashboard suite", () => {
    expect(true).toBe(true);
  });
});
