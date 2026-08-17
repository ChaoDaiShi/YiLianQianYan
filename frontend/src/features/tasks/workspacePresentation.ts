export type ArtifactDisplayKind =
  | "folder"
  | "code"
  | "markdown"
  | "json"
  | "config"
  | "image"
  | "file";

export function formatWorkspacePath(path: string | null | undefined): string | null {
  const normalized = path?.trim();
  return normalized || null;
}

export function classifyArtifactType(name: string): ArtifactDisplayKind {
  const extension = name.split(".").pop()?.toLocaleLowerCase() || "";
  if (["md", "markdown", "mdx"].includes(extension)) return "markdown";
  if (["json", "jsonc"].includes(extension)) return "json";
  if (["ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "css", "html", "vue"].includes(extension)) {
    return "code";
  }
  if (["toml", "yaml", "yml", "ini", "env", "xml"].includes(extension)) return "config";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(extension)) return "image";
  return "file";
}
