import { describe, expect, it } from "vitest";
import { NAV_GROUPS } from "./navGroups";
import navRailSource from "./NavRail.tsx?raw";

describe("NavRail groups", () => {
  it("keeps every current navigation route in one visual group", () => {
    const items = NAV_GROUPS.flatMap((group) => group.items);

    expect(items.map((item) => item.to)).toEqual([
      "/tasks",
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
    expect(navRailSource).toContain("min-[960px]:w-[80px]");
    expect(navRailSource).toContain("min-h-14");
    expect(navRailSource).toContain("min-[960px]:w-[68px]");
    expect(navRailSource).toContain("<Tooltip key={item.id} content={item.label}>");
    expect(navRailSource).toContain("aria-label={item.label}");
    expect(navRailSource).toContain("absolute right-1 top-1");
    expect(navRailSource).not.toContain("min-[1180px]:w-[148px]");
    expect(navRailSource).not.toContain("min-[1180px]:flex-row");
    expect(navRailSource).toContain("whitespace-nowrap");
    expect(navRailSource).toContain("✦");
  });
});
