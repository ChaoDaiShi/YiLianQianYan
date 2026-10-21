import { useEffect, useMemo, useState } from "react";
import { MessageSquare, Plus, Search, Trash2, X } from "lucide-react";
import { deleteConversation, listConversations } from "../../api/client";
import type { ConversationSummary } from "../../types";
import { Button, EmptyState } from "../ui";
import { deriveTaskDisplayTitle, looksLikeCommand } from "./taskTitle";
import type { ConversationFamily } from "../../features/connectors/messages/messageForest";

interface Props {
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onClose?: () => void;
  families?: ConversationFamily[];
}

export default function ConversationSidebar({
  activeId,
  onSelect,
  onNew,
  onClose,
  families = [],
}: Props) {
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let mounted = true;
    const refresh = () => {
      listConversations()
        .then((items) => {
          if (mounted && items) setConversations(items);
        })
        .catch((error) => {
          console.error("刷新任务列表失败:", error);
        });
    };
    refresh();
    const timer = window.setInterval(refresh, 5000);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, []);

  const filtered = useMemo(() => {
    const roots = new Map(families.flatMap(family => family.conversationIds.map(id => [id, family.id] as const)));
    const independent = conversations.filter(conversation => !roots.has(conversation.id) || roots.get(conversation.id) === conversation.id).map(conversation => {
      const family = families.find(item => item.id === conversation.id);
      const members = family ? conversations.filter(item => family.conversationIds.includes(item.id)) : [conversation];
      return { ...conversation, updated_at: Math.max(...members.map(item => item.updated_at || item.created_at)) };
    }).sort((a, b) => b.updated_at - a.updated_at);
    const keyword = query.trim().toLocaleLowerCase();
    if (!keyword) return independent;
    return independent.filter((conversation) =>
      (conversation.title || "新任务").toLocaleLowerCase().includes(keyword)
    );
  }, [conversations, query, families]);

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
      setConversations((current) =>
        current.filter((item) => item.id !== conversation.id)
      );
      if (activeId === conversation.id) createNew();
    } catch (error) {
      console.error("删除任务失败:", error);
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
          {displayTitle}
        </button>
        <button
          type="button"
          onClick={() => void removeConversation(conversation)}
          className="conversation-row-action rounded p-1 text-[var(--text-faint)] opacity-0 transition-opacity hover:bg-[var(--danger)]/10 hover:text-[var(--danger)] focus:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
          title="删除任务"
          aria-label={`删除${displayTitle}`}
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
        {filtered.length === 0 ? (
          <EmptyState
            title={query ? "没有匹配的任务" : "暂无任务"}
            description={query ? "尝试更换搜索词" : "从上方新建一个任务"}
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
    </aside>
  );
}
