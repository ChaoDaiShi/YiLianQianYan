import { Suspense } from "react";
import { Outlet } from "react-router-dom";
import NavRail from "./NavRail";
import { useTheme } from "../../theme";
import { APP_CONTENT_VIEWPORT_CLASS_NAME } from "./workspaceLayout";
import RouteLoadingSurface from "./RouteLoadingSurface";
import RouteErrorBoundary from "./RouteErrorBoundary";
import { useUiNavigationShortcuts } from "./useUiNavigationShortcuts";
import { CommandPalette, CyreneToast } from "../ui";

export default function AppShell() {
  const { theme, resolvedScheme } = useTheme();
  useUiNavigationShortcuts();

  return (
    <div
      className="app-shell relative flex h-screen w-screen overflow-hidden text-[var(--text)]"
      data-color-scheme={resolvedScheme}
    >
      <div
        className="shell-ambient shell-ambient-strong pointer-events-none absolute inset-0 z-0"
        style={{ display: theme.bgMode === "image" ? "none" : undefined }}
        aria-hidden="true"
      />
      <div
        className="shell-ambient-overlay pointer-events-none absolute inset-0 z-0"
        style={{ display: theme.bgMode === "image" ? "none" : undefined }}
        aria-hidden="true"
      />
      {/* Background layer */}
      <div
        className="absolute inset-0 transition-[background] duration-[var(--motion-normal)]"
        style={{
          backgroundColor: "var(--bg-app)",
          backgroundImage: "var(--bg-image)",
          backgroundSize: theme.bgMode === "image" ? "cover" : undefined,
          backgroundPosition: "center",
          zIndex: theme.bgMode === "image" ? 0 : -10,
        }}
      />
      <div
        className="pointer-events-none absolute inset-0 transition-opacity duration-[var(--motion-normal)]"
        style={{
          backgroundColor: "var(--shell-overlay)",
          background: resolvedScheme === "light"
            ? "var(--shell-readability-gradient)"
            : "var(--shell-overlay)",
          opacity: theme.bgMode === "image" ? 1 : 0,
          zIndex: theme.bgMode === "image" ? 1 : -10,
        }}
      />

      <NavRail />
      <CommandPalette />
      <CyreneToast />
      <main className={`${APP_CONTENT_VIEWPORT_CLASS_NAME} workspace-region relative z-10`}>
        <Suspense fallback={<RouteLoadingSurface />}>
          <RouteErrorBoundary>
            <Outlet />
          </RouteErrorBoundary>
        </Suspense>
      </main>
    </div>
  );
}
