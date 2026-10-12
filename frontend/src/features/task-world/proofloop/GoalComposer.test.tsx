import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import GoalComposer from "./GoalComposer";

describe("GoalComposer", () => {
  it("renders goal composer and capability list", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <GoalComposer
          workspaceId="ws_1"
          initialGoal="调研两种桌面本地持久化方案"
          availableCapabilities={[
            { id: "cap_1", name: "资料检索工具", description: "从论文或文档检索" },
            { id: "cap_2", name: "摘要生成工具" },
          ]}
          onDraftCreated={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(html).toContain("从目标生成可验证任务");
    expect(html).toContain("调研两种桌面本地持久化方案");
    expect(html).toContain("资料检索工具");
    expect(html).toContain("从论文或文档检索");
    expect(html).toContain("生成任务计划");
  });

  it("renders model unavailable warning banner when model is disabled", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <GoalComposer
          workspaceId="ws_1"
          modelUnavailable={true}
          onDraftCreated={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(html).toContain("尚未配置可用的大模型服务");
    expect(html).toContain("前往设置");
  });

  it("renders cancel button when onCancel prop is provided", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <GoalComposer
          workspaceId="ws_1"
          onDraftCreated={vi.fn()}
          onCancel={vi.fn()}
        />
      </MemoryRouter>,
    );

    expect(html).toContain("返回");
  });
});
