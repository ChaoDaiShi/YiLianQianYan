import { Plug } from "lucide-react";

/** Presentation slots only; no discovery, invocation, protocol or secret state. */
export default function ExtensionSlots() {
  return <details className="studio-detail" data-testid="studio-extension-slots">
    <summary><Plug size={14}/>MCP 能力与数据绑定</summary>
    <p>使用“选择能力”接入已连接的 MCP 工具，并在参数面板配置输入与上游数据绑定。</p>
    <dl className="studio-extension-info"><dt>能力信息</dt><dd>从能力目录选择工具</dd><dt>参数输入</dt><dd>所选工具提供参数定义</dd></dl>
  </details>;
}
