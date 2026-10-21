import { describe, expect, it } from "vitest";
import { NAV_GROUPS } from "./navGroups";
import navRailSource from "./NavRail.tsx?raw";
import { NAV_ENTRIES, orderNavigationEntries } from "./navRegistry";

describe("NavRail groups", () => {
  it("keeps every current navigation route in one visual group", () => {
    const items = NAV_GROUPS.flatMap((group) => group.items);

    expect(items.map((item)=>item.to)).toEqual(["/chat","/connectors","/workspaces","/capabilities","/agents","/memory","/knowledge","/system","/logs","/settings","/account"]);

    expect(NAV_GROUPS.map((group) => group.label)).toEqual([
      "核心",
      "能力",
      "系统",
    ]);
  });

  it("keeps home first and preserves the remaining user navigation order", () => {
    const selected = ["capabilities", "workspaces", "chat", "connectors"].map(id => NAV_ENTRIES.find(entry => entry.id === id)!);
    expect(orderNavigationEntries(selected).map(entry => entry.id)).toEqual(["chat", "capabilities", "workspaces", "connectors"]);
    expect(selected.map(entry => entry.id)).toEqual(["capabilities", "workspaces", "chat", "connectors"]);
    expect(NAV_ENTRIES.find(entry => entry.id === "chat")?.label).toBe("主页");
    expect(NAV_ENTRIES.find(entry => entry.id === "connectors")?.label).toBe("画布");
  });

  it("uses a compact readable grouped layout at the default desktop breakpoint", () => {
    expect(navRailSource).toContain("nav-rail");
    expect(navRailSource).toContain("nav-rail-list");
    expect(navRailSource).toContain('aria-label="全局导航"');
    expect(navRailSource).toContain("后端状态：");
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
