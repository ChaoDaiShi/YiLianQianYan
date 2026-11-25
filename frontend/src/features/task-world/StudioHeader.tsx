import { ArrowLeft } from "lucide-react";
import type { ReactNode } from "react";
import Identifier from "../../components/Identifier";
import PageShell from "../../components/layout/PageShell";

export default function StudioHeader({ graphId, graphRevision, viewRevision, pending, error, onRetry, onBack, backLabel = "工作流列表", children }: {
  graphId: string;
  graphRevision: number;
  viewRevision: number | null;
  pending: boolean;
  error: string | null;
  onRetry: () => void;
  onBack: () => void;
  backLabel?: string;
  children: ReactNode;
}) {
  const saveState = error ? "failed" : pending ? "saving" : viewRevision === null ? "loading" : "synced";
  const saveLabel = {
    failed: "画布保存失败",
    saving: "正在保存画布…",
    loading: "正在载入画布…",
    synced: "视图已同步",
  }[saveState];

  return <PageShell.Header className="studio-header" title={<>
    <button type="button" className="studio-back" aria-label={backLabel} title={`返回${backLabel}`} onClick={onBack}><ArrowLeft size={18} /></button>
    <h1>任务画布</h1>
  </>} actions={<>
    {children}
    <details className="studio-technical-info">
      <summary>技术信息</summary>
      <dl>
        <dt>任务图 ID</dt><dd><Identifier kind="graph" value={graphId} /></dd>
        <dt>语义修订</dt><dd>r{graphRevision}</dd>
        <dt>视图修订</dt><dd>{viewRevision === null ? "尚未载入" : `r${viewRevision}`}</dd>
      </dl>
    </details>
    <div className="studio-save" data-save-state={saveState}>
      <span className="studio-save-dot" aria-hidden="true" />
      <span role="status">{saveLabel}</span>
      {error && <button type="button" title={error} onClick={onRetry}>重试保存画布</button>}
    </div>
  </>} />;
}
