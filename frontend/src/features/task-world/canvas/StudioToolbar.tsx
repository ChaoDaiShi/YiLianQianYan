import { Maximize, Minus, Plus, Play, LayoutGrid } from "lucide-react";
import { useReactFlow, useViewport } from "@xyflow/react";
import { Button } from "../../../components/ui";

export default function StudioToolbar({onFit,onReset,onLayout,onAdd,onAddCapability,onRun,canAdd,canRun,runLabel}: {
  onFit:()=>void; onReset:()=>void; onLayout:()=>void; onAdd:()=>void; onAddCapability?:()=>void;
  onRun:()=>void; canAdd:boolean; canRun:boolean; runLabel:string;
}) {
  const {zoomIn,zoomOut}=useReactFlow();
  const {zoom}=useViewport();
  return <div className="studio-toolbar" role="toolbar" aria-label="画布布局与镜头">
    <button type="button" aria-label="Zoom Out" title="缩小" onClick={()=>void zoomOut()}><Minus size={16}/></button>
    <output aria-label="当前缩放" className="studio-zoom">{Math.round(zoom*100)}%</output>
    <button type="button" aria-label="Zoom In" title="放大" onClick={()=>void zoomIn()}><Plus size={16}/></button>
    <span className="studio-tool-divider" aria-hidden="true"/>
    <button type="button" aria-label="适应全部" title="适应全部" onClick={onFit}><Maximize size={16}/></button>
    <button type="button" onClick={onReset}>100%</button>
    <button type="button" aria-label="自动布局" title="自动布局（仅显式操作）" onClick={onLayout}><LayoutGrid size={16}/></button>
    <span className="studio-tool-divider" aria-hidden="true"/>
    <Button variant="secondary" size="sm" disabled={!canAdd} onClick={onAdd}><Plus size={15}/>添加任务节点</Button>
    {onAddCapability&&<Button variant="secondary" size="sm" disabled={!canAdd} onClick={onAddCapability}>从能力添加</Button>}
    <Button size="sm" disabled={!canRun} title={runLabel} onClick={onRun}><Play size={15}/>运行选中节点</Button>
  </div>;
}
