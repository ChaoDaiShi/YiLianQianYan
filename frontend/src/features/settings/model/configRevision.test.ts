import { afterEach, expect, it, vi } from "vitest";
import { updateSettings } from "../../../api/system";
import { defaultConfig } from "./config";
import { comparableConfig, rebaseConfigDraft } from "./mapping";

vi.mock("../../../api/controlSession", () => ({ controlSessionHeaders: () => ({}) }));
afterEach(() => vi.unstubAllGlobals());

it("sends the read revision and preserves an explicit conflict response", async () => {
  const fetch = vi.fn(async (_url: string, _init?: RequestInit) => Response.json({ error: "配置已更新", code: "revision_conflict", revision: 9 }, { status: 409 }));
  vi.stubGlobal("fetch", fetch);
  const result = await updateSettings({ ...defaultConfig, revision: 8 });
  const body = JSON.parse(String(fetch.mock.calls[0][1]?.body));
  expect(body.expected_revision).toBe(8);
  expect(body.revision).toBeUndefined();
  expect(result).toMatchObject({ code: "revision_conflict", revision: 9, error: "配置已更新" });
});

it("preserves the field of a rejected setting and the saved directory actions", async () => {
  const pending_application = [
    { field: "skills.directories", action: "refresh_skill_discovery" },
    { field: "subagents.directories", action: "restart_application" },
  ];
  const fetch = vi.fn()
    .mockResolvedValueOnce(Response.json({ error: "上下文窗口须大于 0", code: "invalid_settings", field: "compaction.context_window" }, { status: 400 }))
    .mockResolvedValueOnce(Response.json({ status: "saved", revision: 9, application: "next_read", pending_application }));
  vi.stubGlobal("fetch", fetch);
  expect(await updateSettings(defaultConfig)).toMatchObject({ code: "invalid_settings", field: "compaction.context_window" });
  expect(await updateSettings(defaultConfig)).toMatchObject({ application: "next_read", pending_application });
});

it("rebases only edited fields and retains the latest unrelated settings", () => {
  const base = { ...defaultConfig, revision: 8 };
  const draft = { ...base, voice: { ...base.voice, stt: { ...base.voice.stt, model: "my-stt" } } };
  const latest = { ...base, revision: 9, compaction: { ...base.compaction, context_window: 300000 }, voice: { ...base.voice, tts: { ...base.voice.tts, voice: "new-voice" } } };
  const next = rebaseConfigDraft(base, draft, latest);
  expect(next.revision).toBe(9);
  expect(next.voice.stt.model).toBe("my-stt");
  expect(next.voice.tts.voice).toBe("new-voice");
  expect(next.compaction.context_window).toBe(300000);
  expect(comparableConfig(base)).toBe(comparableConfig({ ...base, revision: 10 }));
});
