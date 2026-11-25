import { requestResult, type ApiResult } from "../../../core/api/http";
import type { LlmModel } from "../../../types";
import { listModelPools, poolRequest, type ModelPool, type ProviderAccount } from "../../llm/modelPools";
import { listBasicAccounts, listBasicServices, type BasicServiceProfile } from "../basicServices";

export const workspaceServiceKinds = ["chat", "vision", "image"] as const;
export type WorkspaceServiceKind = typeof workspaceServiceKinds[number];
export const workspaceServiceLabels = { chat: "对话模型", vision: "图像理解", image: "图像生成" };

export interface ServiceSource {
  source: "profile" | "pool";
  id: string;
}

interface EffectiveServiceSource {
  source: ServiceSource["source"] | "global_config";
  id: string;
  label: string;
  scope: "workspace" | "global";
  revision: number;
}

export interface WorkspaceServiceSetting {
  workspace_id: string;
  service_kind: WorkspaceServiceKind;
  override: ServiceSource | null;
  effective: EffectiveServiceSource | null;
  inherited: EffectiveServiceSource | null;
  revision: number;
  application: "next_call";
}

export interface WorkspaceServiceOverrides {
  workspace_id: string;
  name: string;
  kinds: WorkspaceServiceKind[];
}

export type WorkspaceServiceDraft = Partial<Record<WorkspaceServiceKind, ServiceSource | null>>;
export type WorkspaceServiceSettings = Record<WorkspaceServiceKind, WorkspaceServiceSetting>;

export function listWorkspaceServiceOverrides() {
  return requestResult<{ workspaces: WorkspaceServiceOverrides[] }>("GET", "/api/settings/service-overrides");
}

function serviceSettingPath(workspaceId: string, kind: WorkspaceServiceKind) {
  return `/api/workspaces/${encodeURIComponent(workspaceId)}/service-settings/${kind}`;
}

export async function loadWorkspaceServices(workspaceId: string): Promise<ApiResult<WorkspaceServiceSettings>> {
  const [chat, vision, image] = await Promise.all(workspaceServiceKinds.map(kind =>
    requestResult<WorkspaceServiceSetting>("GET", serviceSettingPath(workspaceId, kind))));
  if (!chat.ok) return chat;
  if (!vision.ok) return vision;
  if (!image.ok) return image;
  return { ok: true, data: { chat: chat.data, vision: vision.data, image: image.data } };
}

export function saveWorkspaceService(workspaceId: string, kind: WorkspaceServiceKind, revision: number, override: ServiceSource | null) {
  return requestResult<WorkspaceServiceSetting>("PUT", serviceSettingPath(workspaceId, kind), { expected_revision: revision, override });
}

export function serviceSourceKey(source: ServiceSource | null) {
  return source ? `${source.source}:${source.id}` : "";
}

export function refreshWorkspaceServiceDraft(draft: WorkspaceServiceDraft, latest: WorkspaceServiceSettings): WorkspaceServiceDraft {
  const next = { ...draft };
  for (const kind of workspaceServiceKinds) {
    const edited = next[kind];
    if (edited !== undefined && serviceSourceKey(edited) === serviceSourceKey(latest[kind].override)) delete next[kind];
  }
  return next;
}

interface ServiceCatalog {
  chats: Pick<LlmModel, "id" | "label" | "provider">[];
  profiles: Pick<BasicServiceProfile, "id" | "label" | "provider_account_id" | "service_kind">[];
  accounts: Pick<ProviderAccount, "id" | "label">[];
  pools: Pick<ModelPool, "id" | "label" | "enabled" | "service_kind">[];
}

export async function loadWorkspaceServiceCatalog(): Promise<ServiceCatalog> {
  const [profiles, accounts, pools, chats] = await Promise.all([
    listBasicServices(), listBasicAccounts(), listModelPools(),
    poolRequest<LlmModel[]>("GET", "/api/llm/models"),
  ]);
  return { profiles, accounts, pools, chats };
}

export function serviceChoices(kind: WorkspaceServiceKind, { profiles, accounts, pools, chats }: ServiceCatalog) {
  const accountLabels = new Map(accounts.map(account => [account.id, account.label]));
  return [
    ...(kind === "chat" ? chats.map(profile => ({
      value: serviceSourceKey({ source: "profile", id: profile.id }),
      label: `${profile.label} · ${profile.provider}`,
    })) : profiles.filter(profile => profile.service_kind === kind).map(profile => ({
      value: serviceSourceKey({ source: "profile", id: profile.id }),
      label: [accountLabels.get(profile.provider_account_id), profile.label].filter(Boolean).join(" · "),
    }))),
    ...pools.filter(pool => pool.service_kind === kind && pool.enabled).map(pool => ({
      value: serviceSourceKey({ source: "pool", id: pool.id }),
      label: `模型池 · ${pool.label}`,
    })),
  ];
}
