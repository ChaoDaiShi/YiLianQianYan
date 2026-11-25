import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { getProviderReadiness } from "../../api/providerConnection";
import { planTaskGraph, type ApiError } from "../../api/taskWorld";
import { listWorkspaces } from "../../api/workspaces";
import { getAttentionProofDraft, type AttentionGoalSource, type ProofDraft } from "../../api/proofloop";
import { Button, ErrorState, Modal, Tabs, Textarea } from "../../components/ui";
import GoalComposer from "../task-world/proofloop/GoalComposer";
import PlanReviewPanel from "../task-world/proofloop/PlanReviewPanel";
import { useCanvasCapabilities } from "../task-world/capability-picker/useCanvasCapabilities";
import { createEmptyTaskGraph } from "./taskGraphActions";
import { taskGraphPath } from "./taskGraphRoutes";

export default function TaskGraphPlanner({ workspaceId, initialMode, initialGoal = "", source, onClose }: { workspaceId?: string; initialMode: "standard" | "proofloop"; initialGoal?: string; source?: AttentionGoalSource; onClose: () => void }) {
  const navigate = useNavigate();
  const [mode, setMode] = useState(initialMode);
  const [goal, setGoal] = useState(initialGoal);
  const [draft, setDraft] = useState<ProofDraft | null>(null);
  const [scope, setScope] = useState<string>();
  const [scopeError, setScopeError] = useState("");
  const [sourceLoading, setSourceLoading] = useState(Boolean(source));
  const [sourceError, setSourceError] = useState("");
  const [modelUnavailable, setModelUnavailable] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const inFlight = useRef(false);
  const capabilities = useCanvasCapabilities();

  useEffect(() => {
    let current = true;
    void getProviderReadiness().then(readiness => { if (current) setModelUnavailable(!readiness?.model.available); });
    void listWorkspaces().then(result => {
      if (!current) return;
      if (!result.ok) { setScopeError(result.error); return; }
      const chosen = result.data.find(space => workspaceId ? space.id === workspaceId && space.status === "active" : space.is_default);
      if (chosen) { setScope(chosen.id); setScopeError(""); }
      else setScopeError("请选择一个可用的工作空间后再规划目标。");
    });
    return () => { current = false; };
  }, [workspaceId]);

  useEffect(() => {
    if (!source) return;
    const cancel = new AbortController();
    void getAttentionProofDraft(source, cancel.signal).then(result => {
      if (cancel.signal.aborted) return;
      if (!result.ok) setSourceError(result.error.message);
      else if (result.data.draft && result.data.draft.contract.workspace_id !== workspaceId) {
        setSourceError("此草稿与当前工作空间不匹配。");
      } else setDraft(result.data.draft);
      setSourceLoading(false);
    });
    return () => cancel.abort();
  }, [source, workspaceId]);

  function plan() {
    const text = goal.trim();
    if (!text) return;
    void createEmptyTaskGraph({ createGraph: async ({ id }) => {
      const readiness = await getProviderReadiness();
      setModelUnavailable(!readiness?.model.available);
      if (!readiness?.model.available) return { ok: false, error: { status: 0, code: "model_unavailable", message: "请先配置可用的模型服务。" } };
      return planTaskGraph(id, text);
    }, navigate, setCreating, setError, inFlight, makeGraphId: () => crypto.randomUUID() });
  }

  function planningPanel() {
    if (mode === "standard") return <>
      <Textarea label="描述任务目标" value={goal} onChange={event => setGoal(event.target.value)} rows={3} maxLength={4000} />
      <Button disabled={creating || !goal.trim()} onClick={plan}>{creating ? "正在生成计划…" : "生成任务计划"}</Button>
    </>;
    if (sourceLoading) return <p role="status">正在读取同一观察的目标草稿…</p>;
    if (sourceError) return <ErrorState title="无法打开关注提议" description={sourceError} />;
    if (draft) return <PlanReviewPanel draft={draft} onUpdateDraft={setDraft} onBackToEdit={source ? undefined : () => setDraft(null)}
      onConfirmed={confirmed => navigate(taskGraphPath(confirmed.graph_id))} />;
    if (scopeError) return <ErrorState title="无法读取规划工作空间" description={scopeError} />;
    if (!scope) return <p role="status">正在读取工作空间…</p>;
    if (capabilities.error) return <ErrorState title="无法读取规划可用的能力" description={capabilities.error} onRetry={() => void capabilities.refresh()} />;
    if (capabilities.loading) return <p role="status">正在读取规划可用的能力…</p>;
    return <GoalComposer workspaceId={scope} initialGoal={goal} source={source} availableCapabilities={capabilities.items}
      modelUnavailable={modelUnavailable} onDraftCreated={setDraft} onCancel={onClose} />;
  }

  return <Modal open onClose={onClose} title="规划任务画布" size="xl">
    <Tabs tabs={[...(source ? [] : [{ id: "standard", label: "快速规划", panelId: false as const }]), { id: "proofloop", label: "可验证规划", panelId: false }]}
      activeId={mode} onChange={id => { setMode(id === "proofloop" ? "proofloop" : "standard"); setDraft(null); }} variant="segment" size="sm" aria-label="规划方式" />
    <div className="mt-4 space-y-3" role="tabpanel" aria-labelledby={`tab-${mode}`}>
      {planningPanel()}
      {modelUnavailable && <p className="text-sm text-[var(--warning-fg)]">还没有配置可用的模型服务。 <Link className="underline" to="/settings?section=model">前往模型设置</Link></p>}
      {error && <p role="alert" className="text-sm text-[var(--danger-fg)]">{error.message}</p>}
    </div>
  </Modal>;
}
