import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { listenForBrowserOpened, listenForBrowserNewTab, nativeBrowserAvailable } from "./desktop/browserBridge";
import BrowserPage from "../features/browser/BrowserPage";
import WorkspaceSurface from "./workspace/WorkspaceSurface";
import DesktopSurfaceSkeleton from "./desktop/DesktopSurfaceSkeleton";
import DesktopPet from "./desktop/DesktopPet";
import DesktopWidget from "./desktop/DesktopWidget";
import { useDesktopPetChatBridge } from "./desktop/desktopPetChat";
import { resolveSurfaceHostMode } from "./surfaceHost";
import { useLocation } from "react-router-dom";
import CallOverlay from "../features/voice/CallOverlay";
import MonitorOverlay from "../features/monitor/MonitorOverlay";
import { MonitorDataHost } from "../features/monitor/TokenUsagePanel";

// Exported so the browser fixture exercises the actual global route listeners.
export function useBrowserNavigationEvents() {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  useEffect(() => { const open = (event: Event) => { const url = (event as CustomEvent<{url:string}>).detail?.url; if (url) navigate(`/browser?url=${encodeURIComponent(url)}`); }; window.addEventListener("yilian:open-browser", open); return () => window.removeEventListener("yilian:open-browser", open); }, [navigate]);
  useEffect(() => {
    if (!nativeBrowserAvailable() || pathname === "/browser/window") return;
    let active = true;
    const stops: Array<()=>void> = [];
    const register=(promise:Promise<()=>void>)=>void promise.then(unlisten=>{if(active)stops.push(unlisten);else unlisten();}).catch(()=>{});
    register(listenForBrowserOpened(tabId => { if (active) navigate(`/browser?tab=${encodeURIComponent(tabId)}`); }));
    register(listenForBrowserNewTab(()=>{if(active)navigate(`/browser?new=${crypto.randomUUID()}`);}));
    return () => { active = false; stops.forEach(stop=>stop()); };
  }, [navigate, pathname === "/browser/window"]);
}

export default function AppRoot() {
  const { pathname } = useLocation();
  if (pathname === "/floating/desktop-widget") return <DesktopWidget />;
  return pathname === "/floating/pet" ? <DesktopPet /> : <AppSurface />;
}

function AppSurface() {
  const { pathname } = useLocation();
  useBrowserNavigationEvents();
  useDesktopPetChatBridge(pathname !== "/browser/window" && !pathname.startsWith("/floating/"));
  if (pathname === "/browser/window") return <BrowserPage detached />;
  if (pathname === "/floating/call") return <CallOverlay />;
  if (pathname === "/floating/monitor") return <MonitorOverlay />;
  const mode = resolveSurfaceHostMode(import.meta.env.VITE_SURFACE_HOST as string | undefined);
  return <><MonitorDataHost />{mode === "desktop-skeleton" ? <DesktopSurfaceSkeleton /> : <WorkspaceSurface />}</>;
}
