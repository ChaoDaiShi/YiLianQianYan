import { describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { renderToStaticMarkup } from "react-dom/server";
import CapabilityNavigation, { CAPABILITY_CATEGORIES } from "./CapabilityNavigation";
import { CapabilityManagerDialog } from "./CapabilityNavigation";
describe("capability navigation", () => {
  it("opens the selected category without duplicate category tabs", () => {
    const html = renderToStaticMarkup(<MemoryRouter><CapabilityManagerDialog category="tool" onClose={() => {}} /></MemoryRouter>);
    expect(html).toContain("工具管理");
    expect(html).not.toContain('role="tablist"');
    expect(html).toContain('role="dialog"');
  });
  it("starts with six ordered navigation entries and opens no manager automatically", () => {
    expect(CAPABILITY_CATEGORIES.map(item => item.id)).toEqual(["skill", "workflow", "mcp", "tool", "plugin", "import"]);
    const html = renderToStaticMarkup(<MemoryRouter><CapabilityNavigation /></MemoryRouter>);
    expect(html.match(/class="capability-navigation-card"/g)).toHaveLength(6);
    expect(html).not.toContain('role="dialog"');
    expect(html).not.toContain("托管 Skill 与声明式 Plugin");
  });
});
