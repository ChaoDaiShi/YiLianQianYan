import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import MessageBubble, { areMessageBubblePropsEqual } from "./MessageBubble";
import type { Message } from "../../types";

const answer:Message={id:"answer",role:"assistant",content:"A historical answer",created_at:1};
describe("explicit chat branching",()=>{
  it("offers a new branch on an assistant answer without a historical continue action",()=>{
    const html=renderToStaticMarkup(<MessageBubble message={answer} onBranch={vi.fn()}/>);
    expect(html).toContain("从这条回答新建分支");
    expect(html).not.toContain("从这条回答继续提问");
  });
  it("keeps branch creation disabled while the current conversation is busy",()=>{
    const html=renderToStaticMarkup(<MessageBubble message={answer} onBranch={vi.fn()} actionsDisabled/>);
    expect(html).toMatch(/disabled=""[^>]*aria-label="从这条回答新建分支"/);
  });
  it("does not expose answer branching on a user message",()=>{
    expect(renderToStaticMarkup(<MessageBubble message={{...answer,role:"user"}} onBranch={vi.fn()}/>)).not.toContain("新建分支");
  });
  it("updates the memoized action when its conversation owner changes",()=>{
    expect(areMessageBubblePropsEqual({message:answer,onBranch:vi.fn()},{message:answer,onBranch:vi.fn()})).toBe(false);
  });
});
