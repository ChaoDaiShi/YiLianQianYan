import { Plug } from "lucide-react";

/** Presentation slots only; no discovery, invocation, protocol or secret state. */
export default function ExtensionSlots() {
  return <details className="studio-detail" data-testid="studio-extension-slots">
    <summary><Plug size={14}/>MCP 能力 · 结构预留</summary>
    <p>尚未接入服务。以下区域不参与任务执行，也不会保存参数。</p>
    <dl className="studio-extension-info"><dt>能力信息</dt><dd>服务与工具元信息预留</dd><dt>参数输入</dt><dd>等待服务提供参数定义</dd></dl>
    <button type="button" disabled>发布动作 · 未接入</button>
  </details>;
}
