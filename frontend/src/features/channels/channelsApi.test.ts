import { afterEach, describe, expect, it, vi } from "vitest";
import { requestResult } from "../../core/api/http";
import {
  cancelChannelLogin,
  checkChannel,
  createChannelPreset,
  deleteChannel,
  deleteChannelPreset,
  dismissChannelMessage,
  getChannelLogin,
  listChannelPresets,
  previewChannelPreset,
  regenerateChannelMessage,
  startChannelLogin,
  setChannelReplyMode,
  updateChannelPreset,
  verifyChannelLogin,
} from "../../api/channels";

vi.mock("../../core/api/http", () => ({ requestResult: vi.fn() }));

afterEach(() => vi.resetAllMocks());

describe("channel management API contracts", () => {
  it("keeps preset CRUD and local preview on the frozen endpoints", async () => {
    vi.mocked(requestResult).mockResolvedValue({ ok: true, data: {} });
    const preset = { name: "温柔日常", agent_id: null, model_profile_id: null, instructions: "自然回应" };

    await listChannelPresets();
    await createChannelPreset(preset);
    await updateChannelPreset("preset / one", preset);
    await deleteChannelPreset("preset / one");
    await previewChannelPreset("preset / one", "你好");

    expect(vi.mocked(requestResult).mock.calls).toEqual([
      ["GET", "/api/channels/presets", undefined],
      ["POST", "/api/channels/presets", preset],
      ["PUT", "/api/channels/presets/preset%20%2F%20one", preset],
      ["DELETE", "/api/channels/presets/preset%20%2F%20one", undefined],
      ["POST", "/api/channels/presets/preset%20%2F%20one/preview", { message: "你好" }],
    ]);
  });

  it("uses the channel login, connection check, delete, regenerate and dismiss routes", async () => {
    vi.mocked(requestResult).mockResolvedValue({ ok: true, data: {} });

    await startChannelLogin("channel / one");
    await getChannelLogin("channel / one");
    await verifyChannelLogin("channel / one", "3742");
    await cancelChannelLogin("channel / one");
    await checkChannel("channel / one");
    await deleteChannel("channel / one");
    await regenerateChannelMessage("channel / one", "message / one");
    await dismissChannelMessage("channel / one", "message / one");

    expect(vi.mocked(requestResult).mock.calls).toEqual([
      ["POST", "/api/channels/channel%20%2F%20one/login", undefined],
      ["GET", "/api/channels/channel%20%2F%20one/login", undefined],
      ["POST", "/api/channels/channel%20%2F%20one/login/verify", { code: "3742" }],
      ["DELETE", "/api/channels/channel%20%2F%20one/login", undefined],
      ["POST", "/api/channels/channel%20%2F%20one/check", undefined],
      ["DELETE", "/api/channels/channel%20%2F%20one", undefined],
      ["POST", "/api/channels/channel%20%2F%20one/messages/message%20%2F%20one/regenerate", undefined],
      ["POST", "/api/channels/channel%20%2F%20one/messages/message%20%2F%20one/dismiss", undefined],
    ]);
  });

  it("saves the reply policy through its explicit channel endpoint", async () => {
    vi.mocked(requestResult).mockResolvedValue({ ok: true, data: {} });
    await setChannelReplyMode("channel / one", "automatic");
    expect(requestResult).toHaveBeenCalledWith("POST", "/api/channels/channel%20%2F%20one/reply-mode", { reply_mode: "automatic" });
  });
});
