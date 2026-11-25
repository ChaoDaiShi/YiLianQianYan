import { useEffect } from "react";
import { subscribeToEvents } from "../../api/events";
import { showToast, toastManager } from "../../components/ui/CyreneToast";

/** Deliver software alerts even when the workbench route is closed. */
export default function SoftwareMonitorNotices() {
  useEffect(() => {
    const pending = new Map<string, string>();
    const shown = new Map<string, string>();
    const announce = (id: string, message: string) => {
      const previous = shown.get(id);
      if (previous) toastManager.dismiss(previous);
      shown.set(id, showToast(message, "info"));
    };
    const subscription = subscribeToEvents(event => {
      if (event.type !== "software.monitor.alert" && event.type !== "software.monitor.explained") return;
      const { registration_id: id, software_name: name, alert } = event.payload;
      if (typeof id !== "string" || typeof name !== "string" || !alert || typeof alert !== "object") return;
      const { summary, explanation } = alert as { summary?: unknown; explanation?: unknown };
      if (typeof summary !== "string") return;
      const hasExplanation = typeof explanation === "string" && explanation.length > 0;
      if (event.type === "software.monitor.explained" && !hasExplanation) return;
      const message = `${name} · ${summary}${hasExplanation ? `。AI 解释：${explanation}` : ""}`;
      if (document.hidden) pending.set(id, message);
      else announce(id, message);
    });
    const onVisible = () => {
      if (document.hidden) return;
      for (const [id, message] of pending) announce(id, message);
      pending.clear();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      subscription.abort();
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);
  return null;
}
