import { describe, expect, it } from "vitest";
import { getDrawerState, getWorkspaceMode } from "./workspaceLayout";

describe("getWorkspaceMode", () => {
  it.each([
    [800, "narrow"],
    [959, "narrow"],
    [960, "compact"],
    [1179, "compact"],
    [1180, "full"],
    [1920, "full"],
  ] as const)("maps %ipx to %s", (width, expected) => {
    expect(getWorkspaceMode(width)).toBe(expected);
  });
});

describe("getDrawerState", () => {
  it("keeps narrow drawers mutually exclusive", () => {
    expect(getDrawerState("narrow", "conversations")).toEqual({
      conversationOpen: true,
      executionOpen: false,
    });
    expect(getDrawerState("narrow", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: true,
    });
  });

  it("opens only the execution drawer in compact mode", () => {
    expect(getDrawerState("compact", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: true,
    });
    expect(getDrawerState("compact", "conversations")).toEqual({
      conversationOpen: false,
      executionOpen: false,
    });
  });

  it("uses no drawers in full mode", () => {
    expect(getDrawerState("full", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: false,
    });
  });
});
