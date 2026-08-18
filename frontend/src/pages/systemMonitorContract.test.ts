import { describe, expect, it } from "vitest";
import source from "./SystemPage.tsx?raw";

describe("Monitor contract", () => {
  it("uses the real system and health APIs without fake online states", () => {
    expect(source).toContain("getSystemInfo");
    expect(source).toContain("healthCheck");
    expect(source).toContain("SystemStatusBadge");
    expect(source).not.toContain("小昔涟在线");
    expect(source).not.toContain("Agent 在线");
    expect(source).not.toContain("MCP 在线");
  });

  it("keeps refresh and honest error recovery", () => {
    expect(source).toContain("刷新");
    expect(source).toContain("重试");
    expect(source).toContain("状态暂时无法获取");
  });
});
