import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type { ConnectorMessage } from "../../../api/connectorMessages";
import MessageSelectionPanel from "./MessageSelectionPanel";

const message: ConnectorMessage = { id: "answer-1", parent_id: null, role: "assistant", content: "# Full answer\n\nA long paragraph remains available in the inspector.", reasoning: "Actual model reasoning", conversation_id: "conversation-1", created_at: 1, status: "completed", error: null };
const render = (item: ConnectorMessage) => renderToStaticMarkup(<MemoryRouter><MessageSelectionPanel message={item} title="Existing conversation" busy={false} onClose={vi.fn()} onBranch={vi.fn()} /></MemoryRouter>);

describe("selected message inspector", () => {
  it("retains full Markdown and the assistant reasoning with explicit navigation actions", () => {
    const html = render(message);
    expect(html).toContain("<h1>Full answer</h1>");
    expect(html).toContain("A long paragraph remains available in the inspector.");
    expect(html).toContain("Actual model reasoning");
    expect(html).toContain('href="/chat/conversation-1"');
    expect(html).toContain("从这条回答继续");
    expect(html).toContain('aria-label="关闭消息详情"');
  });
  it("does not attribute turn-level model reasoning to a user message", () => {
    const html = render({ ...message, id: "question-1", role: "user", content: "The actual question" });
    expect(html).toContain("The actual question");
    expect(html).not.toContain("Actual model reasoning");
    expect(html).not.toContain("从这条回答继续");
  });
});
