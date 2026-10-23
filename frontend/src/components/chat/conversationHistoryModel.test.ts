import { describe, expect, it } from "vitest";
import { buildHistoryEntries, originLabel } from "./conversationHistoryModel";

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
});
