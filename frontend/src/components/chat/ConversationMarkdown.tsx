import { memo, useMemo } from "react";
import type { Element, ElementContent, Root } from "hast";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link } from "react-router-dom";
import { markdownLineTailPlugins } from "../../utils/markdownLineTail";
import { readKnowledgeAnchor } from "../../features/knowledge/knowledgeAnchor";
import { isExternalHttpUrl, openExternalUrl } from "./externalLink";
import { ReplyWebSource, type WebSource } from "./ReplyWebSources";

const markdownPlugins = [remarkGfm];
function renderComponents(sources?: ReadonlyMap<string, WebSource>): Components {
  return {
  span: ({ node, ...props }) => <span key={node?.properties.dataStreamOffset?.toString()} {...props} />,
  a: ({ node: _node, href, children, ...props }) => {
    if (href?.startsWith("/knowledge?") && readKnowledgeAnchor(new URLSearchParams(href.slice(11)))) {
      return <Link to={href}>{children}</Link>;
    }
    const external = Boolean(href && isExternalHttpUrl(href));
    const source = external && sources ? sources.get(new URL(href!).href) : undefined;
    return (
      <>
        <a
          {...props}
          href={href}
          target={external ? "_blank" : undefined}
          rel={external ? "noreferrer" : undefined}
          onClick={external ? (event) => {
            event.preventDefault();
            void openExternalUrl(href!);
          } : undefined}
        >
          {children}
        </a>
        {source && <ReplyWebSource source={source} />}
      </>
    );
  },
  img: ({ node: _node, src, alt, ...props }) => {
    const target = src?.startsWith("//") ? `https:${src}` : src;
    if (!target || !isExternalHttpUrl(target)) return <img {...props} src={src} alt={alt || ""} />;
    return <button type="button" className="underline text-[var(--accent-soft-fg)]" onClick={() => void openExternalUrl(target)}>
      {alt ? `打开图片 · ${alt}` : "打开外部图片"}
    </button>;
  },
  table: ({ node: _node, children, ...props }) => (
    <div className="table-scroll-container my-3 max-w-full overflow-x-auto">
      <table className="min-w-full divide-y border border-[var(--border-soft)]" {...props}>
        {children}
      </table>
    </div>
  ),
  pre: ({ node: _node, ...props }) => (
    <pre
      className="code-scroll-container my-3 max-w-full overflow-x-auto rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs"
      {...props}
    />
  ),
  };
}

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const staticMarkdown = new Set(["pre", "code", "table"]);

function revealStreamingTail(source: string, revealFrom: number) {
  return () => (root: Root) => {
    const letters: Element[] = [];
    const walk = (parent: Root | Element) => {
      for (let i = parent.children.length - 1; i >= 0 && letters.length < 4; i--) {
        const node = parent.children[i];
        if (node.type === "element" && !staticMarkdown.has(node.tagName)) walk(node);
        if (node.type !== "text" || !node.value.trim()) continue;

        const start = node.position?.start.offset;
        const end = node.position?.end.offset;
        if (start === undefined || end === undefined || end <= revealFrom) continue;
        const appended = source.slice(revealFrom, end);
        if (start < revealFrom && !node.value.endsWith(appended)) continue;
        const boundary = start < revealFrom ? node.value.length - appended.length : 0;

        const segments = graphemes.segment(node.value);
        const tail: ElementContent[] = [];
        let cursor = node.value.length;
        while (cursor > boundary && letters.length < 4) {
          const part = segments.containing(cursor - 1);
          if (!part || part.index < boundary) break;
          cursor = part.index;
          const glyph = { type: "text" as const, value: part.segment };
          if (!part.segment.trim()) {
            tail.push(glyph);
            continue;
          }
          const letter: Element = {
            type: "element",
            tagName: "span",
            properties: { className: ["stream-letter"], dataStreamOffset: end - node.value.length + part.index },
            children: [glyph],
          };
          tail.push(letter);
          letters.push(letter);
        }
        if (tail.length) {
          if (cursor) tail.push({ type: "text", value: node.value.slice(0, cursor) });
          parent.children.splice(i, 1, ...tail.reverse());
        }
      }
    };
    walk(root);
    letters.reverse().forEach((letter, index) => { letter.properties.style = `animation-delay: ${index * 20}ms`; });
  };
}

export default memo(function ConversationMarkdown({ text, revealFrom, sources }: { text: string; revealFrom?: number; sources?: ReadonlyMap<string, WebSource> }) {
  const components = useMemo(() => renderComponents(sources), [sources]);
  return (
    <div className="conversation-markdown prose prose-sm min-w-0 max-w-none">
      <ReactMarkdown
        remarkPlugins={markdownPlugins}
        rehypePlugins={revealFrom === undefined ? markdownLineTailPlugins : [revealStreamingTail(text, revealFrom), ...markdownLineTailPlugins]}
        components={components}
      >{text}</ReactMarkdown>
    </div>
  );
});
