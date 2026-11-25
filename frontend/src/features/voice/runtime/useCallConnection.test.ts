import { afterEach, describe, expect, it, vi } from "vitest";
import { getProviderReadiness, verifyProviderConnection } from "../../../api/providerConnection";
import type { GlobalVoiceSession } from "../../../api/voice";
import { useCallConnection } from "./useCallConnection";

vi.mock("react", () => ({
  useRef: (value: unknown) => ({ current: value }),
  useCallback: (callback: unknown) => callback,
}));
vi.mock("../../../api/providerConnection", () => ({
  getProviderReadiness: vi.fn(),
  verifyProviderConnection: vi.fn(),
}));
vi.mock("../floatingSurface", () => ({
  desktopFloatingAvailable: () => false,
  openFloatingSurface: vi.fn(),
}));

afterEach(() => {
  vi.resetAllMocks();
  vi.unstubAllGlobals();
});

describe("call setup", () => {
  it("opens a configured call without submitting test speech to providers", async () => {
    const service = { provider: "fixture", model: "fixture", configured: true, available: true };
    vi.mocked(getProviderReadiness).mockResolvedValue({ model: service, stt: service, tts: service });
    vi.mocked(verifyProviderConnection).mockResolvedValue({ ok: false, error: "TIMEOUT" });
    vi.stubGlobal("AudioContext", class {
      resume = vi.fn().mockResolvedValue(undefined);
      close = vi.fn().mockResolvedValue(undefined);
    });
    const startSession = vi.fn().mockResolvedValue({ state: "listening" } as GlobalVoiceSession);
    const onSessionReady = vi.fn();
    const onFailure = vi.fn();
    const connect = useCallConnection({
      session: null,
      epoch: { current: 0 },
      startSession,
      onStart: vi.fn(),
      onOutputReady: vi.fn(),
      onServicesReady: vi.fn(),
      onSessionReady,
      onFailure,
    });

    connect();

    await vi.waitFor(() => expect(onSessionReady).toHaveBeenCalledOnce());
    expect(startSession).toHaveBeenCalledOnce();
    expect(verifyProviderConnection).not.toHaveBeenCalled();
    expect(onFailure).not.toHaveBeenCalled();
  });
});
