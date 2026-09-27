import SchemaForm from "../schema-form/SchemaForm";
import {parseArguments,hasHeaderBinding,HEADER_UNSUPPORTED} from "../schema-form/schema";
import {useCapabilityDraft} from "../capability-picker/useCapabilityDraft";
import type {useCanvasCapabilities} from "../capability-picker/useCanvasCapabilities";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type {
  ApiError,
  TaskCheckpointSummary,
  TaskEdge,
  TaskNodeKind,
  TaskRevisionSummary,
} from "../../../api/taskWorld";
import { Badge, Button, EmptyState, Panel } from "../../../components/ui";
import {
  executionStatusLabel,
  getExecutorAvailability,
  type TaskNodeProjection,
} from "../taskGraphProjection";
import AcceptanceSection from "./AcceptanceSection";
import ArtifactSection from "./ArtifactSection";
import BasicSection from "./BasicSection";
import DependencySection from "./DependencySection";
import ExecutionSection from "./ExecutionSection";
import ExecutorSection from "./ExecutorSection";
import ResourceSection from "./ResourceSection";
import StateSection from "./StateSection";
import ExtensionSlots from "./ExtensionSlots";
import VersionSection from "./VersionSection";

interface TaskNodeDraft {
  kind: TaskNodeKind;
  title: string;
  input: Record<string, unknown>;
  retry_policy: { max_attempts: number };
}

interface TaskInspectorProps {
  graphId: string;
  capabilities?: ReturnType<typeof useCanvasCapabilities>;
  node: TaskNodeProjection | null;
  expectedRevision: number;
  revisions: TaskRevisionSummary[];
  checkpoints: TaskCheckpointSummary[];
  dependencyEdges: TaskEdge[];
  dependencyCandidates: TaskNodeProjection[];
  saving?: boolean;
  saveError?: ApiError | null;
  onSave: (draft: TaskNodeDraft) => Promise<void>;
  onStart: () => Promise<void>;
  onStartExecution?: () => Promise<void>;
  onCancelExecution?: () => Promise<void>;
  onRerun?: () => Promise<void>;
  onCheckpoint: () => Promise<void>;
  onRestore: (checkpointId: string) => Promise<void>;
  onAddDependency: (from: string, to: string) => Promise<void>;
  onRemoveDependency: (from: string, to: string) => Promise<void>;
  graphLocked: boolean;
}

/**
 * Node inspector shell.
 *
 * Owns only what the sections share: the semantic draft being edited, the
 * single form submit, and the layout order. Each section reads the projection
 * and calls back with intent — none of them talks to the API directly.
 */
export default function TaskInspector({
  graphId,
  capabilities,
  node,
  expectedRevision,
  revisions,
  checkpoints,
  dependencyEdges,
  dependencyCandidates,
  saving = false,
  saveError = null,
  onSave,
  onStartExecution,
  onCancelExecution,
  onRerun,
  onCheckpoint,
  onRestore,
  onAddDependency,
  onRemoveDependency,
  graphLocked,
}: TaskInspectorProps) {
  const capabilityDraft=useCapabilityDraft(graphId,node?.id,expectedRevision,node?.executor_ref);
  const [argumentError,setArgumentError]=useState("");
  const [title, setTitle] = useState("");
  const [instruction, setInstruction] = useState("");
  const [executorRef, setExecutorRef] = useState("");
  const [acceptanceCriteria, setAcceptanceCriteria] = useState("");
  const [dependencyTarget, setDependencyTarget] = useState("");
  const [restoringCheckpoint, setRestoringCheckpoint] = useState("");

  useEffect(() => {
    setArgumentError("");
    setTitle(node?.title || "");
    setInstruction(node?.instruction_summary || "");
    setExecutorRef(node?.executor_ref || "");
    setAcceptanceCriteria(node?.acceptance_criteria.join("\n") || "");
    setDependencyTarget("");
  }, [node?.id, node?.title, node?.instruction_summary, node?.executor_ref, node?.acceptance_criteria.join("\n")]);

  const capability=capabilities?.items.find(c=>`capability://${c.id}`===executorRef);
  const isCapability=executorRef.startsWith("capability://");
  const availability = useMemo(
    () => getExecutorAvailability(executorRef.trim() || null,capability),
    [executorRef,capability],
  );
  const stale =
    saveError?.code === "stale_revision" || saveError?.code === "stale_view_revision";

  if (!node) {
    return (
      <Panel className="task-world-inspector h-full min-h-0 overflow-y-auto">
        <EmptyState
          title="选择一个节点"
          description="从画布或执行轨迹选择节点，查看状态并编辑任务语义。"
          className="py-16"
        />
      </Panel>
    );
  }

  const latestExecution = node.latest_execution;
  const execution_status = latestExecution?.status;
  const evidenceSource = latestExecution ? {
    kind: "node" as const,
    graph_id: graphId,
    node_id: node.id,
    execution_id: latestExecution.execution_id,
  } : undefined;

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const input: Record<string, unknown> = {
      ...capabilityDraft.input,
      instruction: instruction.trim(),
      acceptance_criteria: acceptanceCriteria
        .split("\n")
        .map((value) => value.trim())
        .filter(Boolean),
    };
    if (executorRef.trim()) input.executor_ref = executorRef.trim(); else delete input.executor_ref;
    if(isCapability){
      if(capabilityDraft.loading||capabilityDraft.error)return;
      try{
        if(hasHeaderBinding(capability?.input_schema))throw new Error(HEADER_UNSUPPORTED);
        input.capability_input=parseArguments(capabilityDraft.argumentsJson);setArgumentError("");
      }catch(error){setArgumentError(error instanceof Error?error.message:"参数无效");return;}
    } else delete input.capability_input;
    await onSave({
      kind: node.kind,
      title: title.trim(),
      input,
      retry_policy: node.retry_policy || { max_attempts: 1 },
    });
  };

  return (
    <Panel className="task-world-inspector h-full min-h-0 overflow-y-auto" data-testid="task-world-inspector">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-meta font-semibold uppercase tracking-[0.14em] text-[var(--text-muted)]">
            选中节点
          </p>
          <h2 className="mt-1 truncate text-base font-semibold text-[var(--text)]">{node.title}</h2>
        </div>
        <Badge tone={execution_status === "failed" || node.status === "failed" ? "danger" : node.isRunning ? "info" : "default"}>
          {execution_status ? executionStatusLabel(execution_status) : node.status}
        </Badge>
      </div>

      <dl className="mt-4 grid grid-cols-2 gap-2 text-xs">
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2">
          <dt className="text-[var(--text-faint)]">当前图版本</dt>
          <dd className="mt-1 font-mono text-[var(--text)]">r{expectedRevision}</dd>
        </div>
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2">
          <dt className="text-[var(--text-faint)]">尝试次数</dt>
          <dd className="mt-1 font-mono text-[var(--text)]">{latestExecution?.attempt ?? node.state.attempts}</dd>
        </div>
      </dl>

      {stale && (
        <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] px-3 py-2 text-xs text-[var(--warning-fg)]" role="alert">
          图已被其他窗口更新，请先刷新后再保存；本次修改未覆盖最新语义。
        </p>
      )}
      {saveError && !stale && (
        <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-fg)]" role="alert">
          {saveError.message}
        </p>
      )}

      <form className="mt-4 space-y-3" onSubmit={(event) => void submit(event)}>
        <fieldset disabled={graphLocked || saving || (isCapability&&capabilityDraft.loading)} className="space-y-3">
        <BasicSection
          title={title}
          onTitleChange={setTitle}
          instruction={instruction}
          onInstructionChange={setInstruction}
        />
        <ExecutorSection
          executorRef={executorRef}
          onExecutorRefChange={setExecutorRef}
          availability={availability}
          capabilities={capabilities}
          onSelect={tool=>{setExecutorRef(`capability://${tool.id}`);capabilityDraft.setArgumentsJson("{}");setArgumentError("");}}
        />
        {isCapability&&<>
          {capabilityDraft.loading?<p role="status">正在读取已保存参数…</p>:<SchemaForm schema={capability?.input_schema} value={capabilityDraft.argumentsJson} onChange={capabilityDraft.setArgumentsJson}/>}
          {(argumentError||capabilityDraft.error)&&<p role="alert" className="text-xs text-[var(--danger-fg)]">{argumentError||capabilityDraft.error}</p>}
        </>}
        <details className="studio-detail"><summary>验收标准</summary>
        <AcceptanceSection
          acceptanceCriteria={acceptanceCriteria}
          onAcceptanceCriteriaChange={setAcceptanceCriteria}
        />
        </details>
        <Button type="submit" size="sm" disabled={saving || !title.trim() || (isCapability&&(capabilityDraft.loading||!!capabilityDraft.error||availability.kind!=="configured"))}>
          {saving ? "保存中…" : "保存语义"}
        </Button>
        </fieldset>
      </form>
      {graphLocked && <p className="mt-2 text-xs text-[var(--text-faint)]">有任务执行尚未结束，请先完成或取消执行后再修改任务图。</p>}

      {latestExecution?.status==="waiting_approval"&&isCapability&&<Link className="mt-3 block text-sm underline" to="/system?card=approvals">前往审批中心 · 任务画布</Link>}
      <details className="studio-detail"><summary>状态与执行记录</summary>
      <StateSection node={node} />

      <ExecutionSection
        node={node}
        saving={saving}
        graphLocked={graphLocked}
        onStartExecution={onStartExecution}
        onCancelExecution={onCancelExecution}
        onRerun={onRerun}
      />

      </details>
      <details className="studio-detail"><summary>资源与产物</summary>
      <ResourceSection graphId={graphId} nodeId={node.id} />

      <ArtifactSection
        source={evidenceSource}
        completed={latestExecution?.status === "succeeded"}
      />

      </details>
      {node.kind === "approval" && (
        <section className="mt-5 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs">
          <p className="font-medium text-[var(--warning-fg)]">等待审批</p>
          <p className="mt-1 leading-5 text-[var(--text-secondary)]">审批动作沿用对话中的既有审批入口，不在 Task World 内创建第二套审批状态。</p>
          <Link className="mt-2 inline-flex text-[var(--accent-primary)] hover:underline" to="/chat">
            打开既有审批入口
          </Link>
        </section>
      )}

      <details className="studio-detail"><summary>依赖关系</summary>
      <DependencySection
        node={node}
        dependencyEdges={dependencyEdges}
        dependencyCandidates={dependencyCandidates}
        dependencyTarget={dependencyTarget}
        onDependencyTargetChange={setDependencyTarget}
        onAddDependency={onAddDependency}
        onRemoveDependency={onRemoveDependency}
        saving={saving}
        graphLocked={graphLocked}
      />

      </details>
      <details className="studio-detail"><summary>版本与检查点</summary>
      <VersionSection
        revisions={revisions}
        checkpoints={checkpoints}
        restoringCheckpoint={restoringCheckpoint}
        onRestoringCheckpointChange={setRestoringCheckpoint}
        onCheckpoint={onCheckpoint}
        onRestore={onRestore}
        saving={saving}
        graphLocked={graphLocked}
      />
      </details>
      {!isCapability&&<ExtensionSlots/>}
    </Panel>
  );
}
