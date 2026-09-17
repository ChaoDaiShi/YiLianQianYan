import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ChatVoiceInput, type ChatVoiceControls } from "./ChatVoiceInput";

const controls: ChatVoiceControls = {
  status: "listening",
  volume: 47,
  start: vi.fn(),
  stop: vi.fn(),
  cancel: vi.fn(),
  subscribeFinal: () => () => undefined,
};

describe("ChatVoiceInput contract", () => {
  it("offers an explicit microphone with a measured listening indicator", () => {
    const html = renderToStaticMarkup(
      <ChatVoiceInput controls={controls} onRequestGlobalVoiceSession={vi.fn()} />,
    );
    expect(html).toContain("开始语音输入");
    expect(html).toContain("正在听取");
    expect(html).toContain("47%");
    expect(html).toContain("chat-voice-meter-bar");
    expect(html).toContain("停止录音");
    expect(html).toContain("取消语音输入");
    expect(html).toContain("与小涟语音对话");
  });

  it.each(["acquiring", "listening", "transcribing"] as const)(
    "keeps a visible cancel action while %s",
    (status) => {
      const html = renderToStaticMarkup(
        <ChatVoiceInput controls={{ ...controls, status }} onRequestGlobalVoiceSession={vi.fn()} />,
      );
      expect(html).toContain("取消语音输入");
    },
  );

  it("uses the existing global voice session entry when capture is unavailable", () => {
    const html = renderToStaticMarkup(
      <ChatVoiceInput controls={null} onRequestGlobalVoiceSession={vi.fn()} />,
    );
    expect(html).toContain("与小涟语音对话");
    expect(html).not.toContain("自动发送");
  });

  it("fails closed when the root has not provided the existing-session request", () => {
    const html = renderToStaticMarkup(<ChatVoiceInput controls={null} />);
    expect(html).toContain("disabled");
  });
});
