import { useEffect, useState } from "react";
import { listLlmModels } from "../../api/system";
import { getProviderReadiness } from "../../api/providerConnection";
import type { LlmModel, LlmModelPayload } from "../../types";
import { PROVIDER_PRESETS, type ProviderPresetId } from "../llm/llmModelUtils";
import { connectSetupModel, type ConnectionStage } from "./connectSetupModel";

const initialDraft = () => ({
  provider: "deepseek", name: PROVIDER_PRESETS.deepseek.model,
  base_url: PROVIDER_PRESETS.deepseek.baseUrl, api_key: "",
});

export const CONNECTION_LABELS: Record<ConnectionStage | "checking", string> = {
  saving: "正在保存模型…", verifying: "正在测试连接…",
  activating: "正在启用模型…", checking: "正在确认模型状态…",
};
const FAILURE_MESSAGES: Record<ConnectionStage, string> = {
  saving: "模型未能保存，请检查服务地址、模型名称及本机凭据存储。输入已保留。",
  verifying: "连接测试失败，请检查已保存的服务地址、模型名称和 API Key，再点击重试。",
  activating: "连接测试已通过，但模型未能启用，请点击重试。",
};

export function useQuickModelSetup() {
  const [draft, setDraft] = useState(initialDraft);
  const [profile, setProfile] = useState<LlmModel | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [stage, setStage] = useState<ConnectionStage | "checking" | null>(null);
  const [ready, setReady] = useState(false);
  const [message, setMessage] = useState("");

  const load = async () => {
    setLoading(true);
    setLoadError("");
    const [models, readiness] = await Promise.all([listLlmModels(), getProviderReadiness()]);
    if (!models || !readiness) {
      setLoadError("无法读取模型配置，请确认本地服务可用后重新加载。");
    } else {
      const active = models.find(model => model.active);
      if (active) {
        setProfile(active);
        setDraft({ provider: active.provider, name: active.model, base_url: active.base_url, api_key: "" });
        setReady(Boolean(active.verified_at && !active.last_error && readiness.model.available));
      }
    }
    setLoading(false);
  };
  useEffect(() => { void load(); }, []);

  const changeField = (field: keyof typeof draft, value: string) => {
    setDraft(current => ({ ...current, [field]: value }));
    setReady(false);
    setMessage("");
  };
  const changeProvider = (provider: ProviderPresetId) => {
    const preset = PROVIDER_PRESETS[provider];
    setDraft({ provider, name: preset.model, base_url: preset.baseUrl, api_key: "" });
    setProfile(null);
    setReady(false);
    setMessage("");
  };

  const connect = async () => {
    setMessage("");
    setReady(false);
    const payload: LlmModelPayload = {
      provider: draft.provider, label: profile?.label || draft.name.trim(),
      model: draft.name.trim(), base_url: draft.base_url.trim(), api_format: "openai",
      api_key: draft.api_key || undefined, api_key_env: profile?.api_key_env || undefined,
      temperature: profile?.temperature ?? 0, max_tokens: profile?.max_tokens ?? 16384,
      invoke_timeout_ms: profile?.invoke_timeout_ms ?? 120000,
    };
    try {
      const result = await connectSetupModel(payload, profile?.id ?? null, setStage, saved => {
        setProfile(saved);
        setDraft(current => ({ ...current, api_key: "" }));
      });
      if (!result.ok) {
        setMessage(FAILURE_MESSAGES[result.stage]);
        return false;
      }
      setProfile(result.model);
      setStage("checking");
      const readiness = await getProviderReadiness();
      if (!readiness?.model.available) {
        setMessage("模型已启用，但未能确认可用状态，请检查本地服务后重试。");
        return false;
      }
      setReady(true);
      return true;
    } finally {
      setStage(null);
    }
  };
  return { draft, profile, loading, loadError, stage, ready, message, load, changeField, changeProvider, connect };
}
