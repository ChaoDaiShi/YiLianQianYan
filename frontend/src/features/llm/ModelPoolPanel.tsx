import { useEffect, useState, type ReactNode } from "react";
import { ArrowDown, ArrowUp, X } from "lucide-react";
import { Button, Input, Modal, Select } from "../../components/ui";
import { createLlmModel, listLlmModels } from "../../api/client";
import type { LlmModel } from "../../types";
import SettingsDialogLayout from "../settings/sections/SettingsDialogLayout";
import { PROVIDER_PRESETS, providerLabel } from "./llmModelUtils";
import { listModelPools, listProviderAccounts, poolRequest, type ModelPool, type ModelPoolTier, type ProviderAccount } from "./modelPools";

type PoolCandidate = Pick<LlmModel, "id" | "label" | "model" | "provider" | "base_url" | "verified_at"> & { accountLabel?: string };
type ServiceProfile = { id: string; label: string; model: string; provider_account_id: string; service_kind: string; test: { ok: boolean; tested_at: string } | null };

function candidateDestination(model: PoolCandidate): string {
  let origin = "来源未知";
  try {
    const address = new URL(model.base_url);
    if (address.protocol === "http:" || address.protocol === "https:") origin = address.origin;
  } catch { /* Older or removed accounts may lack an endpoint. */ }
  return [providerLabel(model.provider), model.accountLabel, origin].filter(Boolean).join(" · ");
}

export default function ModelPoolPanel({ modelManager, serviceKind = "chat", refreshToken = 0, onChanged }: {
  modelManager?: ReactNode;
  serviceKind?: "chat" | "vision" | "image";
  refreshToken?: number;
  onChanged?: () => void;
}) {
  const chat = serviceKind === "chat";
  const [agents, setAgents] = useState<Array<{ id: string; name: string }>>([]);
  const [pools, setPools] = useState<ModelPool[]>([]);
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [models, setModels] = useState<PoolCandidate[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [active, setActive] = useState("");
  const [defaultPool, setDefaultPool] = useState<string | null>(null);
  const [draft, setDraft] = useState<ModelPool | null>(null);
  const [account, setAccount] = useState<ProviderAccount | null>(null);
  const [manager, setManager] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [subject, setSubject] = useState("");
  const [accountKey, setAccountKey] = useState("");
  const [accountKeyAction, setAccountKeyAction] = useState("keep");
  const [newModel, setNewModel] = useState({ account_id: "", label: "", model: "" });
  const [modelDialog, setModelDialog] = useState(false);

  async function refresh() {
    try {
      const [allPools, accounts, profiles, binding, agentResult] = await Promise.all([
        listModelPools(),
        listProviderAccounts(),
        chat ? listLlmModels() : poolRequest<ServiceProfile[]>("GET", "/api/service-profiles"),
        poolRequest<{ pool_id?: string | null; source?: string; id?: string | null }>("GET",
          chat ? "/api/model-pool-bindings/default" : "/api/service-defaults/" + serviceKind),
        chat ? poolRequest<{ agents: Array<{ id: string; name: string }> }>("GET", "/api/agents") : Promise.resolve({ agents: [] }),
      ]);
      if (!profiles) throw new Error("无法读取候选模型。");
      const candidates: PoolCandidate[] = profiles
        .filter((profile) => !("service_kind" in profile) || profile.service_kind === serviceKind)
        .map((profile) => {
          if (!("service_kind" in profile)) {
            const account = accounts.find((item) => item.profile_ids.includes(profile.id));
            return { ...profile, accountLabel: account?.label };
          }
          const account = accounts.find((item) => item.id === profile.provider_account_id);
          return {
            id: profile.id, label: profile.label, model: profile.model,
            provider: account?.provider_id ?? "", base_url: account?.base_url ?? "", accountLabel: account?.label,
            verified_at: profile.test?.ok ? Date.parse(profile.test.tested_at) : null,
          };
        });
      const visible = allPools.filter((pool) => pool.service_kind === serviceKind);
      setAgents(agentResult.agents);
      setPools(visible);
      setAccounts(accounts);
      setModels(candidates);
      setDefaultPool(chat ? binding.pool_id ?? null : binding.source === "pool" ? binding.id ?? null : null);
      setActive((current) => visible.some((pool) => pool.id === current) ? current : visible[0]?.id ?? "");
      setLoaded(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  useEffect(() => { void refresh(); }, [serviceKind, refreshToken]);
  const selected = pools.find((pool) => pool.id === active);

  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setError("");
    try {
      await action();
      await refresh();
      onChanged?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  function newPool() {
    setError("");
    setDraft({
      id: "", label: "新模型池", service_kind: serviceKind, enabled: true, revision: 0,
      attempt_timeout_ms: 30000, total_timeout_ms: 120000,
      tiers: [{ id: crypto.randomUUID(), label: "一级", profile_ids: [] }],
    });
  }

  function moveTier(index: number, offset: number) {
    if (!draft) return;
    const tiers = [...draft.tiers];
    const target = index + offset;
    if (target < 0 || target >= tiers.length) return;
    [tiers[index], tiers[target]] = [tiers[target], tiers[index]];
    setDraft({ ...draft, tiers });
  }

  function changeTier(index: number, patch: Partial<ModelPoolTier>) {
    setDraft((current) => current && { ...current, tiers: current.tiers.map((tier, i) => i === index ? { ...tier, ...patch } : tier) });
  }

  function openAccount(item: ProviderAccount) {
    setError("");
    setAccountKey("");
    setAccountKeyAction("keep");
    setAccount(item);
  }

  const candidates = draft ? models.filter((model) => !draft.tiers.some((tier) => tier.profile_ids.includes(model.id))) : [];
  const candidateName = (id: string) => models.find((model) => model.id === id)?.label ?? "模型档案已删除";
  const describeCandidate = (id: string) => {
    const model = models.find((candidate) => candidate.id === id);
    return model ? `${model.label} · ${model.model} · ${candidateDestination(model)}` : candidateName(id);
  };
  const hasNestedEditor = Boolean(draft || account || modelDialog || manager);

  return <section className="space-y-3">
    {!hasNestedEditor && error && <p className="settings-save-error" role="alert">{error}</p>}
    {!loaded && !error && <p className="settings-help" role="status">正在读取模型池…</p>}
    {loaded && pools.length === 0 && <p className="settings-help">还没有模型池。</p>}
    {pools.length > 0 && <Select label="当前模型池" value={active} onChange={(event) => setActive(event.target.value)} options={pools.map((pool) => ({
      value: pool.id, label: pool.label + (defaultPool === pool.id ? "（默认）" : ""),
    }))} />}
    <div className="flex flex-wrap gap-2">
      <Button size="sm" disabled={busy || !loaded} onClick={newPool}>新建模型池</Button>
      {selected && <>
        <Button size="sm" variant="secondary" disabled={busy} onClick={() => { setError(""); setDraft(structuredClone(selected)); }}>编辑层级</Button>
        {defaultPool !== selected.id && <Button size="sm" variant="secondary" disabled={busy} onClick={() => void run(() => poolRequest(
          "PUT", chat ? "/api/model-pool-bindings/default" : "/api/service-defaults/" + serviceKind,
          chat ? { pool_id: selected.id } : { source: "pool", id: selected.id },
        ))}>设为默认</Button>}
      </>}
    </div>

    {selected && <div className="space-y-2">
      {!selected.enabled && <p className="settings-help">模型池已停用</p>}
      <ol className="divide-y divide-[var(--border-soft)] rounded-lg border border-[var(--border-soft)]">
        {selected.tiers.map((tier, index) => (
          <li key={tier.id} className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-3 py-2">
            <span className="text-sm font-medium">{index + 1}. {tier.label}</span>
            <span className="min-w-0 break-words text-sm text-[var(--text-secondary)]">{tier.profile_ids.map(candidateName).join(" · ")}</span>
          </li>
        ))}
      </ol>
    </div>}

    {(chat || selected || defaultPool) && <details className="u2-disclosure">
      <summary>高级选项</summary>
      <div className="space-y-3 pt-2">
        {selected && <div className="space-y-2">
          <p className="settings-help">候选时限 {selected.attempt_timeout_ms / 1000}s · 总时限 {selected.total_timeout_ms / 1000}s · 版本 {selected.revision}</p>
          <ul className="space-y-1 text-xs text-[var(--text-secondary)]">
            {selected.tiers.flatMap((tier) => tier.profile_ids).map((id) => {
              const model = models.find((item) => item.id === id);
              return <li key={id} className="break-words">{model ? `${describeCandidate(id)} · ${model.verified_at ? "连接已验证" : "尚未验证"}` : candidateName(id)}</li>;
            })}
          </ul>
        </div>}
        {chat && <>
          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" size="sm" disabled={busy} onClick={() => openAccount({ id: "", provider_id: "", label: "", profile_ids: [], base_url: "", credential_id: null })}>添加厂商账户</Button>
            <Button variant="secondary" size="sm" disabled={busy || !accounts.length} onClick={() => {
              setError("");
              setNewModel({ account_id: accounts[0]?.id ?? "", label: "", model: "" });
              setModelDialog(true);
            }}>添加模型</Button>
            {modelManager && <Button variant="secondary" size="sm" onClick={() => setManager(true)}>管理模型</Button>}
          </div>
          {accounts.length > 0 && <ul className="divide-y divide-[var(--border-soft)]">
            {accounts.map((item) => (
              <li key={item.id}><button type="button" className="w-full break-words rounded px-2 py-2 text-left text-sm hover:bg-[var(--surface-hover)]"
                onClick={() => openAccount({ ...item, profile_ids: [...item.profile_ids] })}>{item.label} · {item.profile_ids.length} 个模型档案</button></li>
            ))}
          </ul>}
          {selected && <div className="flex flex-wrap items-end gap-2">
            <div className="min-w-40 flex-1"><Select label="选择绑定智能体" value={subject} onChange={(event) => setSubject(event.target.value)}
              options={[{ value: "", label: "选择智能体" }, ...agents.map((agent) => ({ value: agent.id, label: agent.name }))]} /></div>
            <Button size="sm" variant="secondary" disabled={busy || !subject.trim()} onClick={() => void run(() => poolRequest(
              "PUT", "/api/model-pool-bindings/" + encodeURIComponent(subject.trim()), { pool_id: selected.id },
            ))}>绑定智能体</Button>
          </div>}
        </>}
        <div className="flex flex-wrap gap-2">
          {defaultPool && <Button variant="secondary" size="sm" disabled={busy} onClick={() => void run(() => poolRequest(
            "PUT", "/api/model-pool-bindings/" + (chat ? "default" : "service:" + serviceKind), { pool_id: null },
          ))}>解除默认模型池，使用固定模型</Button>}
          {selected && <Button variant="danger" size="sm" disabled={busy} onClick={() => void run(() => poolRequest(
            "DELETE", "/api/model-pools/" + encodeURIComponent(selected.id) + "?expected_revision=" + selected.revision,
          ))}>删除池</Button>}
        </div>
      </div>
    </details>}

    {modelDialog && <SettingsDialogLayout title="添加账户模型" onClose={() => { setModelDialog(false); setError(""); }} error={error} notice=""
      action={<Button disabled={busy || !newModel.account_id || !newModel.label.trim() || !newModel.model.trim()} onClick={() => void run(async () => {
        const selectedAccount = accounts.find((item) => item.id === newModel.account_id);
        if (!selectedAccount) throw new Error("请选择厂商账户");
        const saved = await createLlmModel({
          provider_account_id: selectedAccount.id, provider: selectedAccount.provider_id, label: newModel.label,
          model: newModel.model, base_url: selectedAccount.base_url, api_format: "openai", api_key: "", api_key_env: "",
          temperature: 0, max_tokens: 16384, invoke_timeout_ms: 120000,
        });
        if (!saved) throw new Error("模型保存失败");
        setModelDialog(false);
      })}>{busy ? "正在保存…" : "保存模型"}</Button>}>
      <div className="space-y-3">
        <Select label="模型所属账户" value={newModel.account_id} onChange={(event) => setNewModel({ ...newModel, account_id: event.target.value })}
          options={[{ value: "", label: "选择厂商账户" }, ...accounts.map((item) => ({ value: item.id, label: item.label }))]} />
        <Input label="模型显示名称" value={newModel.label} onChange={(event) => setNewModel({ ...newModel, label: event.target.value })} />
        <Input label="模型 ID" value={newModel.model} onChange={(event) => setNewModel({ ...newModel, model: event.target.value })} />
      </div>
    </SettingsDialogLayout>}

    <Modal open={manager} onClose={() => { setManager(false); void refresh(); }} title="厂商模型与凭据" size="xl">{modelManager}</Modal>

    {account && <SettingsDialogLayout title={account.id ? "编辑厂商账户" : "添加厂商账户"} onClose={() => { setAccount(null); setError(""); }} error={error} notice=""
      action={<>
        {account.id && <Button variant="danger" disabled={busy} onClick={() => void run(async () => {
          await poolRequest("DELETE", "/api/provider-accounts/" + encodeURIComponent(account.id));
          setAccount(null);
        })}>删除账户</Button>}
        <Button disabled={busy} onClick={() => void run(async () => {
          await poolRequest(account.id ? "PUT" : "POST", "/api/provider-accounts" + (account.id ? "/" + encodeURIComponent(account.id) : ""), {
            ...account, credential_action: accountKeyAction, credential_fields: accountKey ? { api_key: accountKey } : {},
          });
          setAccount(null);
        })}>{busy ? "正在保存…" : "保存账户"}</Button>
      </>}>
      <div className="space-y-3">
        <Input label="账户别名" value={account.label} onChange={(event) => setAccount({ ...account, label: event.target.value })} />
        <Select label="厂商预设" value={account.provider_id} options={[
          { value: "", label: "选择厂商", disabled: true },
          ...Object.entries(PROVIDER_PRESETS).map(([value, item]) => ({ value, label: item.label })),
        ]} onChange={(event) => {
          const id = event.target.value as keyof typeof PROVIDER_PRESETS;
          setAccount({ ...account, provider_id: id, base_url: PROVIDER_PRESETS[id].baseUrl, profile_ids: [] });
        }} />
        <Input label="账户 API 地址" value={account.base_url} onChange={(event) => setAccount({ ...account, base_url: event.target.value })} />
        <Select label="账户凭据操作" value={accountKeyAction} onChange={(event) => setAccountKeyAction(event.target.value)}
          options={[{ value: "keep", label: "保留现有凭据" }, { value: "replace", label: "设置或替换 API Key" }, { value: "clear", label: "清除凭据" }]} />
        {accountKeyAction === "replace" && <Input label="账户 API Key" type="password" autoComplete="new-password" value={accountKey}
          onChange={(event) => setAccountKey(event.target.value)} />}
        <details className="u2-disclosure">
          <summary>关联模型与说明</summary>
          <div className="space-y-2 pt-2">
            <p className="settings-help">替换或清除共享凭据会影响使用该凭据的模型和账户。</p>
            {models.filter((model) => model.provider === account.provider_id).map((model) => (
              <label key={model.id} className="flex gap-2 text-sm">
                <input type="checkbox" checked={account.profile_ids.includes(model.id)} onChange={(event) => setAccount({
                  ...account, profile_ids: event.target.checked ? [...account.profile_ids, model.id] : account.profile_ids.filter((id) => id !== model.id),
                })} />{model.label} · {model.model}
              </label>
            ))}
          </div>
        </details>
      </div>
    </SettingsDialogLayout>}

    {draft && <SettingsDialogLayout title={draft.id ? "编辑模型池" : "新建模型池"} onClose={() => { setDraft(null); setError(""); }} error={error} notice=""
      action={<Button disabled={busy} onClick={() => void run(async () => {
        const saved = await poolRequest<ModelPool>(draft.id ? "PUT" : "POST", "/api/model-pools" + (draft.id ? "/" + encodeURIComponent(draft.id) : ""),
          { ...draft, expected_revision: draft.id ? draft.revision : null });
        setActive(saved.id);
        setDraft(null);
      })}>{busy ? "正在保存…" : "保存模型池"}</Button>}>
      <fieldset disabled={busy} className="min-w-0 space-y-3">
        <Input label="模型池名称" value={draft.label} onChange={(event) => setDraft({ ...draft, label: event.target.value })} />
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" className="accent-[var(--accent-soft-fg)]" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />启用模型池
        </label>
        <p className="settings-help">遇到超时、限流等可切换故障时，本次模型请求会发送给下一候选。加入其他厂商或账户，即允许其接收本次模型请求的内容。</p>

        {draft.tiers.map((tier, index) => (
          <section key={tier.id} className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3">
            <div className="flex flex-wrap items-end gap-2">
              <div className="min-w-32 flex-1"><Input label={"第 " + (index + 1) + " 级名称"} value={tier.label} onChange={(event) => changeTier(index, { label: event.target.value })} /></div>
              {draft.tiers.length > 1 && <div className="flex shrink-0 gap-1">
                <Button variant="ghost" size="sm" iconOnly aria-label="上移层级" title="上移层级" disabled={index === 0} onClick={() => moveTier(index, -1)}><ArrowUp size={15} /></Button>
                <Button variant="ghost" size="sm" iconOnly aria-label="下移层级" title="下移层级" disabled={index === draft.tiers.length - 1} onClick={() => moveTier(index, 1)}><ArrowDown size={15} /></Button>
                <Button variant="ghost" size="sm" iconOnly aria-label="删除层级" title="删除层级" onClick={() => setDraft({ ...draft, tiers: draft.tiers.filter((item) => item.id !== tier.id) })}><X size={15} /></Button>
              </div>}
            </div>
            {tier.profile_ids.map((id, candidateIndex) => (
              <div key={id} className="flex flex-wrap items-center gap-1 text-sm">
                <span className="min-w-24 flex-1 break-words">{candidateIndex + 1}. {describeCandidate(id)}</span>
                <Button variant="ghost" size="sm" iconOnly aria-label="提前" title="提前候选" disabled={candidateIndex === 0} onClick={() => {
                  const ids = [...tier.profile_ids];
                  [ids[candidateIndex - 1], ids[candidateIndex]] = [ids[candidateIndex], ids[candidateIndex - 1]];
                  changeTier(index, { profile_ids: ids });
                }}><ArrowUp size={15} /></Button>
                <Button variant="ghost" size="sm" iconOnly aria-label="移除候选" title="移除候选" onClick={() => changeTier(index, { profile_ids: tier.profile_ids.filter((item) => item !== id) })}><X size={15} /></Button>
              </div>
            ))}
            <Select label={"第 " + (index + 1) + " 级添加候选"} value=""
              options={[{ value: "", label: "选择模型", disabled: true }, ...candidates.map((model) => ({ value: model.id, label: describeCandidate(model.id) }))]}
              onChange={(event) => changeTier(index, { profile_ids: [...tier.profile_ids, event.target.value] })} />
          </section>
        ))}
        <Button variant="secondary" size="sm" onClick={() => setDraft({ ...draft, tiers: [...draft.tiers, {
          id: crypto.randomUUID(), label: "第 " + (draft.tiers.length + 1) + " 级", profile_ids: [],
        }] })}>添加层级</Button>
        <details className="u2-disclosure">
          <summary>高级选项</summary>
          <div className="grid gap-3 pt-2 sm:grid-cols-2">
            <Input label="候选超时（毫秒）" type="number" min={1} value={draft.attempt_timeout_ms}
              onChange={(event) => setDraft({ ...draft, attempt_timeout_ms: Number(event.target.value) })} />
            <Input label="整轮总时限（毫秒）" type="number" min={1} value={draft.total_timeout_ms}
              onChange={(event) => setDraft({ ...draft, total_timeout_ms: Number(event.target.value) })} />
          </div>
        </details>
      </fieldset>
    </SettingsDialogLayout>}
  </section>;
}
