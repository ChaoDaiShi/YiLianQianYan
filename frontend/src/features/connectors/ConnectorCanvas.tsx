import { useEffect,useMemo,useRef,useState } from "react";
import { ReactFlow,Background,Controls,Handle,Position,MarkerType,applyNodeChanges,useNodesInitialized,useReactFlow,useStore,type NodeProps,type Node,type NodeChange,type Connection,type Viewport } from "@xyflow/react";
import { Boxes,Settings2 } from "lucide-react";
import WorkspaceIcon,{type WorkspaceIconName} from "../../components/ui/WorkspaceIcon";
import type {ConnectorView,ConnectorNode,ConnectorViewport} from "../../api/connectors";
import {visibleConnectorNodes,planConnectorDeletion} from "./connectorModel";
import {connectorBindingHandles,connectorPort,isConnectorConnection} from "./connectorPorts";
import SettingsIcon,{settingsIconName} from "../settings/components/SettingsIcon";
import "@xyflow/react/dist/style.css";
type NodeData={showLabels:boolean;density:string;item:ConnectorNode;onEdit:(node:ConnectorNode)=>void};type FlowNode=Node<NodeData,"connector">;
const resourceIcons:Partial<Record<string,WorkspaceIconName>>={agent:"agent",conversation:"conversation",constraint:"security",memory:"memory-record",knowledge:"knowledge-base",model:"models-services",tool:"tool-catalog",plugin:"extensions",skill:"skill",workflow:"task-flow",mcp:"mcp"};
function ResourceNode({data,selected}:NodeProps<FlowNode>){const node=data.item;const settingIcon=node.kind.startsWith("setting_")||node.entity_id==="settings-global"?settingsIconName(node.entity_id):undefined;const resourceIcon=node.kind.startsWith("setting_")||node.entity_id==="settings-global"?undefined:resourceIcons[node.kind];const Icon=node.kind.startsWith("setting_")||node.entity_id==="settings-global"?Settings2:Boxes;const port=connectorPort(node.kind);const mutable=node.entity_id!=="settings-global"&&!node.kind.startsWith("setting_");
 return <div className={`connector-node ${node.kind}`} data-selected={selected} aria-expanded={node.settings_expanded} data-settings-group={node.entity_id.startsWith("group:")?node.entity_id.slice(6):undefined} style={{width:data.density==="comfortable"?260:220,padding:data.density==="comfortable"?20:14}} role="button" tabIndex={0} aria-label={`编辑${node.label}`} onKeyDown={event=>{if(event.key==="Enter"||event.key===" "){event.preventDefault();data.onEdit(node);}}}>
 {node.kind==="agent"?<><Handle isConnectable={mutable} id="constraint-in" type="target" position={Position.Top} title="约束"/><Handle isConnectable={mutable} id="context-out" type="source" position={Position.Bottom} title="记忆与知识"/><Handle isConnectable={mutable} id="conversation-in" type="target" position={Position.Left} title="对话"/><Handle isConnectable={mutable} id="capability-out" type="source" position={Position.Right} title="工具、模型与能力"/></>:port==="top"?<Handle id="constraint-out" type="source" position={Position.Bottom} isConnectable={false}/>:port==="bottom"?<Handle isConnectable={mutable} id="context-in" type="target" position={Position.Top}/>:port==="left"?<Handle isConnectable={mutable} id="conversation-out" type="source" position={Position.Right}/>:<Handle isConnectable={mutable} id="capability-in" type="target" position={Position.Left}/>}{node.kind.startsWith("setting_")&&<><Handle id="setting-out" type="source" position={port==="left"?Position.Left:Position.Right} isConnectable={false}/><Handle id="setting-in" type="target" position={port==="left"?Position.Right:Position.Left} isConnectable={false}/></>}<div className="connector-node-title">{settingIcon?<SettingsIcon name={settingIcon}/>:resourceIcon?<WorkspaceIcon name={resourceIcon} size={24} variant="mono"/>:<Icon/>}{data.showLabels&&<span>{node.label}</span>}</div><p>{node.entity_id==="settings-global"?"应用配置 · 点击管理设置节点":node.settings_expanded!==undefined?(node.settings_expanded?"点击收起设置项":"点击展开设置项"):node.kind.startsWith("setting_")?"设置子功能 · 点击配置":node.kind==="agent"?"上约束 · 下记忆 · 左对话 · 右能力":node.kind==="constraint"?"权限与资源范围":node.kind==="model"?(node.model_source==="pool"?"分级模型池 · 点击管理":"模型档案 · 点击修改绑定"):"点击查看与修改"}</p></div>;
}
const nodeTypes={connector:ResourceNode};
const initialFitOptions={maxZoom:1,padding:.14};
type FocusRequest={revision:number;nodeIds:string[]};
function ConnectorCamera({expectedNodes,layoutKey,focusRequest}:{expectedNodes:FlowNode[];layoutKey?:string;focusRequest?:FocusRequest}){
 const {fitView,viewportInitialized}=useReactFlow<FlowNode>();
 const initialized=useNodesInitialized();
 const applied=useRef<{layoutKey?:string;revision?:number}>({layoutKey,revision:undefined});
 const explicit=focusRequest&&focusRequest.revision!==applied.current.revision;
 const categories=expectedNodes.filter(node=>node.data.item.entity_id.startsWith("group:"));
 const overview=categories.length?expectedNodes.filter(node=>node.data.item.kind==="agent"||node.data.item.entity_id.startsWith("group:")):expectedNodes;
 const targetIds=explicit?focusRequest.nodeIds:overview.map(node=>node.id);
 const targetKey=targetIds.join("\0");
 const expected=new Map(expectedNodes.map(node=>[node.id,node]));
 const ready=useStore(state=>state.width>0&&state.height>0&&targetIds.length>0&&targetIds.every(id=>{
  const actual=state.nodeLookup.get(id);const projected=expected.get(id);
  return !!actual&&!!projected&&!!actual.measured.width&&!!actual.measured.height&&actual.position.x===projected.position.x&&actual.position.y===projected.position.y;
 }));
 useEffect(()=>{
  if(!viewportInitialized||!initialized||!ready||(applied.current.layoutKey===layoutKey&&applied.current.revision===focusRequest?.revision))return;
  let cancelled=false;
  void fitView({nodes:targetIds.map(id=>({id})),padding:.18,maxZoom:1,minZoom:.25,duration:220}).then(success=>{
   if(success&&!cancelled)applied.current={layoutKey,revision:focusRequest?.revision};
  });
  return()=>{cancelled=true;};
 },[viewportInitialized,initialized,ready,layoutKey,focusRequest?.revision,targetKey,fitView]);
 return null;
}
export default function ConnectorCanvas({view,onEdit,onConnect,onSave,onHide,onUnbind,layoutKey,focusRequest}:{view:ConnectorView;onEdit:(node:ConnectorNode)=>void;onConnect:(connection:Connection)=>void;onSave:(nodes:Array<{id:string;x:number;y:number}>,viewport:ConnectorViewport)=>void;onHide:(ids:string[])=>void;onUnbind:(ids:string[])=>void;layoutKey?:string;focusRequest?:{revision:number;nodeIds:string[]}}){
 const initialView=useMemo(()=>({viewport:view.viewport,fit:view.viewport.x===0&&view.viewport.y===0&&view.viewport.zoom===1}),[view.canvas_id]);
 const measured=useRef<Record<string,{width:number;height:number}>>({});
 const projected=useMemo(()=>visibleConnectorNodes(view.nodes,view.display).map(item=>({id:item.id,type:"connector" as const,position:{x:item.x,y:item.y},measured:measured.current[item.id],draggable:item.kind!=="constraint"&&item.entity_id!=="settings-global",deletable:item.kind!=="agent",data:{item,onEdit,showLabels:view.display.show_labels,density:view.display.density}})),[view,onEdit]);
 const [selectedEdgeIds,setSelectedEdgeIds]=useState<string[]>([]);
 const [nodes,setNodes]=useState<FlowNode[]>(projected);const nodesRef=useRef(nodes);nodesRef.current=nodes;const viewportRef=useRef(view.viewport);
 useEffect(()=>{setNodes(projected);nodesRef.current=projected;},[projected]);
 const ids=new Set(nodes.map(n=>n.id));const edges=view.display.show_edges?view.bindings.filter(e=>ids.has(e.source_id)&&ids.has(e.target_id)).map(e=>({id:e.id,source:e.source_id,target:e.target_id,...connectorBindingHandles(e),selected:selectedEdgeIds.includes(e.id),markerEnd:{type:MarkerType.ArrowClosed,color:"var(--accent-purple)"},deletable:!e.id.startsWith("derived:"),style:{stroke:"var(--accent-purple)",strokeWidth:1.5},type:"default"})):[];
 const changed=(changes:NodeChange<FlowNode>[])=>{for(const change of changes){if(change.type==="dimensions"&&change.dimensions)measured.current[change.id]=change.dimensions;}setNodes(current=>{const next=applyNodeChanges(changes,current);nodesRef.current=next;return next;});};
 const persist=(viewport:Viewport)=>{viewportRef.current=viewport;onSave(nodesRef.current.map(n=>({id:n.id,...n.position})),viewport);};
 return <div className="connector-canvas h-full w-full"><ReactFlow key={view.canvas_id} nodes={nodes} edges={edges} nodeTypes={nodeTypes} onNodesChange={changed} onEdgesChange={changes=>setSelectedEdgeIds(current=>{let next=current;for(const change of changes){if(change.type==="select")next=change.selected?[...new Set([...next,change.id])]:next.filter(id=>id!==change.id);else if(change.type==="remove")next=next.filter(id=>id!==change.id);}return next;})} onNodeClick={(_,node)=>onEdit(node.data.item)} isValidConnection={connection=>isConnectorConnection(connection,view.nodes)} onConnect={onConnect} onDelete={({nodes:removed,edges:removedEdges})=>{const deletion=planConnectorDeletion(removed.map(node=>({id:node.id,kind:node.data.item.kind})),removedEdges);if(deletion.hiddenNodeIds.length)onHide(deletion.hiddenNodeIds);if(deletion.unboundEdgeIds.length)onUnbind(deletion.unboundEdgeIds);}} onNodeDragStop={(_,node)=>{const updated=nodes.map(n=>n.id===node.id?node:n);setNodes(updated);onSave(updated.map(n=>({id:n.id,...n.position})),viewportRef.current);}} onMoveEnd={(_,viewport)=>persist(viewport)} defaultViewport={initialView.viewport} fitView={initialView.fit} fitViewOptions={initialFitOptions} minZoom={.25} maxZoom={2} deleteKeyCode={["Delete","Backspace"]} proOptions={{hideAttribution:true}}><ConnectorCamera expectedNodes={projected} layoutKey={layoutKey} focusRequest={focusRequest}/><Background color="var(--accent-border)" gap={22}/><Controls showInteractive={false}/></ReactFlow></div>;
}
