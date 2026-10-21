import { describe, expect, it, vi } from "vitest";
import { chooseConversationCanvas, connectConversationToCanvas } from "./conversationCanvasTarget";

describe("conversation canvas targeting", () => {
  it("prefers the actual message graph over a different last selected canvas with the same agent", () => {
    expect(chooseConversationCanvas([
      { canvas: { id: "last-selected" }, hasBinding: true, hasGraph: false },
      { canvas: { id: "existing-graph" }, hasBinding: true, hasGraph: true },
    ], "last-selected")).toBe("existing-graph");
  });
  it("keeps the current canvas among equally valid bindings without choosing an unrelated canvas", () => {
    const matches = [
      { canvas: { id: "a" }, hasBinding: true, hasGraph: false },
      { canvas: { id: "b" }, hasBinding: true, hasGraph: false },
      { canvas: { id: "unrelated" }, hasBinding: false, hasGraph: false },
    ];
    expect(chooseConversationCanvas(matches, "b")).toBe("b");
    expect(chooseConversationCanvas(matches, "unrelated")).toBe("a");
    expect(chooseConversationCanvas([matches[2]], "unrelated")).toBeNull();
  });
  it("binds the explicitly chosen real agent before creating the message graph", async () => {
    const order: string[] = [];
    const bind = vi.fn(async () => { order.push("bind"); });
    const createGraph = vi.fn(async () => { order.push("graph"); });
    await connectConversationToCanvas("target", "chosen-agent", "homepage-chat", { bind, createGraph });
    expect(bind).toHaveBeenCalledWith("target", { kind: "conversation_agent", source_id: "conversation:homepage-chat", target_id: "agent:chosen-agent" });
    expect(createGraph).toHaveBeenCalledWith("target", "homepage-chat");
    expect(order).toEqual(["bind", "graph"]);
  });
  it("does not manufacture an opened graph when the real binding fails", async () => {
    const bind = vi.fn(async () => { throw new Error("already bound elsewhere"); });
    const createGraph = vi.fn(async () => undefined);
    await expect(connectConversationToCanvas("target", "chosen-agent", "chat", { bind, createGraph })).rejects.toThrow("already bound elsewhere");
    expect(createGraph).not.toHaveBeenCalled();
  });
});
