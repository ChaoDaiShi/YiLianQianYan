import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));
import { getPresence, startVoiceSession } from "./voice";
import { getDesktopContextProjection, listTaskProjections } from "./projections";

describe("voice and projection shared API", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses protected provider-neutral voice routes", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ activity: "idle" }) })
      .mockResolvedValueOnce({ ok: true, json: async () => ({ id: "voice-1" }) });
    vi.stubGlobal("fetch", fetchMock);

    await getPresence();
    await startVoiceSession();

    expect(fetchMock.mock.calls[0][0]).toContain("/api/presence");
    expect(fetchMock.mock.calls[1][0]).toContain("/api/voice/sessions/start");
    expect(fetchMock.mock.calls[1][1]?.headers).toMatchObject({
      "X-Yilian-Control-Session": "a".repeat(64),
    });
  });

  it("keeps cross-domain data behind projections", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ tasks: [] }) })
      .mockResolvedValueOnce({ ok: true, json: async () => ({ simulated: true }) });
    vi.stubGlobal("fetch", fetchMock);

    await listTaskProjections("workspace:test");
    const desktop = await getDesktopContextProjection("workspace:test");

    expect(fetchMock.mock.calls[0][0]).toContain("/api/projections/tasks");
    expect(fetchMock.mock.calls[1][0]).toContain("/api/projections/desktop-context");
    expect(desktop?.simulated).toBe(true);
  });
});
