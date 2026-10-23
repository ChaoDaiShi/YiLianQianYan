import { useState } from "react";
import { Play } from "lucide-react";
import { Button, FormField, Modal, Textarea } from "../../components/ui";
import { startWorkflowRun, type WorkflowGraphRecord, type WorkflowRunInputs } from "../../api/workflows";

function initialValues(graph: WorkflowGraphRecord): Record<string, string> {
  return Object.fromEntries(graph.definition.nodes.map(({ id, config }) => [id,
    config.type === "agent" ? config.prompt : config.type === "tool" ? JSON.stringify(config.arguments, null, 2) : config.type === "subagent" ? config.task : config.type === "output" ? config.template ?? "" : "",
  ]));
}

export default function WorkflowRunDialog({ graph, onClose, onStarted }: {
  graph: WorkflowGraphRecord; onClose: () => void; onStarted: (runId: string) => void;
}) {
  const [values, setValues] = useState(() => initialValues(graph));
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState("");
  async function start() {
    setError("");
    const inputs: WorkflowRunInputs = {};
    for (const node of graph.definition.nodes) {
      const value = values[node.id];
      switch (node.config.type) {
        case "agent":
          if (!value.trim()) { setError(`节点 ${node.id} 的提示词不能为空`); return; }
          inputs[node.id] = { type: "agent", prompt: value }; break;
        case "subagent":
          if (!value.trim()) { setError(`节点 ${node.id} 的任务不能为空`); return; }
          inputs[node.id] = { type: "subagent", task: value }; break;
        case "tool":
          try {
            const argumentsValue: unknown = JSON.parse(value);
            if (!argumentsValue || typeof argumentsValue !== "object" || Array.isArray(argumentsValue)) throw new Error("请输入 JSON 对象");
            inputs[node.id] = { type: "tool", arguments: argumentsValue as Record<string, unknown> };
          } catch (cause) { setError(`节点 ${node.id}：${cause instanceof Error ? cause.message : String(cause)}`); return; }
          break;
        case "output": inputs[node.id] = { type: "output", template: value || null }; break;
      }
    }
    setStarting(true);
    const response = await startWorkflowRun(graph.id, inputs);
    setStarting(false);
    if (response.ok) onStarted(response.data.run_id);
    else setError(response.error);
  }
  return <Modal open onClose={() => { if (!starting) onClose(); }} title={`运行 · ${graph.name}`} size="lg" footer={<><Button variant="secondary" disabled={starting} onClick={onClose}>取消</Button><Button disabled={starting} loading={starting} onClick={() => void start()}><Play size={14} />开始运行</Button></>}>
    <div className="space-y-4">
      <p className="text-xs text-[var(--text-secondary)]">调整本次运行使用的节点输入。保存的工作流配置保持原值。</p>
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      {graph.definition.nodes.map(node => <section key={node.id} className="rounded-xl border border-[var(--border-soft)] p-3 space-y-2">
        <div className="flex items-center justify-between gap-2"><h4 className="font-medium text-sm">{node.id}</h4><span className="text-xs text-[var(--text-secondary)]">{node.config.type === "tool" ? node.config.tool_name : node.config.type === "subagent" ? node.config.subagent_name : node.config.type === "agent" ? "文本生成" : node.config.type === "condition" ? "条件" : "输出"}</span></div>
        {node.config.type === "condition" ? <p className="text-xs text-[var(--text-secondary)]">{node.config.when === "always" ? "始终继续" : "上一步成功时继续"}</p> : <FormField label={node.config.type === "tool" ? "工具参数（JSON）" : node.config.type === "agent" ? "提示词" : node.config.type === "subagent" ? "委派任务" : "输出内容"}><Textarea aria-label={`${node.id} 运行参数`} rows={node.config.type === "tool" ? 5 : 3} value={values[node.id]} disabled={starting} className={node.config.type === "tool" ? "font-mono text-xs" : undefined} onChange={event => setValues(current => ({ ...current, [node.id]: event.target.value }))} /></FormField>}
      </section>)}
    </div>
  </Modal>;
}
