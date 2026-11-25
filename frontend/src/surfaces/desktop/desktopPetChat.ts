import { useEffect } from "react";
import { chatRuntime } from "../../features/execution/chatRuntime";
import type { ConversationActivity } from "../../features/execution/conversationRuntime";
import { useNavigate } from "react-router-dom";
import { subscribeToEvents, type ProductEvent } from "../../api/events";
import { managementRequest } from "../../api/productManagement";

const CHANNEL = "yilian-desktop-pet-chat";
type TaskNotice = { label: string };
type ChatMessage = ConversationActivity | "sync" | "sync-request" |
  { kind: "attach" | "detach"; id: string } |
  { kind: "task"; notice: TaskNotice } | {kind:"pressure"; lowMemory:boolean};

function taskNotice(event: ProductEvent): TaskNotice | undefined {
  if (event.type === "task.execution.waiting_approval") return { label: "工作流等待确认" };
  if (event.type === "task.execution.succeeded") return { label: "工作流步骤完成" };
  if (event.type === "task.execution.failed") return { label: "工作流步骤未完成" };
  if (event.type !== "automation.personal.updated") return;
  if (event.payload.status === "completed") return { label: "个人助手任务完成" };
  if (event.payload.status === "failed") return { label: "个人助手任务未完成" };
  if (["waiting_input", "waiting_approval"].includes(String(event.payload.status))) {
    return { label: "个人助手等待确认" };
  }
}

export function useDesktopPetChatBridge(mainSurface: boolean) {
  const navigate = useNavigate();
  useEffect(() => {
    if (!mainSurface || !window.__TAURI_INTERNALS__) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen("desktop-pet:open-chat", () => {
        if (active && !/^\/chat(?:\/|$)/.test(window.location.pathname)) navigate("/chat");
      }))
      .then(dispose => { if (active) unlisten = dispose; else dispose(); })
      .catch(() => undefined);
    return () => { active = false; unlisten?.(); };
  }, [mainSurface, navigate]);
  useEffect(() => {
    if (!mainSurface || !window.__TAURI_INTERNALS__) return;
    const channel = new BroadcastChannel(CHANNEL);
    const unsubscribe = chatRuntime.subscribeActivity(activity => channel.postMessage(activity));
    const subscribers = new Set<string>();
    let events: AbortController | undefined;
    let active = true;
    let unlisten: (() => void) | undefined;
    const stopEvents = () => { events?.abort(); events = undefined; };
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen<boolean>("desktop-pet:open-changed", event => {
        if (!event.payload) { subscribers.clear(); stopEvents(); }
      }))
      .then(dispose => { if (active) unlisten = dispose; else dispose(); })
      .catch(() => undefined);
    channel.onmessage = ({ data }: MessageEvent<ChatMessage>) => {
      if (typeof data === "object" && data.kind === "attach") {
        subscribers.add(data.id);
        events ??= subscribeToEvents(event => {
          if (event.type === "resource.pressure.changed") {
            channel.postMessage({kind:"pressure",lowMemory:event.payload.low_memory===true} satisfies ChatMessage);
          }
          const notice = taskNotice(event);
          if (notice) channel.postMessage({ kind: "task", notice } satisfies ChatMessage);
        },undefined,state=>{
          if(state!=="connected")return;
          void managementRequest<{low_memory:boolean}>("GET","/api/v1/resources/policy").then(policy=>{
            if(active && subscribers.size) channel.postMessage({kind:"pressure",lowMemory:policy.low_memory} satisfies ChatMessage);
          }).catch(()=>undefined);
        });
        return;
      }
      if (typeof data === "object" && data.kind === "detach") {
        subscribers.delete(data.id);
        if (!subscribers.size) stopEvents();
        return;
      }
      if (data !== "sync") return;
      const activity = chatRuntime.currentActivity();
      if (activity === "thinking" || activity === "speaking" || activity === "waiting") {
        channel.postMessage(activity);
      }
    };
    channel.postMessage("sync-request");
    return () => { active = false; unlisten?.(); unsubscribe(); stopEvents(); channel.close(); };
  }, [mainSurface]);
}

export function useDesktopPetChatActivity(onActivity: (activity: ConversationActivity) => void, onTask?: (notice: TaskNotice) => void, onPressure?: (low:boolean)=>void) {
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    const channel = new BroadcastChannel(CHANNEL);
    const id = crypto.randomUUID();
    const attach = () => {
      if (onTask) channel.postMessage({ kind: "attach", id } satisfies ChatMessage);
      channel.postMessage("sync");
    };
    const detach = () => { channel.postMessage({ kind: "detach", id } satisfies ChatMessage); };
    channel.onmessage = ({ data }: MessageEvent<ChatMessage>) => {
      if (data === "sync-request") attach();
      else if (typeof data === "string" && data !== "sync") onActivity(data);
      else if (typeof data === "object" && data.kind === "task") onTask?.(data.notice);
      else if (typeof data === "object" && data.kind === "pressure") onPressure?.(data.lowMemory);
    };
    if (onTask) window.addEventListener("pagehide", detach);
    attach();
    return () => {
      if (onTask) { window.removeEventListener("pagehide", detach); detach(); }
      channel.close();
    };
  }, [onActivity, onTask, onPressure]);
}
