import { afterEach, describe, expect, it, vi } from "vitest";
import { listWorkIndex, reconcileWorkIndex } from "./workIndex";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "controlled-test-session" }) }));
afterEach(() => vi.unstubAllGlobals());
describe("shared work index client", () => {
  it("preserves a read failure instead of returning an empty index", async () => { vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "storage unavailable" }), { status: 500 }))); await expect(listWorkIndex()).rejects.toThrow("storage unavailable"); });
  it("keeps the scope encoded and retains projection diagnostics", async () => { const data = { items: [], limit: 25, truncated: false, projection_issues: [{ kind: "outcome", id: "one", error: "pending", attempts: 2 }] }; const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(data))); vi.stubGlobal("fetch", fetch); expect(await listWorkIndex("a/b", 25)).toEqual(data); expect(fetch.mock.calls[0][0]).toContain("workspace_id=a%2Fb"); });
  it("exposes reconcile refusal and never turns it into a local success", async () => { vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "cannot reconcile" }), { status: 409 }))); await expect(reconcileWorkIndex()).rejects.toThrow("cannot reconcile"); });
});
