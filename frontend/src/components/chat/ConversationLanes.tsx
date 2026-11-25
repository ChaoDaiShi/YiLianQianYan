import { useEffect, useState } from "react";
import { ListTree } from "lucide-react";
import { listConversationLanes, type ConversationLane } from "../../api/conversationLanes";
import { Button, Modal } from "../ui";

const stateLabels: Record<ConversationLane["run_status"], string> = {
  idle: "待发送", running: "处理中", waiting_approval: "等待审批", waiting_input: "等待补充",
  completed: "已结束", failed: "失败", cancelled: "已取消", interrupted: "已中断",
};

export function ConversationLanes({ root, selected, onSelect, onStartWork, revision }: {
  root: string; selected: string; onSelect: (id: string) => void; onStartWork: () => void; revision: number;
}) {
  const [open, setOpen] = useState(false);
  const [lanes, setLanes] = useState<ConversationLane[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    let timer: number | undefined;
    const load = async () => {
      try {
        const rows = await listConversationLanes(root);
        if (live) { setLanes(rows); setError(""); }
      } catch (cause) {
        if (live) setError(cause instanceof Error ? cause.message : "读取并行运行失败");
      } finally {
        if (live) timer = window.setTimeout(load, open ? 1200 : 4500);
      }
    };
    void load();
    return () => { live = false; window.clearTimeout(timer); };
  }, [root, open, revision]);
  const current = lanes.find(lane => lane.id === selected);
  const active = lanes.filter(lane => ["running", "waiting_approval", "waiting_input"].includes(lane.run_status)).length;
  const choose = (id: string) => { onSelect(id); setOpen(false); };
  return <>
    <button type="button" aria-label="并行运行" aria-haspopup="dialog" onClick={() => setOpen(true)} className="inline-flex shrink-0 items-center gap-1 rounded-lg px-2 py-1.5 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)]">
      <ListTree size={15} /><span className="chat-header-action-label">{current?.kind === "work" ? "后台任务" : current?.kind === "chat" ? "并行聊天" : "并行运行"}{active ? ` · ${active}` : ""}</span>
    </button>
    <Modal open={open} onClose={() => setOpen(false)} title="当前会话的并行运行" size="md" className="conversation-lanes-dialog">
      <div className="space-y-2">
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="secondary" aria-pressed={selected === root} onClick={() => choose(root)}>主对话</Button>
          <Button type="button" variant="secondary" onClick={() => { setOpen(false); onStartWork(); }}>发起新任务</Button>
        </div>
        {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
        {!error && lanes.length === 0 && <p className="text-sm text-[var(--text-muted)]">发送「新任务：…」可在当前会话启动后台工作。</p>}
        {lanes.map(lane => <button key={lane.id} type="button" aria-pressed={selected === lane.id} onClick={() => choose(lane.id)} className="flex w-full min-w-0 items-start justify-between gap-3 rounded-lg border border-[var(--border-soft)] px-3 py-2 text-left text-sm hover:bg-[var(--surface-hover)]">
          <span className="min-w-0 break-words">{lane.title}</span>
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{stateLabels[lane.run_status]}</span>
        </button>)}
      </div>
    </Modal>
  </>;
}
