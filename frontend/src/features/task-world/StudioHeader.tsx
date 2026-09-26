import { ArrowLeft, ListTree, Settings2, Ellipsis, Upload } from "lucide-react";
export type StudioPanel = "trail" | "inspector" | "tools" | null;

export default function StudioHeader({graphId,revision,pending,error,onRetry,onBack,panel,onPanel}: {
  graphId:string; revision:number|null; pending:boolean; error:string|null;
  onRetry:()=>void; onBack:()=>void; panel:StudioPanel; onPanel:(panel:StudioPanel)=>void;
}) {
  const toggle=(next:StudioPanel)=>onPanel(panel===next?null:next);
  return <header className="studio-header">
    <button type="button" className="studio-back" aria-label="任务中心" title="返回任务中心" onClick={onBack}><ArrowLeft size={18}/></button>
    <div className="studio-identity"><span className="studio-eyebrow">忆涟 · Studio</span><h1 title={graphId}>{graphId}</h1></div>
    <div className="studio-save" role="status" data-save-state={error?"failed":pending?"saving":revision===null?"loading":"synced"}>
      <span className="studio-save-dot" aria-hidden="true"/>
      <span>{error?"保存失败":pending?"正在保存…":revision===null?"正在载入…":`视图已同步 · r${revision}`}</span>
      {error && <button type="button" onClick={onRetry}>重试保存画布</button>}
    </div>
    <nav className="studio-header-actions" aria-label="工作台面板">
      <button type="button" aria-expanded={panel==="trail"} aria-controls="studio-trail" onClick={()=>toggle("trail")}><ListTree size={16}/>执行轨迹</button>
      <button type="button" aria-expanded={panel==="inspector"} aria-controls="studio-inspector" onClick={()=>toggle("inspector")}><Settings2 size={16}/>节点属性</button>
      <button type="button" aria-expanded={panel==="tools"} aria-controls="studio-tools" onClick={()=>toggle("tools")}><Ellipsis size={18}/>更多</button>
    </nav>
    <button type="button" className="studio-publish" disabled title="发布能力尚未接入，本轮仅预留入口"><Upload size={15}/>发布 · 未接入</button>
  </header>;
}
