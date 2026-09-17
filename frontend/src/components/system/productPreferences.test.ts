import { describe, expect, it } from "vitest";
import { acceptPreferences, defaultPreferences, safePreferences, TRUSTED_MODULES, validPreferences } from "./productPreferences";

describe("trusted system and layout preferences", () => {
  it("does not replace a saved layout with an older delayed read", () => {
    const saved = { ...defaultPreferences(), revision: 2, setup_completed: true };
    expect(acceptPreferences(saved, defaultPreferences())).toEqual(saved);
    expect(acceptPreferences(saved, { schema_version: 99 })).toEqual(saved);
  });
  it("rejects hidden safety entries, disabled mandatory modules and duplicate routes", () => {
    const hidden = defaultPreferences(); hidden.navigation.find((item) => item.id === "settings")!.visible = false;
    expect(validPreferences(hidden)).toBe(false);
    const disabled = defaultPreferences(); disabled.enabled_modules = [];
    expect(validPreferences(disabled)).toBe(false);
    const duplicate = defaultPreferences(); duplicate.navigation[1] = duplicate.navigation[0];
    expect(validPreferences(duplicate)).toBe(false);
  });
  it("falls back from stale or malformed persisted layout to reachable safe defaults", () => {
    expect(safePreferences({ schema_version: 99 })).toEqual(defaultPreferences());
    expect(safePreferences(null)).toEqual(defaultPreferences());
  });
  it("bounds free placement and accepts honest optional-module selection", () => {
    const value = defaultPreferences(); value.monitor.mode = "free"; value.monitor.cards[0].x = 100000;
    expect(validPreferences(value)).toBe(false);
    const selected = defaultPreferences(); selected.enabled_modules = TRUSTED_MODULES.filter((module) => module.mandatory).map((module) => module.id);
    expect(validPreferences(selected)).toBe(true);
  });
});
