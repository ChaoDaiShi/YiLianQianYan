import { useEffect, useRef } from "react";
import { RefreshGate } from "./refreshGate";
export function useVisibleRefresh(refresh: () => Promise<void>) {
  const latest = useRef(refresh); latest.current = refresh;
  useEffect(() => {
    const gate = new RefreshGate();
    const tick = () => { void gate.run(Date.now(), document.visibilityState === "visible", () => latest.current()).catch(() => undefined); };
    tick();
    const timer = window.setInterval(tick, 30_000);
    document.addEventListener("visibilitychange", tick);
    return () => { window.clearInterval(timer); document.removeEventListener("visibilitychange", tick); };
  }, []);
}
