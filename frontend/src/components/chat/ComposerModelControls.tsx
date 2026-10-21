import { useEffect, useState } from "react";
import { getChatModelReadiness } from "../../api/providerConnection";
import { listLlmModels } from "../../api/system";
import type { LlmModel } from "../../types";
import { supportsThinking, type ThinkingMode } from "./chatRequestOptions";
import { ChevronDown, SlidersHorizontal } from "lucide-react";
import { ComposerPopover } from "./ComposerPopover";
interface Props { conversationId?: string | null; modelId: string; thinking: ThinkingMode; disabled: boolean; onModel: (id: string) => void; onThinking: (mode: ThinkingMode) => void }
export function ComposerModelControls({ conversationId, modelId, thinking, disabled, onModel, onThinking }: Props) {
  const [models, setModels] = useState<LlmModel[]>([]);
  const [error, setError] = useState("");
  const [currentModel, setCurrentModel] = useState("");
  useEffect(() => { let active = true; void getChatModelReadiness(conversationId ?? null).then(readiness => { if (active) setCurrentModel(readiness?.model ?? ""); }); return () => { active = false; }; }, [conversationId]);
  useEffect(() => { let active = true; void listLlmModels().then(rows => {
    if (!active) return;
    if (rows) setModels(rows); else setError("模型列表读取失败");
  }); return () => { active = false; }; }, []);
  const selected = models.find(model => model.id === modelId);
  const supported = !selected || supportsThinking(selected.provider, selected.model);
  return <ComposerPopover label="模型与思考设置" disabled={disabled} className="composer-model-trigger" trigger={<><SlidersHorizontal size={15}/><span className="composer-model-label">{selected?.label || selected?.model || currentModel || "模型"}{thinking === "thinking" ? " · 思考" : ""}</span><ChevronDown size={11}/></>}>
    {() => <div className="composer-model-options"><label>对话模型
    <select aria-label="对话模型" title="为本轮对话选择模型" disabled={disabled} value={modelId} onChange={event => { onModel(event.target.value); onThinking("standard"); }} className="max-w-[180px] rounded-lg bg-[var(--panel)] px-2 py-1.5 text-[var(--text)]">
      <option value="">{currentModel ? `当前会话 · ${currentModel}` : "当前会话模型"}</option>
      {models.map(model => <option value={model.id} key={model.id}>{model.label || model.model}</option>)}
    </select></label><label>思考方式
    <select aria-label="思考模式" value={thinking} disabled={disabled} onChange={event => onThinking(event.target.value as ThinkingMode)} className="rounded-lg bg-[var(--panel)] px-2 py-1.5 text-[var(--text)]">
      <option value="standard">标准</option><option value="thinking" disabled={!supported}>深度思考{!supported ? "（此模型未支持）" : ""}</option>
    </select></label>
    {error && <span role="alert" className="text-[var(--danger)]">{error}</span>}
  </div>}
  </ComposerPopover>;
}
