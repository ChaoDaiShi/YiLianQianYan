import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import type { HarnessProfile, HarnessSnapshot } from "../../api/harnesses";
import HarnessProfileDetails from "./HarnessProfileDetails";

describe("Harness component readiness", () => {
  it("keeps selected state separate from agent, Skill and workflow availability", () => {
    const profile: HarnessProfile = {
      id: "example", name: "资料处理", level: "user", tool_group_ids: ["files"], skill_ids: ["outline"],
      workflow_ids: ["export"], plugin_ids: [], agent_id: "author",
    };
    const snapshot: HarnessSnapshot = {
      settings: { revision: 1, active_id: profile.id, groups: [], profiles: [profile] },
      system_groups: [{ id: "files", name: "文件", level: "system", tools: ["read_file", "load_skill"] }],
      catalog: {
        tools: [{ id: "read_file", name: "读取文件" }, { id: "load_skill", name: "加载技能" }],
        skills: [{ id: "outline", name: "内容提纲", plugin_id: null }],
        workflows: [{ id: "export", name: "导出", description: "", ready: true, plugin_id: null, tool_ids: ["write_file"] }],
        plugins: [],
        agents: [{ id: "author", name: "编辑", plugin_id: null, allowed_tools: ["read_file"] }],
      },
    };
    const view = renderToStaticMarkup(<MemoryRouter><HarnessProfileDetails profile={profile} snapshot={snapshot} onClose={() => {}} /></MemoryRouter>);
    expect(view).toContain("当前组合");
    expect(view).toContain("对话无法加载以下技能");
    expect(view).toContain("工具缺失或超出组合范围");
    expect(view).not.toContain("工作流定义与工具范围已就绪");
  });
});
