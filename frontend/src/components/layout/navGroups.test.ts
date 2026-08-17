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

  it("uses a compact readable grouped layout at the default desktop breakpoint", () => {
    expect(navRailSource).toContain("min-[1180px]:w-[148px]");
    expect(navRailSource).toContain("min-[1180px]:mx-2");
    expect(navRailSource).toContain("min-[1180px]:px-2.5");
    expect(navRailSource).toContain("min-[1180px]:h-8");
    expect(navRailSource).toContain("whitespace-nowrap");
    expect(navRailSource).toContain("absolute right-2");
    expect(navRailSource).toContain("min-[1180px]:flex-row");
    expect(navRailSource).toContain("min-[1180px]:block");
    expect(navRailSource).toContain("✦");
  });
});
