import { afterEach, describe, expect, it, vi } from "vitest";
import { getConversationContextManifest, getWorkflowContextManifest } from "./contextManifest";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "controlled-test-session" }) }));
afterEach(() => vi.unstubAllGlobals());
describe("execution context manifest clients", () => {
  it("loads the actual workflow-run snapshot without using a node preparation snapshot", async () => { const payload = { owner: { kind: "workflow", workflow_graph_id: "graph", run_id: "run/same" }, manifest: { schema_version: 1, entries: [], total_chars: 0 } }; const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(payload))); vi.stubGlobal("fetch", fetch); expect(await getWorkflowContextManifest("run/same")).toEqual(payload); expect(fetch.mock.calls[0][0]).toContain("/api/workflow-runs/run%2Fsame/context-manifest"); });
  it("keeps legacy snapshot absence explicit rather than fabricating an empty manifest", async () => { vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(new Response("context_manifest_not_available", { status: 404 })))); await expect(getWorkflowContextManifest("old-run")).rejects.toThrow("context_manifest_not_available"); await expect(getConversationContextManifest("old-chat")).rejects.toThrow("context_manifest_not_available"); });
});
