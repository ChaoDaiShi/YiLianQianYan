import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { GlobalVoiceLeaf } from "./GlobalVoiceLeaf";

const actions = {
  onToggleExpanded: vi.fn(),
  onMicrophone: vi.fn(),
  onSound: vi.fn(),
  onEnd: vi.fn(),
};

describe("GlobalVoiceLeaf product contract", () => {
  it("renders product states and context without runtime handles", () => {
    const html = renderToStaticMarkup(
      <GlobalVoiceLeaf
        state="understanding"
        expanded
        heardText="请整理今天的待办"
        currentContext="当前对话：工作计划"
        interpretedAction="整理待办（待确认）"
        microphoneLabel="停止录音"
        soundLabel="静音"
        {...actions}
      />,
    );

    expect(html).toContain("正在理解");
    expect(html).toContain("请整理今天的待办");
    expect(html).toContain("当前对话：工作计划");
    expect(html).toContain("整理待办（待确认）");
    expect(html).not.toMatch(/voice_session_id|generation|lease|provider/i);
  });

  it.each(["listening", "understanding", "speaking", "confirmation", "error"] as const)(
    "exposes the visible %s state",
    (state) => {
      const html = renderToStaticMarkup(
        <GlobalVoiceLeaf
          state={state}
          expanded={false}
          microphoneLabel="开始说话"
          soundLabel="静音"
          {...actions}
        />,
      );
      expect(html).toContain(`data-state="${state}"`);
    },
  );

  it("keeps every control manual and supplies the bottom-center layout hook", () => {
    const html = renderToStaticMarkup(
      <GlobalVoiceLeaf
        state="listening"
        expanded
        microphoneLabel="停止录音"
        soundLabel="静音"
        {...actions}
      />,
    );
    expect(html).toContain("global-voice-dock");
    expect(html).toContain("voice-mic");
    expect(html).toContain("结束会话");
  });
});
