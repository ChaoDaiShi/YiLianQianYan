import { ArrowLeft } from "lucide-react";
import Identifier from "../../components/Identifier";
import { formatIdentifier } from "../../utils/identifiers";

export default function StudioHeader({graphId,revision,pending,error,onRetry,onBack}: {
  graphId:string; revision:number|null; pending:boolean; error:string|null;
  onRetry:()=>void; onBack:()=>void;
}) {
  return <header className="studio-header">
    <button type="button" className="studio-back" aria-label="任务中心" title="返回任务中心" onClick={onBack}><ArrowLeft size={18}/></button>
    <div className="studio-identity"><span className="studio-eyebrow">忆涟 · Studio</span><h1 aria-label={`任务图 ${formatIdentifier("graph", graphId)}`}><Identifier kind="graph" value={graphId} /></h1></div>
    <div className="studio-save" role="status" data-save-state={error?"failed":pending?"saving":revision===null?"loading":"synced"}>
      <span className="studio-save-dot" aria-hidden="true"/>
      <span>{error?"保存失败":pending?"正在保存…":revision===null?"正在载入…":`视图已同步 · r${revision}`}</span>
      {error && <button type="button" onClick={onRetry}>重试保存画布</button>}
    </div>
  </header>;
}
