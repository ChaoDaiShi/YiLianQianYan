import { useEffect, useMemo, useRef, useState } from "react";
import { Copy, MessageSquareText, Plus, RefreshCw, Save, Sparkles, Trash2 } from "lucide-react";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import { listAgents, type AgentDefinition } from "../../api/tasks";
import { listLlmModels } from "../../api/system";
import type { LlmModel } from "../../types";
import { createChannelPreset, deleteChannelPreset, previewChannelPreset, updateChannelPreset, type ChannelPreset } from "../../api/channels";

type PresetDraft = {
  id: string | null;
  name: string;
  agent_id: string | null;
  model_profile_id: string | null;
  instructions: string;
  builtin: boolean;
};
type PresetNavigation =
  | { kind: "select"; preset: ChannelPreset }
  | { kind: "new" }
  | { kind: "duplicate"; preset: ChannelPreset };
interface PresetEditorSessionDraft { selectedId: string; draft: PresetDraft | null; dirtyNewPreset: boolean; hasUnsavedChanges: boolean; sampleMessage: string }
let presetEditorSessionDraft: PresetEditorSessionDraft | null = null;

export function clearChannelPresetDraft() {
  presetEditorSessionDraft = null;
}
export function hasUnsavedChannelPresetDraft() {
  return Boolean(presetEditorSessionDraft?.hasUnsavedChanges);
}

const toDraft = (preset: ChannelPreset): PresetDraft => ({
  id: preset.id,
  name: preset.name,
  agent_id: preset.agent_id,
  model_profile_id: preset.model_profile_id,
  instructions: preset.instructions,
  builtin: preset.builtin,
});
const emptyDraft = (): PresetDraft => ({ id: null, name: "", agent_id: null, model_profile_id: null, instructions: "", builtin: false });
const problem = (value: unknown) => value instanceof Error ? value.message : "回复预设操作失败";

export default function ChannelPresetsPanel({ presets, onPresetsChange, onUnsavedChange }: {
  presets: ChannelPreset[];
  onPresetsChange: (presets: ChannelPreset[]) => void;
  onUnsavedChange: (value: boolean, busy: boolean) => void;
}) {
  const restored = presetEditorSessionDraft?.hasUnsavedChanges ? presetEditorSessionDraft : null;
  const [selectedId, setSelectedId] = useState(restored?.selectedId ?? "");
  const [draft, setDraft] = useState<PresetDraft | null>(restored?.draft ?? null);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [models, setModels] = useState<LlmModel[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [catalogError, setCatalogError] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [sampleMessage, setSampleMessage] = useState(restored?.sampleMessage ?? "");
  const [preview, setPreview] = useState<{ reply: string; conversation_id: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [dirtyNewPreset, setDirtyNewPreset] = useState(restored?.dirtyNewPreset ?? false);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(restored?.hasUnsavedChanges ?? false);
  const [pendingNavigation, setPendingNavigation] = useState<PresetNavigation | null>(null);
  const alive = useRef(true);

  const sortedPresets = useMemo(() => [...presets].sort((a, b) => Number(b.builtin) - Number(a.builtin) || a.name.localeCompare(b.name, "zh-CN")), [presets]);
  const selected = presets.find(preset => preset.id === selectedId) ?? null;
  const current = draft ?? (selected ? toDraft(selected) : emptyDraft());
  const activeAgents = agents.filter(agent => agent.enabled);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  useEffect(() => { onUnsavedChange(hasUnsavedChanges, busy); }, [busy, hasUnsavedChanges, onUnsavedChange]);
  useEffect(() => {
    if (hasUnsavedChanges) presetEditorSessionDraft = { selectedId, draft, dirtyNewPreset, hasUnsavedChanges, sampleMessage };
    else if (presetEditorSessionDraft?.hasUnsavedChanges) presetEditorSessionDraft = null;
  }, [selectedId, draft, dirtyNewPreset, hasUnsavedChanges, sampleMessage]);

  useEffect(() => {
    if (dirtyNewPreset) return;
    if (selectedId && presets.some(preset => preset.id === selectedId)) return;
    setSelectedId(presets[0]?.id ?? "");
  }, [dirtyNewPreset, presets, selectedId]);

  async function loadCatalogs() {
    setCatalogLoading(true);
    setCatalogError("");
    try {
      const [agentResult, modelRows] = await Promise.all([listAgents(), listLlmModels()]);
      if (!agentResult.ok) throw new Error(agentResult.error);
      if (modelRows === null) throw new Error("模型档案列表无法加载");
      if (!alive.current) return;
      setAgents(agentResult.data);
      setModels(modelRows);
    } catch (reason) {
      if (alive.current) setCatalogError(problem(reason));
    } finally {
      if (alive.current) setCatalogLoading(false);
    }
  }

  useEffect(() => { void loadCatalogs(); }, []);

  function applyNavigation(action: PresetNavigation) {
    setPendingNavigation(null);
    setPreview(null);
    setError("");
    if (action.kind === "select") {
      setSelectedId(action.preset.id);
      setDraft(null);
      setDirtyNewPreset(false);
      setHasUnsavedChanges(false);
      clearChannelPresetDraft();
      return;
    }
    const nextDraft = action.kind === "new"
      ? emptyDraft()
      : { ...toDraft(action.preset), id: null, name: `${action.preset.name}（自定义）`, builtin: false };
    presetEditorSessionDraft = { selectedId: "", draft: nextDraft, dirtyNewPreset: true, hasUnsavedChanges: true, sampleMessage };
    setSelectedId("");
    setDirtyNewPreset(true);
    setHasUnsavedChanges(true);
    setDraft(nextDraft);
  }

  function navigate(action: PresetNavigation) {
    if (hasUnsavedChanges) {
      setPendingNavigation(action);
      return;
    }
    applyNavigation(action);
  }

  function selectPreset(preset: ChannelPreset) { navigate({ kind: "select", preset }); }
  function startNew() { navigate({ kind: "new" }); }
  function duplicate() {
    const source = presets.find(preset => preset.id === current.id);
    if (source) navigate({ kind: "duplicate", preset: source });
  }

  function updateDraft(patch: Partial<PresetDraft>) {
    const next = { ...current, ...patch, id: current.id, builtin: current.builtin };
    presetEditorSessionDraft = { selectedId, draft: next, dirtyNewPreset, hasUnsavedChanges: true, sampleMessage };
    setDraft(next);
    setHasUnsavedChanges(true);
    setPreview(null);
  }

  async function save() {
    setBusy(true);
    setError("");
    try {
      const input = {
        name: current.name.trim(),
        agent_id: current.agent_id,
        model_profile_id: current.model_profile_id,
        instructions: current.instructions.trim(),
      };
      const result = current.id
        ? await updateChannelPreset(current.id, input)
        : await createChannelPreset(input);
      if (!alive.current) return;
      const next = current.id
        ? presets.map(preset => preset.id === result.preset.id ? result.preset : preset)
        : [...presets, result.preset];
      onPresetsChange(next);
      setSelectedId(result.preset.id);
      setDraft(null);
      setDirtyNewPreset(false);
      setHasUnsavedChanges(false);
      clearChannelPresetDraft();
      setPreview(null);
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  async function runPreview() {
    if (!current.id || hasUnsavedChanges || !sampleMessage.trim()) return;
    setBusy(true);
    setError("");
    setPreview(null);
    try {
      const result = await previewChannelPreset(current.id, sampleMessage);
      if (alive.current) setPreview(result);
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  async function remove() {
    if (!current.id) return;
    setBusy(true);
    setError("");
    try {
      await deleteChannelPreset(current.id);
      if (!alive.current) return;
      const next = presets.filter(preset => preset.id !== current.id);
      onPresetsChange(next);
      setSelectedId(next[0]?.id ?? "");
      setDraft(null);
      setDirtyNewPreset(false);
      setHasUnsavedChanges(false);
      clearChannelPresetDraft();
      setPreview(null);
      setConfirmDelete(false);
    } catch (reason) {
      if (alive.current) {
        setError(problem(reason));
        setConfirmDelete(false);
      }
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  const readOnly = current.builtin;
  const selectedAgent = activeAgents.find(agent => agent.id === current.agent_id);
  const selectedModel = models.find(model => model.id === current.model_profile_id);

  return (
    <section className="channel-presets-page">
      <div className="channel-catalog-refresh"><span>角色与模型从当前配置读取。</span><Button variant="ghost" size="sm" onClick={() => void loadCatalogs()} disabled={catalogLoading}><RefreshCw size={13} />刷新角色与模型</Button></div>

      {(catalogError || error) && <p className="channels-error" role="alert">{catalogError || error}</p>}

      <div className="channel-presets-layout">
        <nav className="channel-preset-list" aria-label="回复预设">
          <div className="channel-preset-list-heading"><span>我的预设</span><span>{sortedPresets.length}</span></div>
          {sortedPresets.map(preset => (
            <button key={preset.id} type="button" className={preset.id === selectedId && !dirtyNewPreset ? "selected" : ""} onClick={() => selectPreset(preset)}>
              <span className="channel-preset-list-icon"><MessageSquareText size={15} /></span>
              <span className="channel-preset-list-copy"><strong>{preset.name}</strong><small>{preset.builtin ? "内置模板" : "自定义预设"}</small></span>
            </button>
          ))}
          <Button variant="secondary" size="sm" onClick={startNew}><Plus size={15} />新建预设</Button>
        </nav>

        <div className="channel-preset-editor">
          {catalogLoading && <p role="status" className="channels-muted">正在读取当前角色与模型档案…</p>}
          <div className="channel-preset-editor-heading">
            <div><span className="channel-step-label">{current.builtin ? "内置模板" : current.id ? "自定义预设" : "新预设"}</span><h3>{current.name || "命名你的回复预设"}</h3></div>
            {current.builtin && <span className="channel-preset-builtin-tag">可复制后编辑</span>}
          </div>

          <label>预设名称
            <input aria-label="预设名称" value={current.name} readOnly={readOnly} maxLength={80} onChange={event => updateDraft({ name: event.target.value })} placeholder="例如：昔涟 · 温柔日常" />
          </label>
          <div className="channel-preset-select-grid">
            <label>关联角色
              <select aria-label="关联角色" value={current.agent_id ?? ""} disabled={readOnly || catalogLoading || Boolean(catalogError)} onChange={event => updateDraft({ agent_id: event.target.value || null })}>
                <option value="">使用默认聊天角色</option>
                {current.agent_id && !selectedAgent && <option value={current.agent_id} disabled>已保存引用：{current.agent_id}</option>}
                {activeAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
              </select>
            </label>
            <label>回复模型
              <select aria-label="回复模型" value={current.model_profile_id ?? ""} disabled={readOnly || catalogLoading || Boolean(catalogError)} onChange={event => updateDraft({ model_profile_id: event.target.value || null })}>
                <option value="">使用默认聊天模型</option>
                {current.model_profile_id && !selectedModel && <option value={current.model_profile_id} disabled>已保存引用：{current.model_profile_id}</option>}
                {models.map(model => <option key={model.id} value={model.id}>{model.provider} · {model.label}{model.active ? " · 当前启用" : ""}</option>)}
              </select>
            </label>
          </div>
          <label>回复风格与提示
            <textarea aria-label="回复风格与提示" rows={6} value={current.instructions} readOnly={readOnly} onChange={event => updateDraft({ instructions: event.target.value })} placeholder="描述语气、篇幅和回答习惯。提示不会授予工具或本机资料访问权限。" />
          </label>

          <section className="channel-preset-trial" aria-labelledby="channel-preset-trial-heading">
            <div className="channel-preset-trial-heading"><Sparkles size={17} /><div><h4 id="channel-preset-trial-heading">本地试用</h4><p>调用当前配置的聊天模型，只生成本地预览，不会向外部渠道发送。</p></div></div>
            <label>试用消息
              <textarea aria-label="试用消息" rows={3} value={sampleMessage} onChange={event => { setSampleMessage(event.target.value); setPreview(null); }} placeholder="输入一条想试用的消息" />
            </label>
            <div className="channel-preset-preview-meta">
              <span>角色：{selectedAgent?.name ?? (current.agent_id ? `引用 ${current.agent_id}` : "默认聊天角色")}</span>
              <span>模型：{selectedModel?.label ?? (current.model_profile_id ? `引用 ${current.model_profile_id}` : "默认聊天模型")}</span>
            </div>
            {preview && <div className="channel-preset-preview" aria-live="polite"><strong>本地生成结果</strong><p>{preview.reply}</p><small>本地对话：{preview.conversation_id}</small></div>}
            <Button variant="secondary" size="sm" disabled={busy || !current.id || hasUnsavedChanges || !sampleMessage.trim()} loading={busy} loadingLabel="正在请求模型…" onClick={() => void runPreview()}><Sparkles size={14} />运行本地试用</Button>
            {(!current.id || hasUnsavedChanges) && <p className="channels-muted">{current.id ? "保存修改后，再用当前版本试用。" : "先保存预设，再运行本地试用。"}</p>}
          </section>

          <footer className="channel-preset-actions">
            {current.id && <Button variant="secondary" disabled={busy} onClick={duplicate}><Copy size={15} />复制为自定义</Button>}
            {!current.builtin && <>
              {current.id && <Button variant="danger" disabled={busy} onClick={() => setConfirmDelete(true)}><Trash2 size={15} />删除预设</Button>}
              <Button disabled={busy || catalogLoading || Boolean(catalogError) || !current.name.trim() || !current.instructions.trim()} loading={busy} loadingLabel="保存中…" onClick={() => void save()}><Save size={15} />保存预设</Button>
            </>}
          </footer>
        </div>
      </div>

      <Modal open={confirmDelete} onClose={() => { if (!busy) setConfirmDelete(false); }} title="删除回复预设" size="sm" footer={(
        <>
          <Button variant="secondary" disabled={busy} onClick={() => setConfirmDelete(false)}>取消</Button>
          <Button variant="danger" disabled={busy} loading={busy} onClick={() => void remove()}>删除预设</Button>
        </>
      )}>
        <p className="channel-confirm-copy">确定删除“{current.name}”？如果渠道正在使用此预设，服务器会拒绝删除并说明需要先改选。</p>
      </Modal>
      <Modal open={Boolean(pendingNavigation)} onClose={() => setPendingNavigation(null)} title="放弃未保存的预设修改？" size="sm" footer={(
        <>
          <Button variant="secondary" onClick={() => setPendingNavigation(null)}>继续编辑</Button>
          <Button variant="danger" onClick={() => { if (pendingNavigation) applyNavigation(pendingNavigation); }}>放弃修改并继续</Button>
        </>
      )}>
        <p className="channel-confirm-copy">当前预设的修改尚未保存。继续此操作会放弃这些更改。</p>
      </Modal>
    </section>
  );
}
