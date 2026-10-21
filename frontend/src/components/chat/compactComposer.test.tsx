import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import ChatInput from "./ChatInput";

const voice = { status: "idle" as const, volume: 0, start() {}, stop() {}, cancel() {}, subscribeFinal() { return () => undefined; } };
function render(mode: "chat" | "work" | "create", loading = false) {
  return renderToStaticMarkup(<ChatInput onSend={() => true} isLoading={loading} workspaceMode={mode} onWorkspaceModeChange={async () => undefined} chatVoiceControls={voice} onRequestGlobalVoiceSession={() => undefined}/>);
}
describe("compact composer rendering", () => {
  it("keeps one model trigger and attachment, voice, mode and icon send controls in the chat pill", () => {
    const html = render("chat");
    expect(html).toContain('data-workspace-mode="chat"');
    expect(html.match(/aria-label="模型与思考设置"/g)).toHaveLength(1);
    expect(html).not.toContain("<select");
    expect(html).toContain('aria-label="导入资源"');
    expect(html).toContain('aria-label="开始语音输入"');
    expect(html).toContain('aria-label="切换工作模式"');
    expect(html).toContain('aria-label="发送消息"');
    expect(html).not.toContain("每次操作都先征得你的批准");
    expect(html).not.toContain(">发送<");
  });
  it("uses the same compact control surface with a mode-specific work or creation draft", () => {
    expect(render("work")).toContain('placeholder="描述你希望完成的工作…"');
    expect(render("create")).toContain('placeholder="写下想法，开始创造…"');
    expect(render("create").match(/<textarea/g)).toHaveLength(1);
    expect(render("work", true)).toContain('aria-label="停止执行"');
    expect(render("work", true)).not.toContain('aria-label="发送消息"');
  });
});
