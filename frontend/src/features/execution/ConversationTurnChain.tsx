import { useEffect, useState } from "react";
import { Badge, Button } from "../../components/ui";
import { getConversationContextManifest, getLatestConversationTurn, type ConversationContextSnapshot, type ConversationTurn } from "../../api/contextManifest";
import { getTurnExecutionAudit, type ExecutionAuditEvent } from "../../api/executionAudit";
import type { Message } from "../../types";
import type { PendingApproval } from "../../types/approval";
import type { ExecutionRecord } from "./model";
import { projectTurnChain } from "./turnChain";
import { projectTurnTimeline } from "./turnTimeline";
import TurnTimelineChart from "./TurnTimelineChart";

const statusLabels: Record<string, string> = {
  running: "运行中", waiting_approval: "待审批", waiting_input: "等待回答",
  completed: "本轮结束", failed: "执行失败", cancelled: "已取消", interrupted: "已中断",
};
const sourceLabels: Record<string, string> = {
  memory: "长期记忆", memory_retrieval: "记忆检索", bound_memory: "绑定记忆", prior_chat: "历史对话", harness: "Harness", attachment: "附件", knowledge: "知识库",
};
export default function ConversationTurnChain({ conversationId, messages, persistedMessages, history, pendingApprovals, refreshKey, runError }: {
  conversationId: string; messages: Message[]; persistedMessages?: Message[]; history: ExecutionRecord[]; pendingApprovals: PendingApproval[]; refreshKey: string; runError?: string | null;
}) {
  const latestUser = [...messages].reverse().find(message => message.role === "user");
  const [data, setData] = useState<{ conversationId: string; turn: ConversationTurn | null; context: ConversationContextSnapshot | null; audit: ExecutionAuditEvent[] } | null>(null);
  const [error, setError] = useState(false);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    if (!latestUser?.id) return;
    let active = true;
    setError(false);
    void getLatestConversationTurn(conversationId).then(async turn => {
      if (!active || (turn && turn.user_message_id !== latestUser.id)) return;
      const [saved, audit] = turn ? await Promise.all([
        getConversationContextManifest(conversationId).catch(() => null),
        getTurnExecutionAudit(conversationId, turn.started_at, turn.finished_at),
      ]) : [null, []];
      const context = saved && saved.turn_id === turn?.id ? saved : null;
      if (active) setData({ conversationId, turn, context, audit });
    }).catch(() => { if (active) setError(true); });
    return () => { active = false; };
  }, [conversationId, latestUser?.id, refreshKey, revision]);

  const current = data?.conversationId === conversationId && data.turn?.user_message_id === latestUser?.id ? data : null;
  const turn = current?.turn;
  const chain = turn && projectTurnChain(turn, messages, history, persistedMessages, pendingApprovals);
  if (!latestUser) return null;
  if (!chain || !turn || !current) return <section className="mb-4 rounded-lg border border-[var(--border-soft)] p-3 text-xs" aria-label="本轮工作链路">
    <h3 className="font-semibold">本轮工作链路</h3>
    {error ? <Button size="sm" variant="secondary" onClick={() => setRevision(value => value + 1)}>重试读取本轮记录</Button>
      : <p className="mt-1 text-[var(--text-secondary)]">{data?.conversationId === conversationId && !data.turn ? "此对话还没有本轮执行记录" : "等待本轮执行记录"}</p>}
  </section>;

  const sources = current.context?.manifest.entries ?? [];
  const timing = current.context?.timeline;
  const memory = current.context ? {
    at: timing?.memory_resolved_at,
    sources: timing?.memory_sources ?? sources.filter(entry => ["memory", "bound_memory", "prior_chat"].includes(entry.kind) && ["resolved", "truncated"].includes(entry.status)),
  } : undefined;
  const timeline = projectTurnTimeline(turn, chain.actions, current.audit, pendingApprovals);
  const status = chain.pending ? "待审批" : statusLabels[turn.status] ?? turn.status;
  return <section className="glass-card mb-4 rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3 text-xs" aria-label="本轮工作链路">
    <div className="flex flex-wrap items-center justify-between gap-1">
      <h3 className="text-xs font-semibold">本轮工作链路</h3>
      <Badge tone={chain.pending ? "warning" : turn.status === "completed" ? "success" : turn.status === "failed" ? "danger" : "default"}>{status}</Badge>
    </div>
    <p className="mt-2 text-[var(--text-secondary)]">
      {chain.actions.length ? `${chain.actions.length} 项动作` : "本轮暂无工具动作"}
      {chain.pending > 0 && ` · ${chain.pending} 项待审批`}{chain.approved > 0 && ` · ${chain.approved} 项已批准`}{chain.rejected > 0 && ` · ${chain.rejected} 项已拒绝`}{chain.verified > 0 && ` · ${chain.verified} 项已核验`}
    </p>
    <TurnTimelineChart key={turn.id} turn={turn} nodes={timeline} status={status} runError={runError}
      requestAt={timing?.request_received_at ?? turn.started_at} memory={memory} reply={chain.savedReply} />
    {sources.length > 0 && <details className="mt-1"><summary className="cursor-pointer text-[var(--text-secondary)]">查看上下文记录</summary>
      <ul className="mt-1 space-y-1">{sources.map((entry, index) => <li key={`${entry.kind}:${index}`} className="break-words text-[var(--text-secondary)]">
        {sourceLabels[entry.kind] ?? entry.kind} · {entry.kind === "harness" ? entry.reason ?? entry.source_id : entry.status === "resolved" ? "已采用" : entry.status === "truncated" ? "采用摘录" : "未采用"}{entry.kind === "harness" && entry.version ? ` · 版本 ${entry.version}` : ""}
      </li>)}</ul>
    </details>}
  </section>;
}
