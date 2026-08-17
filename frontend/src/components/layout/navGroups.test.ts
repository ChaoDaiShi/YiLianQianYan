import { describe, expect, it } from "vitest";
import { NAV_GROUPS } from "./navGroups";
import navRailSource from "./NavRail.tsx?raw";

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

  it("uses a readable wide grouped layout at the default desktop breakpoint", () => {
    expect(navRailSource).toContain("min-[1180px]:w-[220px]");
    expect(navRailSource).toContain("min-[1180px]:flex-row");
    expect(navRailSource).toContain("min-[1180px]:block");
    expect(navRailSource).toContain("✦");
  });
});
