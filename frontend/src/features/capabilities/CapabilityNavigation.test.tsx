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
  it("keeps original categories and the shared catalogue in fixed navigation without an outer modal", () => {
    expect(CAPABILITY_CATEGORIES.map(item => item.id)).toEqual(["skill", "workflow", "mcp", "tool", "harness", "plugin", "import", "catalog"]);
    const html = renderToStaticMarkup(<MemoryRouter><CapabilityNavigation /></MemoryRouter>);
    expect(html).toContain('aria-label="能力分类"');
    expect(html).toContain('aria-label="能力工作台"');
    expect(html).toContain('aria-label="能力组合"');
    expect(html).toContain('aria-label="能力组件"');
    expect(html).toContain('aria-label="来源与总览"');
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

  it("keeps package management out of the capability catalog", () => {
    const catalog = renderToStaticMarkup(<MemoryRouter initialEntries={["/capabilities?category=catalog"]}><CapabilityNavigation /></MemoryRouter>);
    expect(catalog).toContain('aria-label="搜索能力"');
    expect(catalog).not.toContain('aria-label="托管能力导入"');
    expect(catalog).not.toContain("capability-category-heading");

    const imports = renderToStaticMarkup(<MemoryRouter initialEntries={["/capabilities?category=import&import_tab=packages"]}><CapabilityNavigation /></MemoryRouter>);
    expect(imports).toContain('aria-label="托管能力导入"');
  });
});
