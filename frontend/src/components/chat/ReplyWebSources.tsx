import { useState } from "react";
import type { ProcessToolCall } from "../../types";
import { Button, Modal } from "../ui";
import { openExternalUrl } from "./externalLink";

export interface WebSource {
  url: string;
  domain: string;
  title: string;
  excerpt?: string;
}

export function replyWebSources(tools: readonly ProcessToolCall[]): Map<string, WebSource> {
  const sources = new Map<string, WebSource>();
  for (const tool of tools) {
    if (tool.name !== "ai_search" || !tool.result || tool.execution?.status !== "success") continue;
    try {
      const data = JSON.parse(tool.result);
      if (data.kind !== "web_search_sources" || !Array.isArray(data.sources)) continue;
      for (const item of data.sources.slice(0, 10)) {
        if (typeof item?.url !== "string" || typeof item.title !== "string") continue;
        const url = new URL(item.url);
        if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) continue;
        sources.set(url.href, { url: url.href, domain: url.hostname, title: item.title, excerpt: typeof item.excerpt === "string" ? item.excerpt : undefined });
      }
    } catch { /* Historic tool output is plain text. */ }
  }
  return sources;
}

export function ReplyWebSource({ source }: { source: WebSource }) {
  const [open, setOpen] = useState(false);
  return <>
    <button type="button" onClick={() => setOpen(true)} className="ml-1 rounded-md border border-[var(--border-soft)] px-1.5 py-0.5 align-baseline text-xs text-[var(--text-muted)]" aria-label={`预览来源 ${source.domain}`}>来源</button>
    {open && <Modal open onClose={() => setOpen(false)} title={source.title || source.domain}>
      <div className="space-y-3 break-words text-sm">
        <p className="text-[var(--text-muted)]">{source.domain} · 搜索结果，尚未核验网页正文</p>
        {source.excerpt && <p>{source.excerpt}</p>}
        <Button type="button" variant="secondary" onClick={() => void openExternalUrl(source.url)}>打开原网页</Button>
      </div>
    </Modal>}
  </>;
}
