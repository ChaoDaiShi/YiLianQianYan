export function isExternalHttpUrl(href: string): boolean {
  try {
    const url = new URL(href);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export async function openExternalUrl(href: string): Promise<void> {
  if (!isExternalHttpUrl(href)) return;

  try {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(href);
  } catch {
    // Browser preview and non-Tauri environments use the normal external tab.
    window.open(href, "_blank", "noopener,noreferrer");
  }
}
