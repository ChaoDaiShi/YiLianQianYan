import { afterEach, describe, expect, it, vi } from "vitest";

import {
  MCP_RUNTIME_STATUS_LABELS,
  MCP_RUNTIME_STATUS_TONES,
  mcpTransportRuntimeLabel,
  EXTERNAL_PROMPT_WARNING,
  RESOURCE_TEXT_PREVIEW_MAX,
  resourcePreview,
  promptMessageText,
  type McpResourceContent,
  type McpPromptMessage,
} from "./mcpRuntime";
import pluginsPageSource from "../pages/PluginsPage.tsx?raw";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("MCP runtime product labels", () => {
  it("treats stdio and Streamable HTTP as supported transports", () => {
    expect(mcpTransportRuntimeLabel("stdio")).toBe("Stdio");
    expect(mcpTransportRuntimeLabel("streamable_http")).toBe("Streamable HTTP");
    expect(mcpTransportRuntimeLabel("http")).toBe("Streamable HTTP");
    expect(mcpTransportRuntimeLabel("sse")).toBe("SSE (legacy)");
  });

  it("exposes runtime status labels and tones", () => {
    expect(MCP_RUNTIME_STATUS_LABELS.ready).toBe("就绪");
    expect(MCP_RUNTIME_STATUS_LABELS.unavailable).toBe("不可用");
    expect(MCP_RUNTIME_STATUS_TONES.ready).toBe("success");
    expect(MCP_RUNTIME_STATUS_TONES.unavailable).toBe("danger");
    expect(MCP_RUNTIME_STATUS_TONES.disabled).toBe("default");
  });
});

describe("resource preview safety", () => {
  it("shows text content bounded for display", () => {
    const content: McpResourceContent = {
      kind: "text",
      uri: "file:///note",
      mime_type: "text/plain",
      text: "hello resource",
    };
    expect(resourcePreview(content)).toEqual({
      kind: "text",
      text: "hello resource",
      truncated: false,
    });
  });

  it("truncates oversized text", () => {
    const long = "a".repeat(RESOURCE_TEXT_PREVIEW_MAX + 100);
    const preview = resourcePreview({
      kind: "text",
      uri: "file:///big",
      mime_type: null,
      text: long,
    });
    expect(preview.kind).toBe("text");
    if (preview.kind === "text") {
      expect(preview.truncated).toBe(true);
      expect(preview.text.length).toBeLessThanOrEqual(RESOURCE_TEXT_PREVIEW_MAX + 1);
    }
  });

  it("never renders blob base64", () => {
    const content: McpResourceContent = {
      kind: "blob",
      uri: "file:///image",
      mime_type: "image/png",
      blob_base64: "aGVsbG8=",
    };
    const preview = resourcePreview(content);
    expect(preview.kind).toBe("blob");
    const serialized = JSON.stringify(preview);
    expect(serialized).not.toContain("aGVsbG8=");
    expect(serialized).not.toContain("blob_base64");
    if (preview.kind === "blob") {
      expect(preview.mimeType).toBe("image/png");
      expect(preview.size).toBe(8);
    }
  });
});

describe("prompt preview safety", () => {
  it("renders the external-content warning", () => {
    expect(EXTERNAL_PROMPT_WARNING).toBe("来自外部 MCP Server 的内容，仅供预览。");
    expect(pluginsPageSource).toContain("EXTERNAL_PROMPT_WARNING");
  });

  it("renders text and keeps image/audio unexpanded", () => {
    const textMessage: McpPromptMessage = {
      role: "user",
      content: { type: "text", text: "hi there" },
    };
    expect(promptMessageText(textMessage)).toBe("hi there");

    const imageMessage: McpPromptMessage = {
      role: "assistant",
      content: { type: "image", data: "AAAA", mime_type: "image/png" },
    };
    expect(promptMessageText(imageMessage)).toBe("[图片] 未展开");
  });
});

describe("PluginsPage runtime surface", () => {
  it("shows Streamable HTTP as a supported runtime and drops stdio-only wording", () => {
    expect(pluginsPageSource).toContain("Streamable HTTP");
    expect(pluginsPageSource).not.toContain("Not supported by current runtime");
    expect(pluginsPageSource).not.toContain("仅支持 stdio");
    expect(pluginsPageSource).not.toContain("不会进入 Agent Runtime registry");
  });

  it("shows per-server runtime counts", () => {
    expect(pluginsPageSource).toContain("tools_count");
    expect(pluginsPageSource).toContain("resources_count");
    expect(pluginsPageSource).toContain("prompts_count");
  });

  it("offers no tool execution button", () => {
    expect(pluginsPageSource).not.toContain("执行工具");
    expect(pluginsPageSource).not.toContain("运行工具");
    expect(pluginsPageSource).not.toContain("调用工具");
    expect(pluginsPageSource).not.toContain("Execute");
    expect(pluginsPageSource).not.toContain("Call Tool");
    // The page states explicitly that no execution entry exists here.
    expect(pluginsPageSource).toContain("不提供执行按钮");
  });

  it("never auto-triggers chat, tasks, or system prompt from a prompt preview", () => {
    expect(pluginsPageSource).not.toContain("sendMessage");
    expect(pluginsPageSource).not.toContain("createTask");
    expect(pluginsPageSource).not.toContain("createConversation");
    expect(pluginsPageSource).not.toContain("createAgent");
  });
});
