import { describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { renderToStaticMarkup } from "react-dom/server";
import CapabilityNavigation, { CAPABILITY_CATEGORIES, CapabilityCategoryContent } from "./CapabilityNavigation";
import navigationSource from "./CapabilityNavigation.tsx?raw";

describe("capability workbench navigation", () => {
  it("renders category content inline with persistent tool list and detail panes", () => {
    const html = renderToStaticMarkup(<MemoryRouter><CapabilityCategoryContent category="tool" /></MemoryRouter>);
    expect(html).toContain('aria-label="运行时工具"');
    expect(html).toContain('aria-label="工具列表"');
    expect(html).toContain('aria-label="工具详情"');
    expect(html).not.toContain('role="dialog"');
  });
  it("keeps all six ordered categories in fixed navigation without an outer modal", () => {
    expect(CAPABILITY_CATEGORIES.map(item => item.id)).toEqual(["skill", "workflow", "mcp", "tool", "plugin", "import"]);
    const html = renderToStaticMarkup(<MemoryRouter><CapabilityNavigation /></MemoryRouter>);
    expect(html).toContain('aria-label="能力分类"');
    expect(html).toContain('aria-label="能力工作台"');
    for(const item of CAPABILITY_CATEGORIES) expect(html).toContain(item.label);
    expect(html).not.toContain('role="dialog"');
    expect(html).not.toContain("capability-navigation-card");
  });
  it("honors existing category and entity query parameters", () => {
    const html = renderToStaticMarkup(<MemoryRouter initialEntries={["/capabilities?category=tool&tool=read_file"]}><CapabilityNavigation /></MemoryRouter>);
    expect(html).toContain('aria-label="运行时工具"');
    expect(html).not.toContain('aria-label="技能详情"');
    for(const key of ["skill","server","workflow","tool"]) expect(navigationSource).toContain(`params.get("${key}")`);
    expect(navigationSource).toContain('initialTool={params.get("tool") ?? undefined}');
  });
});
