import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { subscribeToEvents, type ProductEvent } from "../../api/events";
import { showToast } from "../../components/ui/CyreneToast";
import { acceptNotice, emptyNoticeState, type NoticeState } from "./assistantNoticePolicy";

const messages: Record<string, string> = {
  completed: "个人助手已保存本轮报告。",
  waiting_input: "个人助手需要你补充信息，已暂停等待。",
  waiting_approval: "个人助手有操作需要你确认，尚未执行。",
  failed: "个人助手本轮未完成，请检查原因后再继续。",
  needs_review: "个人助手有执行结果需要核实，已停止自动跟进。",
};
const storageKey = "yilian.personal.notices.v1";
function restore(): NoticeState {
  try {
    const value = JSON.parse(sessionStorage.getItem(storageKey) || "null");
    if (value && Array.isArray(value.seen) && value.seen.every((v: unknown) => typeof v === "string") && value.completed && typeof value.completed === "object") {
      return { seen: value.seen.slice(-256), completed: Object.fromEntries(Object.entries(value.completed).filter(([, v]) => typeof v === "number").slice(-256)) as Record<string, number> };
    }
  } catch { /* Storage can be unavailable; task execution is unaffected. */ }
  return emptyNoticeState();
}

/** Observe committed outcomes only. Notifications never authorize execution. */
export default function PersonalAssistantNoticeHost() {
  const navigate = useNavigate();
  useEffect(() => {
    const state = restore();
    const pending = new Map<string, ProductEvent>();
    const announce = (event: ProductEvent) => {
      const { conversation_id, status, automation_id, occurrence_id } = event.payload;
      if (typeof conversation_id !== "string" || typeof status !== "string" || !messages[status]) return;
      if (typeof automation_id !== "string" || typeof occurrence_id !== "string") return;
      const accepted = acceptNotice(state, automation_id, occurrence_id, status, conversation_id, window.location.pathname, Date.now());
      try { sessionStorage.setItem(storageKey, JSON.stringify(state)); } catch { /* Best effort UI state. */ }
      if (!accepted) return;
      showToast(messages[status], status === "completed" ? "success" : "info", undefined, {
        label: "查看任务对话", onClick: () => navigate(`/chat/${encodeURIComponent(conversation_id)}`),
      });
    };
    const connection = subscribeToEvents(event => {
      if (event.type !== "automation.personal.updated") return;
      const { automation_id, occurrence_id, status } = event.payload;
      if (typeof automation_id !== "string" || typeof occurrence_id !== "string" || typeof status !== "string") return;
      if (!messages[status]) { pending.delete(automation_id); return; }
      if (document.visibilityState === "visible") announce(event); else pending.set(automation_id, event);
    });
    const visible = () => {
      if (document.visibilityState !== "visible") return;
      for (const event of pending.values()) announce(event);
      pending.clear();
    };
    document.addEventListener("visibilitychange", visible);
    return () => { connection.abort(); document.removeEventListener("visibilitychange", visible); };
  }, [navigate]);
  return null;
}
