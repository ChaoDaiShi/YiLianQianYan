import { Outlet } from "react-router-dom";
import NavRail from "./NavRail";
import { useTheme } from "../../theme";
import { APP_CONTENT_VIEWPORT_CLASS_NAME } from "./workspaceLayout";

export default function AppShell() {
  const { theme, resolvedScheme } = useTheme();

  return (
    <div
      className="app-shell relative flex h-screen w-screen overflow-hidden text-[var(--text)]"
      data-color-scheme={resolvedScheme}
    >
      <div
        className="shell-ambient shell-ambient-strong pointer-events-none absolute inset-0 z-0"
        aria-hidden="true"
      />
      <div
        className="shell-ambient-overlay pointer-events-none absolute inset-0 z-0"
        aria-hidden="true"
      />
      {/* Background layer */}
      <div
        className="absolute inset-0 -z-10 transition-[background] duration-500"
        style={{
          backgroundColor: "var(--bg-app)",
          backgroundImage: "var(--bg-image)",
          backgroundSize: theme.bgMode === "image" ? "cover" : undefined,
          backgroundPosition: "center",
        }}
      />
      <div
        className="pointer-events-none absolute inset-0 -z-10 transition-opacity duration-300"
        style={{
          backgroundColor: "var(--shell-overlay)",
          opacity: theme.bgMode === "image" ? 1 : 0,
        }}
      />

      <NavRail />
      <main className={`${APP_CONTENT_VIEWPORT_CLASS_NAME} workspace-region relative z-10`}>
        <Outlet />
      </main>
    </div>
  );
}
