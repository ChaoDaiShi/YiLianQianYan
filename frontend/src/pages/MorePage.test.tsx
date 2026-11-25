import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { expect, it, vi } from "vitest";
import MorePage from "./MorePage";

vi.mock("../components/system/useProductPreferences", () => ({
  useProductPreferences: () => ({
    preferences: { navigation: [
      { id: "account", visible: true },
      { id: "memory", visible: true },
      { id: "knowledge", visible: false },
    ] },
  }),
}));

vi.mock("../features/plugins/PluginFeaturePage", () => ({
  usePluginFeatures: () => ({ features: [], loading: false, error: "", reload: vi.fn() }),
}));

it("keeps enabled destinations and one plugin management link in the compact directory", () => {
  const html = renderToStaticMarkup(<MemoryRouter><MorePage /></MemoryRouter>);
  expect(html).toContain('href="/browser"');
  expect(html).toContain('href="/software"');
  expect(html).toContain("软件挂载");
  expect(html).not.toContain('href="/channels"');
  expect(html).toContain('href="/memory"');
  expect(html).not.toContain("账号");
  expect(html).not.toContain('href="/knowledge"');
  expect(html).toContain("定时任务");
  expect(html).toContain('href="/more/manual"');
  expect(html.match(/管理插件/g)).toHaveLength(1);
  expect(html).not.toContain("暂无提供功能页面的插件");
});
