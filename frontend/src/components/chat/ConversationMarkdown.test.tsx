import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import ConversationMarkdown from "./ConversationMarkdown";
import { replyWebSources } from "./ReplyWebSources";

describe("conversation Markdown line endings", () => {
  it("keeps the final Chinese word and its punctuation together across inline formatting", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text="请核对**运行结**果。" />);
    expect(html).toContain('<p>请核对<strong>运行</strong><span class="markdown-line-tail"><strong>结</strong>果。</span></p>');
  });

  it("joins a final single-character word to the preceding word when possible", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text="这段文字已经结束了。" />);
    expect(html).toContain('<span class="markdown-line-tail">结束了。</span>');
  });

  it("allows standalone characters, symbols, and explicitly separate lines", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text={"甲。\n\nA\n\n👍🏽\n\n上面是结果。  \n乙"} />);
    expect(html).toContain("<p>甲。</p>");
    expect(html).toContain("<p>A</p>");
    expect(html).toContain("<p>👍🏽</p>");
    expect(html).toContain("<br/>\n乙</p>");
  });

  it("keeps a trailing symbol with a word when they are adjacent", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text="请核对结果👍🏽" />);
    expect(html).toContain('<span class="markdown-line-tail">结果👍🏽</span>');
  });

  it("keeps a link a single interactive element when the final word crosses it", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text="请阅读[运行结](https://example.com)果。" />);
    expect(html).toContain('<span class="markdown-line-tail"><a href="https://example.com" target="_blank" rel="noreferrer">运行结</a>果。</span>');
  });

  it("offers previews only for cited links from a successful search in the same reply", () => {
    const execution = { toolCallId: "search", name: "ai_search", status: "success" as const, args: {} };
    const result = JSON.stringify({ kind: "web_search_sources", sources: [{ url: "https://example.com/report", title: "研究报告", excerpt: "搜索摘要" }] });
    const sources = replyWebSources([{ toolCallId: "search", name: "ai_search", result, execution }]);
    const html = renderToStaticMarkup(<ConversationMarkdown text="请查看[报告](https://example.com/report)和[其他](https://example.net/other)。" sources={sources} />);
    expect(html.match(/aria-label="预览来源 /g)).toHaveLength(1);
    expect(html).toContain('aria-label="预览来源 example.com"');
    expect(html).not.toContain("搜索摘要");
    expect(replyWebSources([{ toolCallId: "search", name: "ai_search", result, execution: { ...execution, status: "error" } }]).size).toBe(0);
  });

  it("opens remote Markdown images only after user interaction", () => {
    const html = renderToStaticMarkup(<ConversationMarkdown text="![远端图片](https://example.com/tracker.png)" />);
    expect(html).toContain("打开图片 · 远端图片");
    expect(html).not.toContain("<img");
    expect(html).not.toContain("tracker.png");
  });

  it("keeps stable document citations in the application router", () => {
    const html = renderToStaticMarkup(<MemoryRouter><ConversationMarkdown text="资料见[原文](/knowledge?document=doc-1&version=v1&start=0&end=12)。" /></MemoryRouter>);
    expect(html).toContain('href="/knowledge?document=doc-1&amp;version=v1&amp;start=0&amp;end=12"');
    expect(html).not.toContain('target="_blank"');
  });
});

describe("streaming conversation Markdown", () => {
  it("reveals only the newest four graphemes while preserving Markdown and emoji", () => {
    const previous = "之前的正文。";
    const html = renderToStaticMarkup(<ConversationMarkdown text={`${previous} **快速出结果👍🏽**`} revealFrom={previous.length} />);

    expect(html).toContain("之前的正文。 <strong>快速");
    expect([...html.matchAll(/class="stream-letter"[^>]*style="animation-delay:([0-9]+ms)">([^<]+)</g)]
      .map((match) => match.slice(1))).toEqual([["0ms", "出"], ["20ms", "结"], ["40ms", "果"], ["60ms", "👍🏽"]]);
  });

  it("animates appended graphemes within a protected Chinese word", () => {
    const previous = "请核对结";
    const html = renderToStaticMarkup(<ConversationMarkdown text={`${previous}果。`} revealFrom={previous.length} />);
    expect(html).toContain('<span class="markdown-line-tail">结<span class="stream-letter"');
    expect([...html.matchAll(/class="stream-letter"[^>]*>([^<]+)</g)].map((match) => match[1])).toEqual(["果", "。"]);
  });

  it("leaves previously rendered text, history, and fenced code intact", () => {
    const previous = "**前文**\n\n```js\n";
    const text = previous + "const result = 10;\n```";
    for (const revealFrom of [undefined, previous.length]) {
      const html = renderToStaticMarkup(<ConversationMarkdown text={text} revealFrom={revealFrom} />);
      expect(html).toContain("<strong>前文</strong>");
      expect(html).toContain("const result = 10;");
      expect(html).not.toContain("stream-letter");
    }
  });
});
