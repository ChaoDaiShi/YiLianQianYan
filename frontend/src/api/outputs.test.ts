import { afterEach, describe, expect, it, vi } from "vitest";
import { listOutputs } from "./outputs";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "controlled-test-session" }) }));
afterEach(() => vi.unstubAllGlobals());
describe("unified output catalogue client", () => {
  it("retains valid results and explicit partial-source failures from the same response", async () => {
    const output = { id: "valid", kind: "artifact", title: "Actual report", execution_status: "completed", verification_status: "unknown" };
    const issue = { kind: "conversation", id: "missing-owner", error: "source_not_available" };
    const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ outputs: [output], issues: [issue], limit: 20, truncated: false })));
    vi.stubGlobal("fetch", fetch);
    const result = await listOutputs("space/one", 20);
    expect(result.outputs).toEqual([output]); expect(result.issues).toEqual([issue]);
    expect(result.outputs[0].verification_status).toBe("unknown");
    expect(fetch.mock.calls[0][0]).toContain("workspace_id=space%2Fone");
    expect(fetch.mock.calls[0][1].headers).toEqual({ "X-Yilian-Control-Session": "controlled-test-session" });
  });
  it("does not turn a complete storage failure into a successful empty catalogue", async () => { vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ message: "storage failed" }), { status: 500 }))); await expect(listOutputs()).rejects.toThrow("storage failed"); });
});
