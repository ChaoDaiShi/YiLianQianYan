import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import PageShell from "./PageShell";
import ManagementFrame from "../../features/capabilities/ManagementFrame";
import account from "../../pages/AccountCenterPage.tsx?raw";
import logs from "../../pages/LogsPage.tsx?raw";
import system from "../../pages/SystemPage.tsx?raw";
import search from "../../pages/SearchPage.tsx?raw";
import skills from "../../pages/SkillsPage.tsx?raw";
import plugins from "../../pages/PluginsPage.tsx?raw";

describe("management scroll boundaries", () => {
  it("keeps viewport-managed pages out of the standard unbounded content wrapper", () => {
    for (const source of [account, logs, system, search, skills, plugins]) {
      expect(source).toMatch(/<(?:PageShell|ManagementFrame)\b[^>]*variant="workspace"/);
    }
    const html = renderToStaticMarkup(<PageShell variant="workspace"><div className="cloud-layout"><nav>分类</nav><main className="cloud-content">正文</main></div></PageShell>);
    expect(html).toContain('class="flex-1 min-h-0 overflow-hidden flex flex-col"');
    expect(html).not.toContain("max-w-[1600px]");
  });
  it("makes an embedded split manager bounded without replacing its inner panes", () => {
    const html = renderToStaticMarkup(<ManagementFrame embedded variant="workspace"><div className="capability-list-scroll">列表</div><div className="capability-detail-panel">内容</div></ManagementFrame>);
    expect(html).toContain('class="min-h-0 min-w-0 flex-1 flex flex-col overflow-hidden"');
    expect(html).toContain('class="capability-list-scroll"');
    expect(html).toContain('class="capability-detail-panel"');
  });
});
