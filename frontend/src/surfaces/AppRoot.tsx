import WorkspaceSurface from "./workspace/WorkspaceSurface";
import DesktopSurfaceSkeleton from "./desktop/DesktopSurfaceSkeleton";
import { resolveSurfaceHostMode } from "./surfaceHost";

export default function AppRoot() {
  const mode = resolveSurfaceHostMode(import.meta.env.VITE_SURFACE_HOST as string | undefined);
  return mode === "desktop-skeleton" ? <DesktopSurfaceSkeleton /> : <WorkspaceSurface />;
}
