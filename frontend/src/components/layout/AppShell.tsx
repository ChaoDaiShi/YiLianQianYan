import { Outlet } from "react-router-dom";
import NavRail from "./NavRail";
import { useTheme } from "../../theme";

export default function AppShell() {
  const { theme } = useTheme();

  return (
    <div className="relative flex h-screen w-screen overflow-hidden text-[var(--text)]">
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
      <main className="flex-1 flex flex-col min-w-0 relative z-0 animate-page-in">
        <Outlet />
      </main>
    </div>
  );
}
