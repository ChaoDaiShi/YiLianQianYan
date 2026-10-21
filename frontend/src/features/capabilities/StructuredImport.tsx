import { useState } from "react";
import { createMcpServer } from "../../api/plugins";
import { createWorkflow, createWorkflowGraph } from "../../api/workflows";
import { Button, Textarea, ErrorState } from "../../components/ui";
export default function StructuredImport() {
  const [kind, setKind] = useState("workflow"); const [text, setText] = useState(""); const [busy, setBusy] = useState(false); const [error, setError] = useState(""); const [result, setResult] = useState("");
  async function install() { setBusy(true); setError(""); setResult(""); try {
    const parsed: unknown = JSON.parse(text);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("需要一个 JSON 对象。");
    const value = parsed as Record<string, unknown>;
    if (typeof value.name !== "string" || !value.name.trim()) throw new Error("需要名称 name。");
    if (kind === "workflow") {
      if (value.definition) {
        const saved = await createWorkflowGraph(value as unknown as Parameters<typeof createWorkflowGraph>[0]); if (!saved.ok) throw new Error(saved.error); setResult(`已导入运行工作流：${saved.data.name}`); return;
      }
      if (!Array.isArray(value.nodes) || value.nodes.some(node => typeof node !== "string")) throw new Error("模板工作流 nodes 必须为字符串列表。运行 DAG 请使用工作流编辑器。");
      const saved = await createWorkflow(value); if (!saved) throw new Error("工作流未能保存。"); setResult(`已导入工作流：${saved.name}`);
    } else {
      if (value.enabled === true) throw new Error("导入配置必须保持 enabled:false，连接前请在 MCP 管理中检查。");
      const saved = await createMcpServer({ ...value, enabled: false }); if (!saved.ok) throw new Error(saved.error); setResult(`已导入 MCP：${saved.data.name}（未连接）`);
    }
  } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } finally { setBusy(false); } }
  return <section className="my-4 rounded-xl border border-[var(--border-soft)] p-4"><h2>工作流与 MCP 配置导入</h2><label>配置类型 <select aria-label="配置类型" value={kind} onChange={event => { setKind(event.target.value); setResult(""); }}><option value="workflow">工作流 JSON</option><option value="mcp">单个 MCP 服务 JSON</option></select></label><Textarea label="JSON 内容" value={text} onChange={event => { setText(event.target.value); setResult(""); }} rows={5} /><Button disabled={busy || !text.trim()} onClick={() => void install()}>确认导入配置</Button>{error && <ErrorState title="配置未导入" description={error} />}{result && <p role="status">{result}</p>}</section>;
}
