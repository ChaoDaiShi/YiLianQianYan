import { describe, expect, it } from "vitest";
import { buildHistoryEntries, originLabel } from "./conversationHistoryModel";
import { conversationFamilies } from "../../features/connectors/messages/messageForest";

describe("conversation history sources and branch families", () => {
  it("keeps a channel family together and preserves its most recent activity", () => {
    const rows=[{id:"root",title:"QQ会话",created_at:1,updated_at:2,origin:{kind:"channel" as const,provider:"onebot11",channel_name:"QQ助手"}},{id:"fork",title:"分支",created_at:3,updated_at:9,origin:{kind:"channel" as const,provider:"onebot11"}},{id:"desktop",title:"本机",created_at:2,updated_at:4,origin:{kind:"desktop" as const}}];
    const entries=buildHistoryEntries(rows,[{id:"root",title:"QQ会话",conversationIds:["root","fork"]}]);
    expect(entries.map(x=>x.id)).toEqual(["root","desktop"]);
    expect(entries[0].pathCount).toBe(2);
    expect(entries[0].updated_at).toBe(9);
    expect(originLabel(entries[0].origin)).toBe("QQ");
  });
  it("finds a family by a branch title while leaving desktop and channel origins distinct", () => {
    const entries=buildHistoryEntries([{id:"a",title:"计划",created_at:1,updated_at:1},{id:"b",title:"另一个方案",created_at:2,updated_at:2}],[{id:"a",title:"计划",conversationIds:["a","b"]}],"方案");
    expect(entries).toHaveLength(1);
    expect(entries[0].id).toBe("a");
    expect(originLabel(entries[0].origin)).toBe("桌面");
  });
  it("keeps an empty fork in its authoritative overview family", () => {
    const overview = [
      { id: "root", title: "主线", created_at: 1, updated_at: 1, nodes: [{ id: "answer", parent_id: null }], branch: null },
      { id: "fork", title: "空分支", created_at: 2, updated_at: 2, nodes: [], branch: { parent_node_id: "answer" } },
    ];
    const entries = buildHistoryEntries(overview, conversationFamilies(overview));
    expect(entries.map(entry => entry.id)).toEqual(["root"]);
    expect(entries[0].pathCount).toBe(2);
  });
  it("shows retained paths after the root and its branch links are deleted", () => {
    const remaining = [{ id: "fork", title: "保留的分支", created_at: 2, updated_at: 2, nodes: [{ id: "copy", parent_id: null }], branch: null }];
    const entries = buildHistoryEntries(remaining, conversationFamilies(remaining));
    expect(entries.map(entry => entry.id)).toEqual(["fork"]);
    expect(entries[0].pathCount).toBe(1);
  });
});
