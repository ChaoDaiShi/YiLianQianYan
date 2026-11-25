import { afterEach, describe, expect, it, vi } from "vitest";
import { startPersonalAssistant } from "./automations";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({}) }));
afterEach(() => vi.unstubAllGlobals());
const input = { expected_revision: 2, request_id: "5c68d2d1-bafe-4a33-9b01-111111111111" };

describe("personal start acceptance evidence", () => {
  it("clears a pending identity only for a matching explicit server rejection", async () => {
    for (const sample of [
      { status: 400, id: input.request_id, rejected: true, definitive: true },
      { status: 400, id: "different-request", rejected: true, definitive: false },
      { status: 400, id: input.request_id, rejected: false, definitive: false },
      { status: 500, id: input.request_id, rejected: true, definitive: false },
    ]) {
      vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "cannot start", request_id: sample.id, definitive_rejected: sample.rejected }), { status: sample.status })));
      await expect(startPersonalAssistant("a", input)).rejects.toMatchObject({ definitiveRejected: sample.definitive });
    }
  });
  it("keeps uncertain transport and malformed responses recoverable", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("connection reset after upload")));
    const transport = await startPersonalAssistant("a", input).catch(error => error);
    expect(transport.definitiveRejected).not.toBe(true);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));
    const malformed = await startPersonalAssistant("a", input).catch(error => error);
    expect(malformed.definitiveRejected).not.toBe(true);
    expect(malformed.message).toContain("核对");
  });
});
