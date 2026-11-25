export function isExternalHttpUrl(href: string): boolean {
  try {
    const url = new URL(href);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/** Default website route is the app's own browser; failures never fall back externally. */
export async function openExternalUrl(href: string): Promise<void> {
  if (!isExternalHttpUrl(href)) return;
  window.dispatchEvent(new CustomEvent("yilian:open-browser", { detail: { url: href } }));
}

/** Explicit user choice, retained for sites requiring a system browser. */
export async function openInSystemBrowser(href: string): Promise<void> {
  if (!isExternalHttpUrl(href)) return;

  if (!("__TAURI_INTERNALS__" in window)) {
    // Explicit system-browser choice in the web preview uses a normal web tab.
    window.open(href, "_blank", "noopener,noreferrer");
    return;
  }
  const { open } = await import("@tauri-apps/plugin-shell");
  // A desktop refusal or uncertain launch must surface, never try another path.
  await open(href);
}
