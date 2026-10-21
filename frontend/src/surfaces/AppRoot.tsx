import WorkspaceSurface from "./workspace/WorkspaceSurface";
import DesktopSurfaceSkeleton from "./desktop/DesktopSurfaceSkeleton";
import { resolveSurfaceHostMode } from "./surfaceHost";
import { useLocation } from "react-router-dom";
import CallOverlay from "../features/voice/CallOverlay";
import MonitorOverlay from "../features/monitor/MonitorOverlay";
import { MonitorDataHost } from "../features/monitor/TokenUsagePanel";

export default function AppRoot() {
  const { pathname } = useLocation();
  if (pathname === "/floating/call") return <CallOverlay />;
  if (pathname === "/floating/monitor") return <MonitorOverlay />;
  const mode = resolveSurfaceHostMode(import.meta.env.VITE_SURFACE_HOST as string | undefined);
  return <><MonitorDataHost />{mode === "desktop-skeleton" ? <DesktopSurfaceSkeleton /> : <WorkspaceSurface />}</>;
}
