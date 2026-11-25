import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowUpRight, X } from "lucide-react";
import type { ConversationActivity } from "../../features/execution/conversationRuntime";
import { useDesktopPetChatActivity } from "./desktopPetChat";

const status: Record<ConversationActivity, string> = {
  idle: "等待新任务",
  thinking: "正在处理",
  speaking: "正在回复",
  waiting: "等待确认",
  succeeded: "任务完成",
  failed: "任务未完成",
};

export default function DesktopWidget() {
  const [now, setNow] = useState(() => new Date());
  const [activity, setActivity] = useState<ConversationActivity>("idle");
  const [error, setError] = useState("");
  useDesktopPetChatActivity(setActivity);
  useEffect(() => {
    const tick = window.setInterval(() => setNow(new Date()), 1_000);
    return () => window.clearInterval(tick);
  }, []);
  return <main className="grid h-screen grid-cols-[1fr_auto] items-start gap-2 bg-[var(--surface-solid)] p-3 text-[var(--text-primary)]">
    <div className="min-w-0">
      <time className="text-2xl font-semibold tabular-nums">{now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time>
      <p className="truncate text-sm text-[var(--text-muted)]" role="status">{error || status[activity]}</p>
    </div>
    <div className="flex gap-1">
      <button className="p-1" type="button" title="打开主窗口" aria-label="打开主窗口"
        onClick={() => void invoke("show_main_from_desktop_pet").catch(() => setError("无法打开主窗口"))}><ArrowUpRight size={18} /></button>
      <button className="p-1" type="button" title="关闭桌面组件" aria-label="关闭桌面组件"
        onClick={() => void getCurrentWindow().close().catch(() => setError("无法关闭桌面组件"))}><X size={18} /></button>
    </div>
  </main>;
}
