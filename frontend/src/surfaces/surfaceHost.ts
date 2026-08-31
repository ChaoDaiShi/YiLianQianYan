export type SurfaceHostMode = "standalone" | "desktop-skeleton";

export function resolveSurfaceHostMode(value?: string): SurfaceHostMode {
  return value === "desktop-skeleton" ? "desktop-skeleton" : "standalone";
}

export function surfaceContainsWorkspace(_mode: SurfaceHostMode): true {
  return true;
}
