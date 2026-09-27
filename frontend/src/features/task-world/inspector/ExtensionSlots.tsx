import { Plug } from "lucide-react";

/** Presentation slots only; no discovery, invocation, protocol or secret state. */
export default function ExtensionSlots() {
  return <details className="studio-detail" data-testid="studio-extension-slots">
    <summary><Plug size={14}/>MCP 能力与后续绑定</summary>
    <p>使用“选择能力”接入已连接的 MCP 工具。自动数据绑定与专用发布操作尚未接入。</p>
    <dl className="studio-extension-info"><dt>能力信息</dt><dd>从能力目录选择工具</dd><dt>参数输入</dt><dd>所选工具提供参数定义</dd></dl>
    <button type="button" disabled>发布动作 · 未接入</button>
  </details>;
}
