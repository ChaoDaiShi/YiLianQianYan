import { Outlet } from "react-router-dom";
import NavRail from "./NavRail";
import { useTheme } from "../../theme";
import { APP_CONTENT_VIEWPORT_CLASS_NAME } from "./workspaceLayout";

export default function AppShell() {
  const { theme } = useTheme();

  return (
    <div className="app-shell relative flex h-screen w-screen overflow-hidden text-[var(--text)]">
      <div
        className="shell-ambient pointer-events-none absolute inset-0 z-0"
        aria-hidden="true"
      />
      <div
        className="shell-ambient-overlay pointer-events-none absolute inset-0 z-0"
        aria-hidden="true"
      />
      {/* Background layer */}
      <div
        className="absolute inset-0 -z-10 transition-[filter,background] duration-500"
        style={{
          backgroundColor: "var(--bg)",
          backgroundImage: "var(--bg-image)",
          backgroundSize: theme.bgMode === "image" ? "cover" : undefined,
          backgroundPosition: "center",
          filter: `blur(var(--bg-blur)) brightness(var(--bg-brightness))`,
        }}
      />
      <div
        className="pointer-events-none absolute inset-0 -z-10 transition-opacity duration-300"
        style={{
          backgroundColor: "var(--backdrop)",
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
