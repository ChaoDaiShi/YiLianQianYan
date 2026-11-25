import { useEffect, useState } from "react";
import { Button, Input, Select } from "../../../components/ui";
import { listLlmModels } from "../../../api/system";
import AccountCredentialFields from "../../llm/AccountCredentialFields";
import ModelCatalogField from "../../llm/ModelCatalogField";
import { credentialChoice, credentialLabels, credentialReady } from "../../llm/accountCredentials";
import { PROVIDER_PRESETS, type ProviderPresetId } from "../../llm/llmModelUtils";
import { poolRequest, type ProviderAccount, type ModelPool } from "../../llm/modelPools";
import { listBasicServices, listBasicAccounts, testBasicService, type BasicServiceKind, type BasicServiceProfile, type BasicServiceResult } from "../basicServices";
import SettingsDialogLayout from "./SettingsDialogLayout";

type DefaultSource = { source: "profile" | "pool" | null; id: string | null };

async function loadServiceSettings(kind: BasicServiceKind) {
  const [profiles, accounts, defaultSource, pools, models] = await Promise.all([
    listBasicServices(),
    listBasicAccounts(),
    poolRequest<DefaultSource>("GET", "/api/service-defaults/" + kind),
    poolRequest<ModelPool[]>("GET", "/api/model-pools"),
    listLlmModels(),
  ]);
  if (!models) throw new Error("无法读取已保存的模型凭据来源。");
  return {
    profiles: profiles.filter((profile) => profile.service_kind === kind),
    accounts,
    defaultSource,
    pools: pools.filter((pool) => pool.service_kind === kind),
    models,
  };
}

function newProfile(kind: BasicServiceKind): BasicServiceProfile {
  return {
    id: "", label: kind === "vision" ? "识图服务" : "生图服务", provider_account_id: "",
    service_kind: kind, model: kind === "vision" ? "deepseek-flash" : "",
    adapter: "openai", parameters: {}, revision: 0, test: null,
  };
}

const adapters = [
  { value: "openai", label: "OpenAI 兼容" },
  { value: "dashscope", label: "阿里云百炼" },
  { value: "minimax", label: "MiniMax Image Generation" },
];

export default function BasicServicePanel({ kind, onClose }: { kind: BasicServiceKind; onClose?: () => void }) {
  const vision = kind === "vision";
  const defaultProvider = vision ? "deepseek" : "openai";
  const [provider, setProvider] = useState<ProviderPresetId>(defaultProvider);
  const [base, setBase] = useState(PROVIDER_PRESETS[defaultProvider].baseUrl);
  const [key, setKey] = useState("");
  const [credentialModelId, setCredentialModelId] = useState("");
  const [data, setData] = useState<Awaited<ReturnType<typeof loadServiceSettings>> | null>(null);
  const [draft, setDraft] = useState<BasicServiceProfile | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | "manage" | null>(null);
  const [prompt, setPrompt] = useState(vision ? "请描述这张图片中的内容。" : "一朵粉色莲花，简洁插画，纯白背景。");
  const [image, setImage] = useState("");
  const [result, setResult] = useState<BasicServiceResult | null>(null);
  const [testPool, setTestPool] = useState("");

  useEffect(() => {
    let active = true;
    void loadServiceSettings(kind).then((loaded) => {
      if (!active) return;
      setData(loaded);
      const selected = loaded.defaultSource.source === "profile"
        ? loaded.profiles.find((profile) => profile.id === loaded.defaultSource.id) : undefined;
      const account = loaded.accounts.find((item) => item.id === selected?.provider_account_id);
      setProvider((account?.provider_id ?? defaultProvider) as ProviderPresetId);
      setDraft(selected ?? newProfile(kind));
      if (loaded.defaultSource.source === "pool" && loaded.defaultSource.id) setTestPool(loaded.defaultSource.id);
    }).catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, [kind, defaultProvider]);

  const accounts = data?.accounts ?? [];
  const account = accounts.find((item) => item.id === draft?.provider_account_id);
  const providerAccounts = accounts.filter((item) => item.provider_id === provider);
  const savedProfile = data?.profiles.find((profile) => profile.id === draft?.id);
  const savedAccount = accounts.find((item) => item.id === savedProfile?.provider_account_id);
  const ready = credentialReady(account, provider, key, credentialModelId);
  const effectiveBase = account?.base_url ?? base;
  const category = PROVIDER_PRESETS[provider]?.category;
  const showAddress = !draft?.provider_account_id && (category === "local" || category === "custom");

  function resetEditor(profile: BasicServiceProfile) {
    const selectedAccount = accounts.find((item) => item.id === profile.provider_account_id);
    const id = (selectedAccount?.provider_id ?? defaultProvider) as ProviderPresetId;
    setProvider(id);
    setBase(PROVIDER_PRESETS[id]?.baseUrl ?? "");
    setKey("");
    setCredentialModelId("");
    setDraft(profile);
    setResult(null);
    setError("");
    setNotice("");
  }

  async function run(operation: "save" | "test" | "manage", action: () => Promise<void>) {
    setBusy(operation);
    setError("");
    setNotice("");
    try {
      await action();
      setData(await loadServiceSettings(kind));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }

  const setDefaultProfile = (id: string) => poolRequest("PUT", "/api/service-defaults/" + kind, { source: "profile", id });

  async function saveProfile() {
    if (!draft) return;
    await run("save", async () => {
      let saved = account;
      if (!credentialReady(saved, provider, key, credentialModelId)) throw new Error("请先输入 API Key 或选择已配置的凭据。");
      if (!saved) {
        saved = await poolRequest<ProviderAccount>("POST", "/api/provider-accounts", {
          id: "", provider_id: provider, label: PROVIDER_PRESETS[provider].label, base_url: base,
          credential_id: null, profile_ids: [], ...credentialChoice(key, credentialModelId),
        });
        setDraft({ ...draft, provider_account_id: saved.id });
      } else if (key.trim() || credentialModelId) {
        saved = await poolRequest<ProviderAccount>("PUT", "/api/provider-accounts/" + encodeURIComponent(saved.id), {
          id: saved.id, provider_id: saved.provider_id, label: saved.label, base_url: saved.base_url,
          credential_id: saved.credential_id, profile_ids: saved.profile_ids, ...credentialChoice(key, credentialModelId),
        });
      }
      const savedCredentials = saved;
      setData((current) => current && {
        ...current,
        accounts: [...current.accounts.filter((item) => item.id !== savedCredentials.id), savedCredentials],
      });
      setKey("");
      setCredentialModelId("");
      if (!credentialReady(saved, saved.provider_id, "", "")) throw new Error("账户凭据尚不可用，请检查 API Key 或凭据存储。");
      const profile = await poolRequest<BasicServiceProfile>(draft.id ? "PUT" : "POST",
        "/api/service-profiles" + (draft.id ? "/" + encodeURIComponent(draft.id) : ""),
        { ...draft, provider_account_id: saved.id, expected_revision: draft.id ? draft.revision : null });
      setDraft(profile);
      try {
        await setDefaultProfile(profile.id);
      } catch (cause) {
        throw new Error("服务档案已保存，设为默认失败。" + (cause instanceof Error ? cause.message : String(cause)));
      }
      setNotice("已保存并设为默认");
    });
  }

  function uploadTestImage(file: File) {
    const reader = new FileReader();
    reader.onload = () => setImage(String(reader.result));
    reader.onerror = () => setError("无法读取测试图片，请重新选择。");
    reader.readAsDataURL(file);
  }

  const providerOptions = Object.entries(PROVIDER_PRESETS)
    .filter(([, preset]) => (preset.services as string[]).includes(kind))
    .map(([value, preset]) => ({ value, label: preset.label }));

  return (
    <SettingsDialogLayout title={vision ? "图像理解" : "图像生成"} onClose={onClose} error={error} notice={notice}
      action={<Button disabled={busy !== null || !draft?.model.trim() || !ready || (!draft.provider_account_id && !base.trim())}
        onClick={() => void saveProfile()}>{busy === "save" ? "正在保存…" : "保存并设为默认"}</Button>}>
      {!data ? !error && <p className="settings-help" role="status">正在读取服务配置…</p> : (
        <fieldset className="min-w-0 space-y-3" disabled={busy !== null}>
          {data.profiles.length > 0 && <Select label="服务档案" value={draft?.id ?? ""} options={[
              { value: "", label: "新建服务档案" },
              ...data.profiles.map((profile) => ({
                value: profile.id,
                label: profile.label + (data.defaultSource.source === "profile" && data.defaultSource.id === profile.id ? "（默认）" : ""),
              })),
            ]} onChange={(event) => {
              const profile = data.profiles.find((item) => item.id === event.target.value);
              resetEditor(profile ?? newProfile(kind));
            }} />}

          {draft && <>
            <Select label="服务商" value={provider} options={providerOptions} onChange={(event) => {
              const id = event.target.value as ProviderPresetId;
              setProvider(id);
              setBase(PROVIDER_PRESETS[id].baseUrl);
              setKey("");
              setCredentialModelId("");
              setNotice("");
              setDraft({
                ...draft, provider_account_id: "", model: vision && id === "deepseek" ? "deepseek-flash" : "",
                adapter: !vision && id === "minimax" ? "minimax" : !vision && id === "qwen" ? "dashscope" : "openai",
              });
            }} />
            {(providerAccounts.length > 0 || draft.provider_account_id) && <Select label="账户" value={draft.provider_account_id} options={[
              { value: "", label: "创建新账户" },
              ...providerAccounts.map((item) => ({ value: item.id, label: item.label + " · " + credentialLabels[item.credential_status ?? "unavailable"] })),
            ]} onChange={(event) => {
              setKey("");
              setCredentialModelId("");
              setNotice("");
              setDraft({ ...draft, provider_account_id: event.target.value });
            }} />}
            {showAddress && <Input label="API 地址" value={base} onChange={(event) => { setBase(event.target.value); setCredentialModelId(""); }} />}
            <AccountCredentialFields account={account} provider={provider} baseUrl={effectiveBase} models={data.models}
              apiKey={key} modelId={credentialModelId} onKey={setKey} onModel={setCredentialModelId} />
            <ModelCatalogField provider={provider} baseUrl={effectiveBase} apiKey={key}
              accountId={credentialModelId ? undefined : draft.provider_account_id} modelId={credentialModelId}
              enabled={ready} preserveValue={Boolean(draft.id)} value={draft.model}
              onChange={(model) => setDraft((current) => current && { ...current, model })}
              advancedFields={<>
                {!draft.provider_account_id && !showAddress && <Input label="API 地址" value={base}
                  onChange={(event) => { setBase(event.target.value); setCredentialModelId(""); }} />}
                <Input label="显示名称" value={draft.label} onChange={(event) => setDraft({ ...draft, label: event.target.value })} />
                {!vision && <Select label="图像协议" options={adapters} value={draft.adapter}
                  onChange={(event) => setDraft({ ...draft, adapter: event.target.value as BasicServiceProfile["adapter"] })} />}
                {draft.id && <div className="flex flex-wrap gap-2">
                  <Button variant="secondary" size="sm" disabled={busy !== null || !!key.trim() || !!credentialModelId || !savedAccount || !credentialReady(savedAccount, savedAccount.provider_id, "", "")}
                    onClick={() => void run("manage", async () => { await setDefaultProfile(draft.id); setNotice("已设为默认"); })}>设为默认</Button>
                  <Button variant="danger" size="sm" onClick={() => void run("manage", async () => {
                    await poolRequest("DELETE", "/api/service-profiles/" + encodeURIComponent(draft.id));
                    resetEditor(newProfile(kind));
                  })}>删除</Button>
                </div>}
              </>} />
            {data.defaultSource.source === "pool" && <p className="settings-help">保存并设为默认会替换当前模型池绑定。</p>}

            <details className="u2-disclosure">
              <summary>测试服务</summary>
              <div className="space-y-3 pt-2">
                <Input label={vision ? "识图问题" : "生图提示词"} value={prompt} onChange={(event) => setPrompt(event.target.value)} />
                {vision && <label className="block text-sm">上传图片<input className="mt-2 block w-full" type="file" accept="image/*" onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) uploadTestImage(file);
                }} /></label>}
                {vision && image && <img className="max-h-40 rounded-lg" src={image} alt="待识别图像" />}
                {data.pools.length > 0 && <Select label="测试来源" value={testPool} options={[
                  { value: "", label: "当前固定服务档案" },
                  ...data.pools.map((pool) => ({ value: pool.id, label: pool.label + " · 模型池" })),
                ]} onChange={(event) => setTestPool(event.target.value)} />}
                <Button variant="secondary" size="sm" disabled={busy !== null || (!testPool && !draft.id) || (vision && !image)}
                  onClick={() => void run("test", async () => {
                    setResult(null);
                    const response = testPool
                      ? await poolRequest<BasicServiceResult>("POST", "/api/model-pools/" + encodeURIComponent(testPool) + "/invoke", { prompt, image_url: vision ? image : undefined })
                      : await testBasicService(draft.id, prompt, vision ? image : undefined);
                    setResult(response);
                    if (!testPool) {
                      const tested = (await listBasicServices()).find((profile) => profile.id === draft.id);
                      if (tested) setDraft(tested);
                    }
                  })}>{busy === "test" ? "正在请求服务…" : vision ? "识别图片" : "生成图片"}</Button>
                {draft.test && <p className="settings-help">{draft.test.ok ? "测试成功" : "测试失败"} · 版本 {draft.test.revision} · {draft.test.tested_at}</p>}
                {result?.text && <p className="whitespace-pre-wrap break-words rounded-lg bg-[var(--surface-muted)] p-3 text-sm">{result.text}</p>}
                {result?.image_url && <div>
                  <img src={result.image_url} className="max-h-96 rounded-lg" alt="服务实际生成的图像" />
                  <a href={result.image_url} target="_blank" rel="noreferrer" className="text-sm underline">打开生成图像</a>
                </div>}
              </div>
            </details>
          </>}
        </fieldset>
      )}
    </SettingsDialogLayout>
  );
}
