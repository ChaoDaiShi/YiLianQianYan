import type { ConversationProcess } from "../../types";
import ToolCallCard from "./ToolCallCard";
import { formatToolDisplayName } from "./toolDisplay";
import "./chatProcess.css";

export default function MessageProcess({ process }: { process?: ConversationProcess }) {
  if (!process?.reasoning && !process?.tools.length) return null;
  return <details className="message-process" aria-label="本轮处理过程">
    <summary><span aria-hidden="true">⌄</span><span>处理过程</span>{process.reasoning && <span>思考</span>}{process.tools.length > 0 && <span>{process.tools.length} 次工具调用</span>}</summary>
    <div className="message-process-content">
      {process.reasoning && <section aria-label="模型实际返回的思考"><h4>模型思考</h4><pre className="message-reasoning">{process.reasoning}</pre></section>}
      {process.tools.map(tool => tool.execution ? <ToolCallCard key={tool.toolCallId} {...tool.execution} compact/> : <details key={tool.toolCallId} className="message-protocol-call"><summary>{formatToolDisplayName(tool.name)} · 调用记录</summary><dl><dt>工具调用</dt><dd>{tool.toolCallId}</dd><dt>参数</dt><dd><pre>{tool.arguments || JSON.stringify(tool.args, null, 2)}</pre></dd>{tool.result && <><dt>返回内容</dt><dd><pre>{tool.result}</pre></dd></>}</dl></details>)}
    </div>
  </details>;
}
