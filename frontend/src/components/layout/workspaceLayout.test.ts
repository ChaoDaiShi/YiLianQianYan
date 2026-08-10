import { describe, expect, it } from "vitest";
import appShellSource from "./AppShell.tsx?raw";
import chatPageSource from "../../pages/ChatPage.tsx?raw";
import chatViewSource from "../chat/ChatView.tsx?raw";
import messageListSource from "../chat/MessageList.tsx?raw";
import * as workspaceLayout from "./workspaceLayout";

const { getDrawerState, getWorkspaceMode } = workspaceLayout;

describe("getWorkspaceMode", () => {
  it.each([
    [800, "narrow"],
    [959, "narrow"],
    [960, "compact"],
    [1179, "compact"],
    [1180, "full"],
    [1920, "full"],
  ] as const)("maps %ipx to %s", (width, expected) => {
    expect(getWorkspaceMode(width)).toBe(expected);
  });
});

describe("getDrawerState", () => {
  it("keeps narrow drawers mutually exclusive", () => {
    expect(getDrawerState("narrow", "conversations")).toEqual({
      conversationOpen: true,
      executionOpen: false,
    });
    expect(getDrawerState("narrow", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: true,
    });
  });

  it("opens only the execution drawer in compact mode", () => {
    expect(getDrawerState("compact", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: true,
    });
    expect(getDrawerState("compact", "conversations")).toEqual({
      conversationOpen: false,
      executionOpen: false,
    });
  });

  it("uses no drawers in full mode", () => {
    expect(getDrawerState("full", "execution")).toEqual({
      conversationOpen: false,
      executionOpen: false,
    });
  });
});

interface CandidateScrollTarget {
  scrollHeight: number;
  scrollTo(options: ScrollToOptions): void;
}

type CandidateWorkspaceLayout = typeof workspaceLayout & {
  APP_CONTENT_VIEWPORT_CLASS_NAME?: string;
  WORKBENCH_VIEWPORT_CLASS_NAME?: string;
  CHAT_COLUMN_VIEWPORT_CLASS_NAME?: string;
  MESSAGE_LIST_VIEWPORT_CLASS_NAME?: string;
  scrollMessageListToBottom?: (
    container: CandidateScrollTarget | null,
    behavior?: ScrollBehavior
  ) => void;
};

describe("history conversation viewport contract", () => {
  const candidate = workspaceLayout as CandidateWorkspaceLayout;

  it("scrolls the supplied message container instead of a page anchor", () => {
    let received: ScrollToOptions | undefined;
    const container: CandidateScrollTarget = {
      scrollHeight: 27_237,
      scrollTo(options) {
        received = options;
      },
    };

    expect(candidate.scrollMessageListToBottom).toBeTypeOf("function");
    candidate.scrollMessageListToBottom?.(container, "smooth");

    expect(received).toEqual({ top: 27_237, behavior: "smooth" });
    expect(() =>
      candidate.scrollMessageListToBottom?.(null, "smooth")
    ).not.toThrow();
  });

  it.each([
    ["APP_CONTENT_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-hidden"]],
    [
      "WORKBENCH_VIEWPORT_CLASS_NAME",
      ["min-h-0", "overflow-hidden", "grid-rows-[minmax(0,1fr)]"],
    ],
    ["CHAT_COLUMN_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-hidden"]],
    ["MESSAGE_LIST_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-y-auto"]],
  ] as const)("exports %s with required tokens", (name, requiredTokens) => {
    const className = candidate[name];
    expect(className).toBeTypeOf("string");
    const tokens = new Set(className?.split(/\s+/));
    requiredTokens.forEach((token) => expect(tokens.has(token)).toBe(true));
  });

  it("wires the shared viewport contracts into the rendered components", () => {
    expect(appShellSource).toContain("APP_CONTENT_VIEWPORT_CLASS_NAME");
    expect(chatPageSource).toContain("WORKBENCH_VIEWPORT_CLASS_NAME");
    expect(chatViewSource).toContain("CHAT_COLUMN_VIEWPORT_CLASS_NAME");
    expect(chatViewSource).toContain("scrollMessageListToBottom");
    expect(chatViewSource).not.toContain("scrollIntoView");
    expect(messageListSource).toContain("MESSAGE_LIST_VIEWPORT_CLASS_NAME");
    expect(messageListSource).toContain("ref={scrollContainerRef}");
  });
});
