import type { ReactNode } from "react";
import { Link, Navigate, useLocation } from "react-router-dom";
import RouteLoadingSurface from "../../components/layout/RouteLoadingSurface";
import { useProductPreferences } from "../../components/system/useProductPreferences";
import "./firstRunSetup.css";

/** First-run routing belongs to the active shell, independently of the navigation editor. */
export default function FirstRunSetup({ children }: { children: ReactNode }) {
  const { preferences, loaded, error } = useProductPreferences();
  const { pathname } = useLocation();
  if (!loaded) return error
    ? <div className="first-run-load-error" role="alert"><p>无法读取首次配置状态，请确认本地服务可用后重新加载。</p><button type="button" onClick={() => window.location.reload()}>重新加载</button></div>
    : <RouteLoadingSurface />;
  const deferred = window.sessionStorage.getItem("yilian.setup.deferred") === "true";
  if (!preferences.setup_completed && !deferred && pathname !== "/setup") return <Navigate to="/setup" replace />;
  return <div className="first-run-route">
    {!preferences.setup_completed && deferred && pathname !== "/setup" && <aside className="first-run-reminder" aria-label="模型配置提醒"><span>模型快速配置尚未完成，连接模型后即可开始与小涟协作。</span><Link to="/setup">去配置模型</Link></aside>}
    {children}
  </div>;
}
