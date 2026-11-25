import { useEffect, useState } from "react";
import { Brain, ChevronDown, Cpu, MessagesSquare, Search, SlidersHorizontal } from "lucide-react";
import { getChatModelReadiness, listCliProviders, type CliProvider } from "../../api/providerConnection";
import { listLlmModels } from "../../api/system";
import { listAgents, type AgentDefinition } from "../../api/tasks";
import type { LlmModel } from "../../types";
import { availableThinkingModes, thinkingLevels, type ThinkingMode } from "./chatRequestOptions";
import { ComposerOption } from "./ComposerOption";
import { ComposerPopover } from "./ComposerPopover";

interface Props {
  conversationId?: string | null;
  modelId: string;
  agentId?: string;
  thinking: ThinkingMode;
  disabled: boolean;
  onModel: (id: string) => void;
  onAgent?: (id: string) => void;
  onThinking: (mode: ThinkingMode) => void;
}

type Readiness = Awaited<ReturnType<typeof getChatModelReadiness>>;

export function ComposerModelControls({ conversationId, modelId, agentId, thinking, disabled, onModel, onAgent, onThinking }: Props) {
  const [models, setModels] = useState<LlmModel[]>([]);
  const [cliProviders, setCliProviders] = useState<CliProvider[]>([]);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [modelListState, setModelListState] = useState<"loading" | "ready" | "error">("loading");
  const [query, setQuery] = useState("");
  const [revision, setRevision] = useState(0);
  const [readiness, setReadiness] = useState<{ conversationId: Props["conversationId"]; result: Readiness }>();
  useEffect(() => {
    let active = true;
    void getChatModelReadiness(conversationId ?? null).then(result => {
      if (active) setReadiness({ conversationId, result });
    });
    return () => { active = false; };
  }, [conversationId]);
  useEffect(() => {
    let active = true;
    void listLlmModels().then(rows => {
      if (!active) return;
      if (rows) setModels(rows.filter(model => model.api_format === "openai"));
      setModelListState(rows ? "ready" : "error");
    });
    void listCliProviders().then(rows => {
      if (active && rows) setCliProviders(rows);
    });
    return () => { active = false; };
  }, [revision]);
  useEffect(() => {
    if (!onAgent) return;
    let active = true;
    void listAgents().then(result => {
      if (active && result.ok) setAgents(result.data.filter(agent => agent.enabled));
    });
    return () => { active = false; };
  }, [onAgent]);

  const current = readiness?.conversationId === conversationId ? readiness?.result : undefined;
  const selected = models.find(model => model.id === modelId);
  const selectedCli = cliProviders.find(provider => provider.id === modelId);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matches = normalizedQuery ? models.filter(model => [model.model, model.label, model.provider].some(value => value?.toLocaleLowerCase().includes(normalizedQuery))) : models;
  const selectedName = selectedCli?.label || selected?.label || selected?.model || current?.model || "模型";
  const levels: ThinkingMode[] = selectedCli ? ["standard"] : selected ? availableThinkingModes(selected.provider, selected.model) :
    current?.thinking_modes ?? (current ? availableThinkingModes(current.provider, current.model) : ["standard"]);
  const level = Math.max(0, levels.indexOf(thinking));
  const depth = thinkingLevels[levels[level]];
  const selectModel = (id: string) => {
    if (id === modelId) return;
    onModel(id);
    onThinking("standard");
  };

  return <ComposerPopover label="模型与思考设置" disabled={disabled} className="composer-model-trigger"
    trigger={<><SlidersHorizontal size={15} /><span className="composer-model-label">{selectedName}{level > 0 ? ` · ${depth.label}` : ""}</span><ChevronDown size={11} /></>}>
    {() => <div className="composer-model-options">
      <div className="composer-model-heading">对话模型</div>
      {models.length > 6 && <label className="composer-model-search"><Search size={15} aria-hidden="true" /><input type="search" aria-label="搜索对话模型" placeholder="搜索模型名称或供应商" value={query} onChange={event => setQuery(event.currentTarget.value)} /></label>}
      <div className="composer-menu-options composer-model-list" role="group" aria-label="对话模型">
        <ComposerOption icon={MessagesSquare} label="当前会话模型"
          description={current?.model || "沿用当前会话的模型路由"} selected={!modelId} disabled={disabled} onClick={() => selectModel("")} />
        {matches.map(model => <ComposerOption key={model.id} icon={Cpu} label={model.label || model.model}
          description={`${model.provider} · ${model.model}`} selected={modelId === model.id} disabled={disabled} onClick={() => selectModel(model.id)} />)}
        {cliProviders.filter(provider => !normalizedQuery || provider.label.toLocaleLowerCase().includes(normalizedQuery)).map(provider =>
          <ComposerOption key={provider.id} icon={Cpu} label={provider.label}
            description={provider.ready ? "文本对话 · 使用当前会话历史" : "需要配置 CLI 或开启外部模型权限"}
            selected={modelId === provider.id} disabled={disabled || !provider.ready} onClick={() => selectModel(provider.id)} />)}
        {modelListState === "loading" && <p className="composer-menu-hint" role="status">正在读取模型…</p>}
        {modelListState === "error" && <p className="composer-menu-hint" role="alert">模型列表读取失败 <button type="button" className="underline" onClick={() => { setModelListState("loading"); setRevision(value => value + 1); }}>重试</button></p>}
        {modelListState === "ready" && matches.length === 0 && <p className="composer-menu-hint" role="status">{normalizedQuery ? "没有匹配的模型" : "暂无已配置的对话模型"}</p>}
      </div>
      {onAgent && <label className="composer-depth-label px-2 py-2">
        <span className="composer-option-copy"><strong>智能体</strong></span>
        <select aria-label="智能体" value={agentId ?? ""} disabled={disabled} onChange={event => { onAgent(event.target.value); onThinking("standard"); }} className="min-w-0 max-w-full rounded-md border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1 text-xs">
          <option value="">自动选择</option>
          {agents.map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
        </select>
      </label>}
      <div className="composer-depth">
        <label className="composer-depth-label">
          <span className="composer-option-icon"><Brain size={17} aria-hidden="true" /></span>
          <span className="composer-option-copy"><strong>思考深度 · {depth.label}</strong>
            <small>{levels.length > 1 ? depth.description : current || selected ? "当前模型仅提供标准设置" : "等待模型能力信息"}</small>
          </span>
          <input type="range" aria-label="思考深度" aria-valuetext={depth.label} min={0} max={levels.length - 1} step={1}
            value={level} disabled={disabled || levels.length < 2} onChange={event => onThinking(levels[event.currentTarget.valueAsNumber])} />
        </label>
        {levels.length > 1 && <div className="composer-depth-ticks" aria-hidden="true">
          {levels.map(mode => <span key={mode} data-current={mode === levels[level]}>{thinkingLevels[mode].label}</span>)}
        </div>}
      </div>
    </div>}
  </ComposerPopover>;
}
