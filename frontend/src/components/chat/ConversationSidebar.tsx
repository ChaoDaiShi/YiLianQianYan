import { useEffect, useMemo, useState } from "react";
import { MessageSquare, Plus, Search, Trash2, X } from "lucide-react";
import { deleteConversation, listConversations } from "../../api/client";
import type { ConversationSummary } from "../../types";
import { Button, EmptyState } from "../ui";

interface Props {
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onClose?: () => void;
}

export default function ConversationSidebar({
  activeId,
  onSelect,
  onNew,
  onClose,
}: Props) {
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let mounted = true;
    const refresh = () => {
      listConversations().then((items) => {
        if (mounted && items) setConversations(items as ConversationSummary[]);
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
    const keyword = query.trim().toLocaleLowerCase();
    if (!keyword) return conversations;
    return conversations.filter((conversation) =>
      (conversation.title || "新任务").toLocaleLowerCase().includes(keyword)
    );
  }, [conversations, query]);

  const selectConversation = (id: string) => {
    onSelect(id);
    onClose?.();
  };

  const createNew = () => {
    onNew();
    onClose?.();
  };

  const removeConversation = async (conversation: ConversationSummary) => {
    const deleted = await deleteConversation(conversation.id);
    if (!deleted) return;
    setConversations((current) =>
      current.filter((item) => item.id !== conversation.id)
    );
    if (activeId === conversation.id) createNew();
  };

  return (
    <aside className="flex h-full min-h-0 flex-col bg-[var(--panel)]">
      <div className="flex h-14 shrink-0 items-center gap-2 border-b border-[var(--border)] px-3">
        <MessageSquare className="h-4 w-4 text-[var(--accent)]" />
        <h2 className="text-sm font-semibold">任务</h2>
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

      <div className="space-y-2 border-b border-[var(--border)] p-3">
        <Button type="button" onClick={createNew} className="w-full" size="md">
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

      <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto p-2">
        {filtered.length === 0 ? (
          <EmptyState
            title={query ? "没有匹配的任务" : "暂无任务"}
            description={query ? "尝试更换搜索词" : "从上方新建一个任务"}
            className="py-10"
          />
        ) : (
          <div className="space-y-1">
            {filtered.map((conversation) => {
              const active = activeId === conversation.id;
              return (
                <div
                  key={conversation.id}
                  className={`group relative flex items-center gap-2 rounded-lg border px-3 py-2.5 text-sm transition-colors ${
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
                    onClick={() => selectConversation(conversation.id)}
                    className="min-w-0 flex-1 truncate text-left"
                    title={conversation.title || "新任务"}
                  >
                    {conversation.title || "新任务"}
                  </button>
                  <button
                    type="button"
                    onClick={() => void removeConversation(conversation)}
                    className="rounded p-1 text-[var(--text-faint)] opacity-0 transition-opacity hover:bg-[var(--danger)]/10 hover:text-[var(--danger)] focus:opacity-100 group-hover:opacity-100"
                    title="删除任务"
                    aria-label={`删除${conversation.title || "任务"}`}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </aside>
  );
}
