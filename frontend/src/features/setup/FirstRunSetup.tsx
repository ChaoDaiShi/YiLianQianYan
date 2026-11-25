import { useEffect, useState, type ReactNode } from "react";
import { Link, Navigate, useLocation } from "react-router-dom";
import RouteLoadingSurface from "../../components/layout/RouteLoadingSurface";
import { useProductPreferences } from "../../components/system/useProductPreferences";
import { getChatModelReadiness } from "../../api/providerConnection";
import type { ProviderReadinessProjection } from "../../types";
import "./firstRunSetup.css";

/** First-run routing belongs to the active shell, independently of the navigation editor. */
export default function FirstRunSetup({ children }: { children: ReactNode }) {
  const { preferences, loaded, error } = useProductPreferences();
  const { pathname } = useLocation();
  const [readiness, setReadiness] = useState<ProviderReadinessProjection["model"] | null>();
  useEffect(() => {
    if (!loaded || preferences.setup_completed) return;
    let active = true;
    const refresh = () => { void getChatModelReadiness(null).then(value => { if (active) setReadiness(value); }); };
    refresh();
    window.addEventListener("focus", refresh);
    return () => { active = false; window.removeEventListener("focus", refresh); };
  }, [loaded, preferences.setup_completed]);
  if (!loaded) return error
    ? <div className="first-run-load-error" role="alert"><p>无法读取首次配置状态，请确认本地服务可用后重新加载。</p><button type="button" onClick={() => window.location.reload()}>重新加载</button></div>
    : <RouteLoadingSurface />;
  const deferred = window.sessionStorage.getItem("yilian.setup.deferred") === "true";
  const needsSetup = !preferences.setup_completed && readiness !== undefined && readiness !== null && !readiness.available;
  if (!preferences.setup_completed && readiness === undefined && pathname !== "/setup") return <RouteLoadingSurface />;
  if (needsSetup && !deferred && pathname !== "/setup") return <Navigate to="/setup" replace />;
  return <div className="first-run-route">
    {!preferences.setup_completed && readiness === null && pathname !== "/setup" && pathname !== "/chat" && !pathname.startsWith("/chat/") && <aside className="first-run-reminder" aria-label="模型状态提醒"><span>暂时无法确认聊天模型状态，请检查本地服务后重试。</span><Link to="/setup">查看模型配置</Link></aside>}
    {children}
  </div>;
}
