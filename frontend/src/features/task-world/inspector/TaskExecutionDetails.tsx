import { useCallback, useEffect, useState } from "react";
import { getTaskGraph, getTaskNodeExecutionEvidence, reviewTaskExecutionNoEffect, type ExternalEffectReview, type TaskNodeExecutionSummary } from "../../../api/taskWorld";
import { listWorkflowRuns } from "../../../api/workflows";
import { Button, ErrorState, Modal, Textarea } from "../../../components/ui";
import WorkflowRunInspector from "../../../components/workflow/WorkflowRunInspector";
import { executionStatusLabel, isActiveExecution } from "../taskGraphProjection";

type OutputState = { value: unknown; error?: string };
type WorkflowState = { runId: string | null; error?: string };

export default function TaskExecutionDetails({ graphId, nodeId, execution, onClose }: {
  graphId: string;
  nodeId: string;
  execution: TaskNodeExecutionSummary;
  onClose: () => void;
}) {
  const [workflow, setWorkflow] = useState<WorkflowState | null>(null);
  const [output, setOutput] = useState<OutputState | null>(null);
  const [showWorkflow, setShowWorkflow] = useState(false);
  const [retry, setRetry] = useState(0);
  const [reviewNote, setReviewNote] = useState("");
  const [confirmedNoEffect, setConfirmedNoEffect] = useState(false);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState("");
  const [reviewed, setReviewed] = useState(false);
  const [loadedExecution, setLoadedExecution] = useState<TaskNodeExecutionSummary | null>(null);
  const [loadedForVersion, setLoadedForVersion] = useState("");
  const [review, setReview] = useState<ExternalEffectReview | null>(null);
  const executionVersion = `${execution.updated_at}:${execution.status}:${execution.failure_code ?? ""}:${execution.validation?.checked_at ?? ""}`;
  const shown = loadedExecution?.execution_id === execution.execution_id && loadedForVersion === executionVersion ? loadedExecution : execution;
  const { execution_id: id, executor_ref: executor, status } = shown;
  const isWorkflow = executor?.startsWith("workflow://");
  const hasStoredOutput = status === "succeeded" || status === "stale";
  const needsExternalReview = !reviewed && ["execution_interrupted", "external_mutation_uncertain", "cancelled_external_mutation_uncertain", "proofloop_budget_exhausted_result_unknown"].includes(shown.failure_code ?? "");

  useEffect(() => {
    setLoadedExecution(null); setReview(null); setReviewed(false);
    setReviewNote(""); setConfirmedNoEffect(false); setReviewError("");
  }, [execution.execution_id]);

  async function recordReview() {
    setReviewBusy(true); setReviewError("");
    const graph = await getTaskGraph(graphId);
    if (!graph.ok) { setReviewError(graph.error.message); setReviewBusy(false); return; }
    const result = await reviewTaskExecutionNoEffect(graphId, id, graph.data.revision, reviewNote.trim());
    if (result.ok) { setReviewed(true); setLoadedExecution(result.data); setLoadedForVersion(executionVersion); setRetry(value => value + 1); }
    else setReviewError(result.error.message);
    setReviewBusy(false);
  }

  useEffect(() => {
    if (!isWorkflow || workflow?.runId) return;
    let current = true;
    let timer: number | undefined;
    async function readChild() {
      const result = await listWorkflowRuns({ parent_execution_id: id, limit: 2 });
      if (!current) return;
      if (!result.ok) setWorkflow({ runId: null, error: result.error });
      else if (result.data.length > 1) setWorkflow({ runId: null, error: "这次执行关联了多条运行记录，请在工作流历史中核对。" });
      else {
        const runId = result.data[0]?.run_id ?? null;
        setWorkflow({ runId });
        if (!runId && isActiveExecution(status)) timer = window.setTimeout(() => void readChild(), 1000);
      }
    }
    void readChild();
    return () => { current = false; window.clearTimeout(timer); };
  }, [isWorkflow, id, status, retry, workflow?.runId]);

  useEffect(() => {
    let current = true;
    void getTaskNodeExecutionEvidence(graphId, nodeId, id).then(result => {
      if (!current) return;
      if (result.ok) {
        setOutput({ value: result.data.output });
        setReview(result.data.review);
        if (result.data.execution) { setLoadedExecution(result.data.execution); setLoadedForVersion(executionVersion); }
      } else setOutput({ value: null, error: result.error.message });
    });
    return () => { current = false; };
  }, [graphId, nodeId, id, retry, executionVersion]);

  const backToAttempt = useCallback(() => setShowWorkflow(false), []);
  const runId = workflow?.runId;
  const readAgain = () => { setRetry(value => value + 1); };

  return <Modal open size="xl" title={showWorkflow ? "关联工作流运行" : `第 ${execution.attempt} 次执行`} onClose={onClose} closeDisabled={reviewBusy}
    footer={showWorkflow ? <Button variant="secondary" disabled={reviewBusy} onClick={backToAttempt}>返回任务执行</Button> : <Button variant="secondary" disabled={reviewBusy} onClick={onClose}>关闭</Button>}>
    {showWorkflow && runId ? <WorkflowRunInspector runId={runId} onClose={backToAttempt} allowCancel={false} /> :
      <div className="min-w-0 space-y-4 text-sm">
        <p role="status">{executionStatusLabel(status)}{executor && <span className="ml-2 break-all text-[var(--text-secondary)]">{executor}</span>}</p>
        {!reviewed && shown.error && <p className="whitespace-pre-wrap break-words text-[var(--danger-fg)]">{shown.error}</p>}
        {output?.error && !hasStoredOutput && <ErrorState title="无法读取执行证据" description={output.error} onRetry={readAgain} />}
        {reviewed && <p role="status">已保存人工核验记录。此操作不会重新执行任务；可关闭详情后手动重新准备该步骤。</p>}
        {needsExternalReview && <section className="space-y-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3">
          <h4 className="font-medium">外部结果核验</h4>
          <p>请先查看对应应用、服务回执或目标文件。结果仍不确定时保持当前状态；确认未产生效果后，可记录依据以允许手动重新准备。</p>
          <Textarea label="核验依据" value={reviewNote} maxLength={1000} rows={3} disabled={reviewBusy}
            placeholder="填写核对的对象、回执或文件，以及确认未执行的依据" onChange={event => setReviewNote(event.target.value)} />
          <label className="flex items-start gap-2"><input type="checkbox" checked={confirmedNoEffect} disabled={reviewBusy} onChange={event => setConfirmedNoEffect(event.target.checked)} /><span>我已在外部核实，这次操作没有产生效果</span></label>
          {reviewError && <p role="alert" className="text-[var(--danger-fg)]">{reviewError}</p>}
          <Button disabled={reviewBusy || !confirmedNoEffect || !reviewNote.trim()} onClick={() => void recordReview()}>{reviewBusy ? "正在保存…" : "保存核验记录"}</Button>
        </section>}
        {review && <section className="space-y-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3">
          <h4 className="font-medium">用户核验陈述</h4>
          <p className="text-[var(--text-secondary)]">来源：用户陈述。原始工具输出和执行状态单独保留。</p>
          <p className="whitespace-pre-wrap break-words">{review.note}</p>
          {review.previous_error && <details><summary>原不确定记录</summary><p className="whitespace-pre-wrap break-words">{review.previous_error}</p></details>}
        </section>}
        {shown.validation?.issues?.map(issue => <p key={issue} className="whitespace-pre-wrap break-words">{issue}</p>)}
        {isWorkflow && <section className="space-y-2">
          <h4 className="font-medium">关联工作流</h4>
          {workflow?.error ? <ErrorState title="无法读取关联运行" description={workflow.error} onRetry={readAgain} /> :
            runId ? <Button size="sm" variant="secondary" onClick={() => setShowWorkflow(true)}>查看工作流节点和结果</Button> :
              <p role="status" className="text-[var(--text-secondary)]">{!workflow ? "正在读取关联运行…" : isActiveExecution(status) ? "正在等待创建运行记录" : "此次尝试未产生工作流运行记录"}</p>}
        </section>}
        {hasStoredOutput && <section className="space-y-2">
          <h4 className="font-medium">本次输出</h4>
          {status === "stale" && <p className="text-[var(--warning-fg)]">此输出来自已失效的历史尝试。</p>}
          {!output ? <p role="status">正在读取输出…</p> : output.error ?
            <ErrorState title="无法读取执行输出" description={output.error} onRetry={readAgain} /> : output.value == null ?
              <p className="text-[var(--text-secondary)]">本次执行未保留输出。</p> :
              <pre className="select-text whitespace-pre-wrap break-all rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-3 text-xs">
                {typeof output.value === "string" ? output.value : JSON.stringify(output.value, null, 2)}
              </pre>}
        </section>}
      </div>}
  </Modal>;
}
