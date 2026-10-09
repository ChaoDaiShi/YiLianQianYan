import { expect, it } from "vitest";
import type { CloudIdentity, CloudSyncStatus } from "../../api/cloud";
import {
  buildSearchChatPrompt,
  buildSearchKnowledgeRecord,
  buildSearchTaskDraft,
  identityPresentation,
  syncPresentation,
  wizardCanFinish,
} from "./cloudPresentation";

const guest: CloudIdentity = {
  local_identity_id: "local-1",
  device_id: "device-1",
  account_id: null,
  email: null,
  status: "guest",
  local_core_available: true,
  cloud_enabled: false,
};

it("presents guest mode as complete local use rather than an error", () => {
  expect(identityPresentation(guest)).toEqual({ label: "仅本地", tone: "default", description: "核心功能可用，登录后可启用跨设备同步。" });
});

it("keeps sync backlog and errors visible", () => {
  const sync: CloudSyncStatus = {
    pending_count: 3,
    conflict_count: 1,
    error_count: 2,
    cursor: "9",
    last_success_at: 1,
    last_error_code: "SERVICE_UNAVAILABLE",
  };
  expect(syncPresentation(sync)).toEqual({ label: "需要处理", tone: "warning", detail: "3 项待同步 · 1 项冲突 · 2 项失败" });
});

it("requires model readiness but keeps identity search and voice optional", () => {
  expect(wizardCanFinish({ identity: false, model: true, search: false, stt: false, tts: false })).toBe(true);
  expect(wizardCanFinish({ identity: true, model: false, search: true, stt: true, tts: true })).toBe(false);
});

it("routes a search result into existing chat task and knowledge shapes", () => {
  const result = { title: "Rust docs", url: "https://example.test/rust", snippet: "Cancellation safety" };
  expect(buildSearchChatPrompt(result)).toContain("请总结并核实以下搜索结果");
  expect(buildSearchTaskDraft(result)).toEqual({
    title: "研究：Rust docs",
    description: "Cancellation safety\n\n来源：https://example.test/rust",
  });
  expect(buildSearchKnowledgeRecord(result)).toMatchObject({ category: "knowledge", source: "ai_search" });
});
