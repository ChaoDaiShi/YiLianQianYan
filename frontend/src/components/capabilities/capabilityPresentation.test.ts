import { describe, expect, it } from "vitest";
import {
  formatAgentSource,
  formatCapabilityKind,
  formatCapabilityProvider,
  formatCapabilityRisk,
  formatCapabilityStatus,
  formatPermission,
  formatPluginTransport,
} from "./capabilityPresentation";

describe("capability presentation", () => {
  it("maps known capability values without inventing unknown values", () => {
    expect(formatCapabilityKind("mcp_tool")).toBe("MCP 工具");
    expect(formatCapabilityProvider("skill_runtime")).toBe("技能运行时");
    expect(formatCapabilityStatus("ready")).toEqual({ label: "就绪", tone: "success" });
    expect(formatCapabilityRisk("high")).toEqual({ label: "高风险", tone: "danger" });
    expect(formatCapabilityKind("future_kind")).toBe("future_kind");
  });

  it("formats existing permission and source identifiers", () => {
    expect(formatPermission("mcp.invoke")).toBe("调用 MCP 工具");
    expect(formatAgentSource("local_file")).toBe("本地文件");
    expect(formatPluginTransport("streamable_http")).toBe("Streamable HTTP");
    expect(formatPluginTransport("custom_transport")).toBe("custom_transport");
  });
});
