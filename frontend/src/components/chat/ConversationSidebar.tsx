import { useEffect, useMemo, useState } from "react";
import { GitBranch, MessageSquare, Monitor, Plus, Radio, Search, Trash2, X } from "lucide-react";
import { buildHistoryEntries, originDescription } from "./conversationHistoryModel";
import "./conversationHistory.css";
import { deleteConversation } from "../../api/client";
import type { ConversationSummary } from "../../types";
import { Button, ConfirmDialog, EmptyState } from "../ui";
import { notifyConversationHistoryChanged } from "./conversationHistoryEvents";
import { deriveTaskDisplayTitle, looksLikeCommand } from "./taskTitle";
import type { ConversationFamily } from "../../features/connectors/messages/messageForest";

interface Props {
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onClose?: () => void;
  families?: ConversationFamily[];
  onShowBranches?: (id:string)=>void;
  conversations: ConversationSummary[];
  historyError: string;
  historyLoading: boolean;
}

export default function ConversationSidebar({
  activeId,
  onSelect,
  onNew,
  onClose,
  families = [],
  onShowBranches,
  conversations,
  historyError,
  historyLoading,
}: Props) {
  const [query, setQuery] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<ConversationSummary | null>(null);
  const [deleteError, setDeleteError] = useState("");
  const [source,setSource]=useState<"desktop"|"channel">("desktop");
  const activeSource=conversations.find(conversation=>conversation.id===activeId)?.origin?.kind;
  useEffect(()=>{if(activeSource)setSource(activeSource);},[activeId,activeSource]);

  const entries=useMemo(()=>buildHistoryEntries(conversations,families,query),[conversations,families,query]);
  const filtered=useMemo(()=>entries.filter(entry=>(entry.origin?.kind||"desktop")===source),[entries,source]);

  const selectConversation = (id: string) => {
    onSelect(id);
    onClose?.();
  };

  const createNew = () => {
    onNew();
    onClose?.();
  };

  const removeConversation = async (conversation: ConversationSummary) => {
    try {
      await deleteConversation(conversation.id);
      setDeleteTarget(null); setDeleteError("");
      notifyConversationHistoryChanged();
      if (activeId === conversation.id) createNew();
    } catch (error) {
      setDeleteTarget(null);
      setDeleteError(error instanceof Error ? error.message : "删除对话失败");
    }
  };

  const grouped = useMemo(() => {
    const now = new Date();
    const startOfToday = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate()
    ).getTime();
    const sevenDaysAgo = startOfToday - 6 * 24 * 60 * 60 * 1000;

    const today: ConversationSummary[] = [];
    const last7Days: ConversationSummary[] = [];
    const earlier: ConversationSummary[] = [];
    const ungrouped: ConversationSummary[] = [];

    for (const conversation of filtered) {
      const rawTime = conversation.updated_at || conversation.created_at;
      if (!rawTime || typeof rawTime !== "number" || isNaN(rawTime) || rawTime <= 0) {
        ungrouped.push(conversation);
        continue;
      }
      const timeMs = rawTime < 1e11 ? rawTime * 1000 : rawTime;
      if (timeMs >= startOfToday) {
        today.push(conversation);
      } else if (timeMs >= sevenDaysAgo) {
        last7Days.push(conversation);
      } else {
        earlier.push(conversation);
      }
    }

    return { today, last7Days, earlier, ungrouped };
  }, [filtered]);

  const renderRow = (conversation: ConversationSummary) => {
    const family = families.find(item => item.id === conversation.id);
    const active = activeId === conversation.id || !!activeId && !!family?.conversationIds.includes(activeId);
    const displayTitle = deriveTaskDisplayTitle(conversation.title);
    const isCommand = looksLikeCommand(conversation.title ?? "");
    return (
      <div
        key={conversation.id}
        className={`conversation-row ${active ? "conversation-row-active" : ""} group relative flex items-center gap-2 rounded-lg border px-3 py-2.5 text-sm transition-colors ${
          active
            ? "border-[var(--accent)]/30 bg-[var(--accent)]/10 text-[var(--text)]"
            : "border-transparent text-[var(--text-muted)] hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
        }`}
      >
        {active && (
          <span className="absolute inset-y-2 left-0 w-0.5 rounded-r-full bg-[var(--accent)]" />
        )}
        <button
          type="button"
          onClick={() => selectConversation(active && activeId ? activeId : conversation.id)}
          className="min-w-0 flex-1 truncate whitespace-nowrap text-left"
          title={
            isCommand
              ? `命令：${conversation.title}`
              : conversation.title || "新任务"
          }
        >
          <span className="history-row-title">{displayTitle}</span>
          {conversation.origin?.kind==="channel"&&<span className="history-row-source" title={conversation.origin.channel_name}>{originDescription(conversation.origin)}</span>}
        </button>
        {family&&<button type="button" className="history-branch-button" aria-label={`查看${displayTitle}的${family.conversationIds.length}条对话路径`} title={`${family.conversationIds.length} 条路径 · 管理或新建分支`} onClick={()=>onShowBranches?.(family.id)}><GitBranch size={15}/><span>{family.conversationIds.length}</span></button>}
        <button
          type="button"
          onClick={() => { setDeleteError(""); setDeleteTarget(conversation); }}
          className="conversation-row-action rounded p-1 text-[var(--text-faint)] opacity-0 transition-opacity hover:bg-[var(--danger)]/10 hover:text-[var(--danger)] focus:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
          title={family && family.conversationIds.length > 1 ? "删除主线，保留其他分支" : "删除对话"}
          aria-label={family && family.conversationIds.length > 1 ? `删除${displayTitle}的主线` : `删除${displayTitle}`}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>
    );
  };

  const hasGroups =
    grouped.today.length > 0 ||
    grouped.last7Days.length > 0 ||
    grouped.earlier.length > 0;

  return (
    <aside className="glass-rail conversation-sidebar flex h-full min-h-0 min-w-0 flex-col bg-[var(--panel)]">
      <div className="conversation-sidebar-header flex h-14 shrink-0 items-center gap-2 border-b border-[var(--border)] px-3">
        <MessageSquare className="h-4 w-4 text-[var(--accent)]" />
        <h2 className="text-sm font-semibold">任务</h2>
        <span className="conversation-sidebar-motif" aria-hidden="true" />
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="ml-auto rounded-md p-2 text-[var(--text-muted)] transition-colors hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
            aria-label="关闭任务列表"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      <div className="conversation-sidebar-actions space-y-2 border-b border-[var(--border)] p-3">
        <Button
          type="button"
          onClick={createNew}
          variant="secondary"
          className="conversation-create-button h-10 w-full"
          size="md"
        >
          <Plus className="h-4 w-4" />
          新建任务
        </Button>
        <div className="history-source-tabs" role="tablist" aria-label="会话来源">
          <button type="button" role="tab" aria-selected={source==="desktop"} onClick={()=>setSource("desktop")}><Monitor size={13}/><span>桌面</span><small>{entries.filter(x=>x.origin?.kind!=="channel").length}</small></button>
          <button type="button" role="tab" aria-selected={source==="channel"} onClick={()=>setSource("channel")}><Radio size={13}/><span>渠道</span><small>{entries.filter(x=>x.origin?.kind==="channel").length}</small></button>
        </div>
        <label className="relative block">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--text-faint)]" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索任务"
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] py-2 pl-9 pr-3 text-xs text-[var(--text)] placeholder:text-[var(--text-faint)]"
          />
        </label>
      </div>

      <div className="conversation-list scrollbar-thin min-h-0 flex-1 overflow-y-auto p-2">
        {deleteError && <p role="alert">{deleteError}</p>}
        {historyError && <p role="alert">读取对话历史失败：{historyError}</p>}
        {historyLoading ? <p role="status">正在读取对话历史…</p> : filtered.length === 0 && !historyError ? (
          <EmptyState
            title={query ? "没有匹配的对话" : source==="channel"?"暂无渠道对话":"暂无桌面对话"}
            description={query ? "尝试更换搜索词或来源" : source==="channel"?"QQ、微信和飞书消息会记录在这里":"从上方新建一个任务"}
            className="py-10"
          />
        ) : !hasGroups ? (
          <div className="space-y-1">
            {filtered.map(renderRow)}
          </div>
        ) : (
          <div className="space-y-3">
            {grouped.today.length > 0 && (
              <section aria-label="今天">
                <h3 className="conversation-group-header">今天</h3>
                <div className="space-y-1">{grouped.today.map(renderRow)}</div>
              </section>
            )}
            {grouped.last7Days.length > 0 && (
              <section aria-label="近7天">
                <h3 className="conversation-group-header">近7天</h3>
                <div className="space-y-1">{grouped.last7Days.map(renderRow)}</div>
              </section>
            )}
            {grouped.earlier.length > 0 && (
              <section aria-label="更早">
                <h3 className="conversation-group-header">更早</h3>
                <div className="space-y-1">{grouped.earlier.map(renderRow)}</div>
              </section>
            )}
            {grouped.ungrouped.length > 0 && (
              <section aria-label="其他任务">
                <h3 className="conversation-group-header">其他</h3>
                <div className="space-y-1">{grouped.ungrouped.map(renderRow)}</div>
              </section>
            )}
          </div>
        )}
      </div>
      <ConfirmDialog open={!!deleteTarget} title="删除对话" tone="danger" confirmLabel="删除对话" description={deleteTarget && families.find(family => family.id === deleteTarget.id && family.conversationIds.length > 1) ? `将删除“${deriveTaskDisplayTitle(deleteTarget.title)}”这条主线及其消息。其他分支会保留，并按剩余关系显示。` : `将删除“${deriveTaskDisplayTitle(deleteTarget?.title)}”这条对话及其消息。`} onConfirm={() => deleteTarget ? removeConversation(deleteTarget) : undefined} onCancel={() => setDeleteTarget(null)} />
    </aside>
  );
}
