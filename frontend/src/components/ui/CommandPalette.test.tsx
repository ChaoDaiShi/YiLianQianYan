import { describe, expect, it, beforeEach } from "vitest";
import { getRecentCommandIds, saveRecentCommandId } from "./CommandPalette";

describe("CommandPalette localStorage resilience", () => {
  const KEY = "cyrene_recent_commands";
  let storage: Record<string, string> = {};

  beforeEach(() => {
    storage = {};
    const mockLocalStorage = {
      getItem: (k: string) => storage[k] ?? null,
      setItem: (k: string, v: string) => {
        storage[k] = v;
      },
      removeItem: (k: string) => {
        delete storage[k];
      },
      clear: () => {
        storage = {};
      },
    };
    Object.defineProperty(globalThis, "localStorage", {
      value: mockLocalStorage,
      writable: true,
      configurable: true,
    });
  });

  it("handles corrupted non-array json such as '{}' without crashing", () => {
    storage[KEY] = "{}";
    const ids = getRecentCommandIds();
    expect(ids).toEqual([]);
    expect(() => ids.includes("test")).not.toThrow();
  });

  it("handles non-string array entries such as '[1, 2, null, true]'", () => {
    storage[KEY] = JSON.stringify([1, "valid-cmd", null, true, "second-cmd"]);
    const ids = getRecentCommandIds();
    expect(ids).toEqual(["valid-cmd", "second-cmd"]);
  });

  it("handles invalid json gracefully", () => {
    storage[KEY] = "{invalid:json}";
    const ids = getRecentCommandIds();
    expect(ids).toEqual([]);
  });

  it("saves and limits recent command IDs properly", () => {
    saveRecentCommandId("cmd-1");
    saveRecentCommandId("cmd-2");
    saveRecentCommandId("cmd-3");
    saveRecentCommandId("cmd-4");
    saveRecentCommandId("cmd-5");
    saveRecentCommandId("cmd-6");

    const ids = getRecentCommandIds();
    expect(ids.length).toBe(5);
    expect(ids[0]).toBe("cmd-6");
  });

  it("ignores non-string or empty id inputs when saving", () => {
    // @ts-expect-error testing runtime bad input
    saveRecentCommandId(null);
    // @ts-expect-error testing runtime bad input
    saveRecentCommandId(123);
    saveRecentCommandId("");
    expect(getRecentCommandIds()).toEqual([]);
  });
});
