import { afterEach, describe, expect, it, vi } from "vitest";
import { loadWorkspaceServices, refreshWorkspaceServiceDraft, saveWorkspaceService, serviceChoices, type WorkspaceServiceSetting } from "./workspaceServices";

vi.mock("../../../api/controlSession", () => ({ controlSessionHeaders: () => ({}) }));

const profile = { source: "profile", id: "vision-default" } as const;
const override = { source: "pool", id: "vision-pool" } as const;

function setting(service_kind: "chat" | "vision" | "image", revision: number, value = service_kind === "vision" ? override : null): WorkspaceServiceSetting {
  return {
    workspace_id: "workspace:a", service_kind, revision, override: value,
    effective: value ? { ...value, label: "项目识图", scope: "workspace", revision } : null,
    inherited: { ...profile, label: "全局识图", scope: "global", revision: 3 }, application: "next_call",
  };
}

afterEach(() => vi.unstubAllGlobals());

describe("workspace service overrides", () => {
  it("selects chat profiles and chat pools without mixing basic-service profiles", () => {
    expect(serviceChoices("chat", {
      chats: [{ id: "same", label: "对话档案", provider: "local" }],
      profiles: [{ id: "same", label: "识图档案", provider_account_id: "account", service_kind: "vision" }],
      accounts: [],
      pools: [{ id: "chat-pool", label: "对话模型池", service_kind: "chat", enabled: true }],
    })).toEqual([
      { value: "profile:same", label: "对话档案 · local" },
      { value: "pool:chat-pool", label: "模型池 · 对话模型池" },
    ]);
  });

  it("maps only profiles and enabled pools for the requested purpose", () => {
    expect(serviceChoices("vision", {
      chats: [],
      profiles: [
        { id: "shared", label: "识图档案", provider_account_id: "account", service_kind: "vision" },
        { id: "image", label: "生图档案", provider_account_id: "account", service_kind: "image" },
      ],
      accounts: [{ id: "account", label: "厂商账户" }],
      pools: [
        { id: "shared", label: "项目池", service_kind: "vision", enabled: true },
        { id: "disabled", label: "停用池", service_kind: "vision", enabled: false },
      ],
    })).toEqual([
      { value: "profile:shared", label: "厂商账户 · 识图档案" },
      { value: "pool:shared", label: "模型池 · 项目池" },
    ]);
  });

  it("restores inheritance by writing null then reads all purposes at the new revision", async () => {
    const saved = { ...setting("vision", 8, null), effective: { ...profile, label: "全局识图", scope: "global", revision: 3 } };
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return Response.json(saved);
      return Response.json(url.endsWith("/vision") ? saved : setting(url.endsWith("/chat") ? "chat" : "image", 8));
    });
    vi.stubGlobal("fetch", fetch);

    expect(await saveWorkspaceService("workspace:a", "vision", 7, null)).toMatchObject({ ok: true, data: saved });
    expect(fetch.mock.calls[0][0]).toContain("/api/workspaces/workspace%3Aa/service-settings/vision");
    expect(JSON.parse(String(fetch.mock.calls[0][1]?.body))).toEqual({ expected_revision: 7, override: null });
    expect(await loadWorkspaceServices("workspace:a")).toMatchObject({ ok: true, data: { chat: { revision: 8 }, vision: { revision: 8 }, image: { revision: 8 } } });
  });

  it("keeps an unsaved reset after a conflict and rebases it on the latest revision", async () => {
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return Response.json({ error: "revision_conflict" }, { status: 409 });
      return Response.json(setting(url.endsWith("/vision") ? "vision" : url.endsWith("/chat") ? "chat" : "image", 8));
    });
    vi.stubGlobal("fetch", fetch);
    const draft = { vision: null };

    expect(await saveWorkspaceService("workspace:a", "vision", 7, draft.vision)).toMatchObject({ ok: false, status: 409 });
    expect(fetch).toHaveBeenCalledTimes(1);
    const latest = await loadWorkspaceServices("workspace:a");
    if (!latest.ok) throw new Error(latest.error);

    expect(refreshWorkspaceServiceDraft(draft, latest.data)).toEqual({ vision: null });
    expect(latest.data.vision.revision).toBe(8);
    expect(refreshWorkspaceServiceDraft(draft, { ...latest.data, vision: setting("vision", 9, null) })).toEqual({});
  });
});
