import { useCallback, useEffect, useRef, useState } from "react";
import { ListTodo } from "lucide-react";
import { bindConversationGoal, getConversationGoal, type ConversationGoalBinding } from "../../api/conversationGoal";
import { listProofDrafts, type ProofDraftSummary } from "../../api/proofloop";
import { requestResult } from "../../core/api/http";
import GoalTrackingPanel from "../../features/task-world/goal-view/GoalTrackingPanel";
import { confirmedGoalChoices } from "../../features/task-world/goal-view/goalDraftChoices";
import { Button, Modal, Select } from "../ui";

export default function ConversationGoalControl({ conversationId, busy = false }: { conversationId: string; busy?: boolean }) {
  const [restricted, setRestricted] = useState(false);
  const [binding, setBinding] = useState<ConversationGoalBinding | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [open, setOpen] = useState(false);
  const [choices, setChoices] = useState<ProofDraftSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [choicesLoading, setChoicesLoading] = useState(false);
  const [choiceError, setChoiceError] = useState("");
  const [saving, setSaving] = useState(false);
  const request = useRef(0);

  const reload = useCallback(async () => {
    const ticket = ++request.current;
    setLoading(true);
    const result = await getConversationGoal(conversationId);
    if (ticket !== request.current) return;
    if (result.ok) { setBinding(result.data.binding); setRestricted(result.data.restricted === true); if (result.data.restricted) setOpen(false); setError(""); }
    else setError(result.error.message);
    setLoading(false);
  }, [conversationId]);

  useEffect(() => {
    void reload();
    const refresh = () => void reload();
    window.addEventListener("focus", refresh);
    return () => { request.current++; window.removeEventListener("focus", refresh); };
  }, [reload]);

  useEffect(() => {
    if (!open) return;
    let active = true;
    setChoicesLoading(true); setChoiceError("");
    void (async () => {
      const workspace = await requestResult<{ workspace_id: string | null }>("GET", `/api/conversations/${encodeURIComponent(conversationId)}/workspace`);
      if (!workspace.ok) throw new Error(workspace.error);
      const result = await listProofDrafts(workspace.data.workspace_id ?? undefined);
      if (!result.ok) throw new Error(result.error.message);
      if (active) { setChoices(confirmedGoalChoices(result.data.drafts)); setSelected(""); }
    })().catch((cause: unknown) => { if (active) setChoiceError(cause instanceof Error ? cause.message : String(cause)); })
      .finally(() => { if (active) setChoicesLoading(false); });
    return () => { active = false; };
  }, [conversationId, open]);

  async function bind() {
    if (!selected || saving || busy) return;
    setSaving(true); setChoiceError("");
    const result = await bindConversationGoal(conversationId, selected);
    if (result.ok) { setBinding(result.data.binding); setError(""); setOpen(false); }
    else setChoiceError(result.error.message);
    setSaving(false);
  }

  if (restricted && !error) return <p className="mx-3 mt-2 text-xs text-[var(--text-secondary)] min-[960px]:mx-5">此对话使用个人委托的范围与预算，请在时间任务中管理。</p>;
  return <section className="conversation-goal-control mx-3 mt-2 min-w-0 shrink-0 text-xs min-[960px]:mx-5" aria-label="对话关联任务" data-testid="conversation-goal-control">
    {error ? <div role="alert" className="flex flex-wrap items-center gap-2 text-[var(--danger)]"><span>关联任务读取失败：{error}</span><Button size="sm" variant="ghost" onClick={() => void reload()}>重试</Button></div>
      : loading && !binding ? <p role="status">正在读取关联任务…</p>
        : binding ? <div className="conversation-goal-state"><GoalTrackingPanel key={binding.graph_id} graphId={binding.graph_id} compact /></div>
          : <Button size="sm" variant="ghost" disabled={busy} title={busy ? "当前一轮结束后可关联目标" : undefined} onClick={() => setOpen(true)}><ListTodo size={14} />关联已有目标</Button>}
    <Modal open={open} onClose={() => { if (!saving) setOpen(false); }} title="关联目标任务" size="md" footer={<><Button variant="secondary" disabled={saving} onClick={() => setOpen(false)}>取消</Button><Button loading={saving} disabled={!selected || saving || busy || choicesLoading || Boolean(choiceError)} onClick={() => void bind()}>确认关联</Button></>}>
      <div className="space-y-4">
        <p className="text-sm text-[var(--text-secondary)]">选择已确认的目标；对话已选工作空间时，仅列出该空间的目标。关联后，对话、任务画布和空间展示同一目标及执行记录；此对话将持续使用该目标。</p>
        {busy && <p role="status">当前一轮结束后可关联目标</p>}
        {choiceError ? <p role="alert" className="text-sm text-[var(--danger)]">{choiceError}</p>
          : choicesLoading ? <p role="status">正在读取已有目标…</p>
            : choices.length ? <Select label="目标任务" value={selected} onChange={event => setSelected(event.target.value)} options={[{ value: "", label: "请选择一个目标" }, ...choices.map(choice => ({ value: choice.graph_id, label: choice.user_goal }))]} />
              : <p>当前范围没有已确认目标。可先在工作流的「任务画布」中规划并确认目标。</p>}
      </div>
    </Modal>
  </section>;
}
