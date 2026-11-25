import { Suspense } from "react";
import { Outlet } from "react-router-dom";
import NavRail from "./NavRail";
import { useTheme } from "../../theme";
import { APP_CONTENT_VIEWPORT_CLASS_NAME } from "./workspaceLayout";
import RouteLoadingSurface from "./RouteLoadingSurface";
import RouteErrorBoundary from "./RouteErrorBoundary";
import { useUiNavigationShortcuts } from "./useUiNavigationShortcuts";
import { CommandPalette, CyreneToast } from "../ui";
import FirstRunSetup from "../../features/setup/FirstRunSetup";

export default function AppShell() {
  const { theme, resolvedScheme } = useTheme();
  useUiNavigationShortcuts();

  return (
    <div
      className="app-shell relative flex h-screen w-screen overflow-hidden text-[var(--text)]"
      data-color-scheme={resolvedScheme}
      data-background-mode={theme.bgMode}
    >
      <NavRail />
      <CommandPalette />
      <CyreneToast />
      <main className={`${APP_CONTENT_VIEWPORT_CLASS_NAME} workspace-region relative z-10`}>
        <Suspense fallback={<RouteLoadingSurface />}>
          <RouteErrorBoundary>
            <FirstRunSetup><Outlet /></FirstRunSetup>
          </RouteErrorBoundary>
        </Suspense>
      </main>
    </div>
  );
}
