import { describe, expect, it } from "vitest";
import { acceptNotice, emptyNoticeState } from "./assistantNoticePolicy";

describe("personal assistant notice policy", () => {
  it("deduplicates replay across serialized state and quiets repeated completions", () => {
    const state = emptyNoticeState();
    expect(acceptNotice(state, "a", "1", "completed", "c", "/more", 0)).toBe(true);
    const restored = JSON.parse(JSON.stringify(state));
    expect(acceptNotice(restored, "a", "1", "completed", "c", "/more", 1)).toBe(false);
    expect(acceptNotice(restored, "a", "2", "completed", "c", "/more", 1000)).toBe(false);
    expect(acceptNotice(restored, "a", "3", "completed", "c", "/more", 300000)).toBe(true);
  });
  it("preserves attention notices during cooldown and suppresses current chat duplicates", () => {
    const state = emptyNoticeState();
    acceptNotice(state, "a", "1", "completed", "c", "/more", 0);
    expect(acceptNotice(state, "a", "2", "waiting_approval", "c", "/more", 1)).toBe(true);
    expect(acceptNotice(state, "a", "2", "waiting_input", "c", "/more", 2)).toBe(true);
    expect(acceptNotice(state, "a", "3", "waiting_input", "c", "/chat/c", 3)).toBe(false);
    expect(acceptNotice(state, "b", "4", "completed", "d", "/more", 4)).toBe(true);
  });
  it("bounds replay history", () => {
    const state = emptyNoticeState();
    for (let i = 0; i < 1000; i++) acceptNotice(state, `a${i}`, `${i}`, "completed", "c", "/more", i);
    expect(state.seen.length).toBe(256);
    expect(Object.keys(state.completed).length).toBe(256);
  });
});
