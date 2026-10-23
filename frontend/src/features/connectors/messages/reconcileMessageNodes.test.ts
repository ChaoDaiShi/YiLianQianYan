import { describe, expect, it } from "vitest";
import { applyNodeChanges, type Node } from "@xyflow/react";
import { reconcileMessageNodes } from "./reconcileMessageNodes";

type MessageNode = Node<{ content: string }, "conversationSpeech">;
const node = (id: string, content: string): MessageNode => ({ id, type: "conversationSpeech", position: { x: 48, y: 40 }, data: { content } });

describe("controlled conversation node reconciliation", () => {
  it("keeps React Flow measurements across refreshed content and new branch nodes", () => {
    const measured = applyNodeChanges<MessageNode>([
      { id: "source", type: "dimensions", dimensions: { width: 288, height: 128 }, setAttributes: true },
    ], [node("source", "原回答")]);
    const refreshed = reconcileMessageNodes(measured, [node("source", "已持久化回答"), node("branch", "新分支问题")]);
    expect(refreshed[0].data.content).toBe("已持久化回答");
    expect(refreshed[0].measured).toEqual({ width: 288, height: 128 });
    expect(refreshed[0].width).toBe(288);
    expect(refreshed[0].height).toBe(128);
    expect(refreshed[1].measured).toBeUndefined();

    const branchMeasured = applyNodeChanges<MessageNode>([
      { id: "branch", type: "dimensions", dimensions: { width: 288, height: 128 } },
    ], refreshed);
    const completed = reconcileMessageNodes(branchMeasured, [node("source", "已持久化回答"), node("branch", "新分支已完成")]);
    expect(completed.every((item) => item.measured?.width === 288 && item.measured.height === 128)).toBe(true);
  });

  it("updates the authoritative projection while retaining controlled drag state", () => {
    const current: MessageNode[] = [{ ...node("kept", "旧内容"), dragging: true, measured: { width: 288, height: 128 } }, node("removed", "旧路径")];
    const projection = [{ ...node("kept", "新内容"), position: { x: 372, y: 200 }, selected: true }];
    const next = reconcileMessageNodes(current, projection);
    expect(next.map((item) => item.id)).toEqual(["kept"]);
    expect(next[0].position).toEqual({ x: 372, y: 200 });
    expect(next[0].selected).toBe(true);
    expect(next[0].dragging).toBe(true);
    expect(next[0].measured).toEqual({ width: 288, height: 128 });
  });
});
