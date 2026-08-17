import { describe, expect, it } from "vitest";
import { NAV_GROUPS } from "./navGroups";

describe("NavRail groups", () => {
  it("keeps every current navigation route in one visual group", () => {
    const items = NAV_GROUPS.flatMap((group) => group.items);

    expect(items.map((item) => item.to)).toEqual([
      "/chat",
      "/workflows",
      "/workspaces",
      "/skills",
      "/plugins",
      "/agents",
      "/capabilities",
      "/knowledge",
      "/system",
      "/logs",
      "/settings",
    ]);

    expect(NAV_GROUPS.map((group) => group.label)).toEqual([
      "核心",
      "能力",
      "系统",
    ]);
  });
});
