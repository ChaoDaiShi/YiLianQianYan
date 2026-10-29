import type { ConversationProcess } from "../../types";
import { useEffect, useState } from "react";
import ToolCallCard from "./ToolCallCard";
import { formatToolDisplayName } from "./toolDisplay";
import "./chatProcess.css";

export default function MessageProcess({ process, streaming = false }: { process?: ConversationProcess; streaming?: boolean }) {
  const failedCount = process?.tools.filter(tool=>tool.execution?.status==="error"||tool.execution?.verificationStatus==="failed").length??0;
  const [open, setOpen] = useState(streaming || failedCount>0);
  useEffect(() => { if (streaming || failedCount>0) setOpen(true); }, [streaming,failedCount]);
  if (!process?.reasoning && !process?.tools.length) return null;
  return <details className="message-process" aria-label="本轮处理过程" open={open} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary><span aria-hidden="true">⌄</span><span>{streaming ? "本轮处理中" : "处理过程"}</span>{process.reasoning && !streaming && <span>模型返回的思考</span>}{process.tools.length > 0 && <span>{process.tools.length} 次工具调用</span>}{failedCount>0&&<span className="message-process-failure">{failedCount} 项失败</span>}</summary>
    <div className="message-process-content">
      {process.reasoning && <section aria-label="模型实际返回的思考"><h4>模型思考</h4><pre className="message-reasoning">{process.reasoning}</pre></section>}
      {process.tools.map(tool => tool.execution ? <ToolCallCard key={tool.toolCallId} {...tool.execution} compact/> : <details key={tool.toolCallId} className="message-protocol-call"><summary>{formatToolDisplayName(tool.name)} · 调用记录</summary><dl><dt>工具调用</dt><dd>{tool.toolCallId}</dd><dt>参数</dt><dd><pre>{tool.arguments || JSON.stringify(tool.args, null, 2)}</pre></dd>{tool.result && <><dt>返回内容</dt><dd><pre>{tool.result}</pre></dd></>}</dl></details>)}
    </div>
  </details>;
}
