import { useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { listConversations, deleteConversation } from "../../api/client";
import { Button, EmptyState } from "../ui";

interface Props {
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
}

export default function ConversationSidebar({ activeId, onSelect, onNew }: Props) {
  const [convs, setConvs] = useState<any[]>([]);

  useEffect(() => {
    listConversations().then((c) => {
      if (c) setConvs(c);
    });
    const t = setInterval(
      () =>
        listConversations().then((c) => {
          if (c) setConvs(c);
        }),
      5000
    );
    return () => clearInterval(t);
  }, []);

  return (
    <div className="flex flex-col h-full">
      <div className="p-3">
        <Button onClick={onNew} className="w-full" size="md">
          <Plus className="w-4 h-4" />
          新对话
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 scrollbar-thin">
        {convs.length === 0 ? (
          <EmptyState title="暂无对话" description="点击上方开始新对话" className="py-10" />
        ) : (
          convs.map((c) => (
            <div
              key={c.id}
              onClick={() => onSelect(c.id)}
              className={`group flex items-center gap-2 px-3 py-2.5 rounded-lg cursor-pointer text-sm mb-0.5 transition-colors ${
                activeId === c.id
                  ? "bg-[var(--accent)]/15 text-[var(--accent)]"
                  : "hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
              }`}
            >
              <span className="truncate flex-1">{c.title || "新对话"}</span>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  deleteConversation(c.id);
                  setConvs((p) => p.filter((x) => x.id !== c.id));
                }}
                className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-500/20 text-[var(--text-faint)] hover:text-[var(--danger)]"
                title="删除"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
