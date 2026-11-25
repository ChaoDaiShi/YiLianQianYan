export function normalizeBrowserAddress(raw: string): string {
 const url = new URL(raw.trim());
 if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password || /[\u0000-\u001f\u007f]/.test(raw)) throw new Error("请输入不含账号密码的 HTTP 或 HTTPS 网址");
 return url.href;
}
export function browserOrigin(raw: string): string { return new URL(normalizeBrowserAddress(raw)).origin; }
export function siteIdentityLabel(project?: string): string { return project ? `${project} · 服务归属未核实` : "未关联项目 · 服务归属未核实"; }

/** Shared by the omnibox and start page; saved sites still use strict URL input. */
export function resolveBrowserInput(raw: string): string {
 const input = raw.trim();
 if (!input || /[\u0000-\u001f\u007f]/.test(raw)) throw new Error("请输入网址或搜索关键词，不要包含控制字符");
 if (/^https?:/i.test(input)) return normalizeBrowserAddress(input);
 const hostPort = /^(?:[\w.-]+|\[[\da-f:]+\]):\d+(?:[/?#]|$)/i.test(input);
 if (!hostPort && /^[a-z][a-z\d+.-]*:/i.test(input)) throw new Error("仅支持 HTTP 或 HTTPS 网址");
 if (!/\s/.test(input)) {
  const host = input.split(/[/?#]/, 1)[0];
  // Only authority userinfo denotes credentials; paths/queries/fragments may contain @.
  if (host.includes("@")) throw new Error("请输入不含账号密码的网址");
  if (hostPort || host.includes(".") || host === "localhost" || host.startsWith("[")) {
   const local = hostPort || /^localhost(?:[.:]|$)/i.test(host) || /\.localhost$/i.test(host) || /^127\./.test(host) || host.startsWith("[");
   return normalizeBrowserAddress(`${local ? "http" : "https"}://${input}`);
  }
 }
 const search = new URL("https://www.bing.com/search");
 search.searchParams.set("q", input);
 return search.href;
}
