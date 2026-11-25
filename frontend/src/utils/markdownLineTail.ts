import type { Element, ElementContent, Root } from "hast";

const words = new Intl.Segmenter("zh", { granularity: "word" });
const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const prose = new Set(["p", "li", "h1", "h2", "h3", "h4", "h5", "h6", "td", "th"]);
const phrasing = new Set(["a", "del", "em", "span", "strong", "sup"]);

function isPhrasing(node: ElementContent) {
  return node.type === "text" || (node.type === "element" && phrasing.has(node.tagName));
}

function inlineText(node: ElementContent): string {
  if (node.type === "text") return node.value;
  if (node.type === "element" && phrasing.has(node.tagName)) return node.children.map(inlineText).join("");
  return "\n";
}

function lineTailStart(text: string) {
  const segments = words.segment(text);
  let last = segments.containing(text.trimEnd().length - 1);
  while (last && !last.isWordLike && !/\s/u.test(last.segment)) {
    last = segments.containing(last.index - 1);
  }
  if (!last?.isWordLike || !/\p{Script=Han}/u.test(last.segment)) return;

  if (graphemes.segment(last.segment).containing(0)?.segment !== last.segment) return last.index;

  const previous = segments.containing(last.index - 1);
  if (previous?.isWordLike) return previous.index;
}

function splitInlineAt(nodes: ElementContent[], offset: number): [ElementContent[], ElementContent[]] {
  for (const [index, node] of nodes.entries()) {
    const length = inlineText(node).length;
    if (offset >= length) {
      offset -= length;
      continue;
    }

    const before = nodes.slice(0, index);
    const after = nodes.slice(index + 1);
    if (offset === 0) return [before, [node, ...after]];
    if (node.type === "text") {
      return [[...before, { ...node, value: node.value.slice(0, offset) }], [{ ...node, value: node.value.slice(offset) }, ...after]];
    }
    // Keep links whole so each link has one keyboard stop and one hit target.
    if (node.type === "element" && node.tagName !== "a") {
      const [leading, trailing] = splitInlineAt(node.children, offset);
      if (leading.length) before.push({ ...node, children: leading });
      return [before, [{ ...node, children: trailing }, ...after]];
    }
    return [before, [node, ...after]];
  }
  return [nodes, []];
}

function protectInlineRun(nodes: ElementContent[]): ElementContent[] {
  const offset = lineTailStart(nodes.map(inlineText).join(""));
  if (offset === undefined) return nodes;

  const [leading, trailing] = splitInlineAt(nodes, offset);
  return [...leading, { type: "element", tagName: "span", properties: { className: ["markdown-line-tail"] }, children: trailing }];
}

function protectBlock(node: Element) {
  const children: ElementContent[] = [];
  let start = 0;
  for (const [index, child] of node.children.entries()) {
    if (isPhrasing(child)) continue;
    children.push(...protectInlineRun(node.children.slice(start, index)), child);
    start = index + 1;
  }
  node.children = children.concat(protectInlineRun(node.children.slice(start)));
}

function protectMarkdownLineTail() {
  return (root: Root) => {
    const visit = (parent: Root | Element) => {
      for (const child of parent.children) {
        if (child.type !== "element") continue;
        if (prose.has(child.tagName)) protectBlock(child);
        visit(child);
      }
    };
    visit(root);
  };
}

export const markdownLineTailPlugins = [protectMarkdownLineTail];
