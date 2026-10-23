import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, Check, GitBranch, MessageSquare } from "lucide-react";
import { Button, Modal } from "../ui";
import type { MessageCanvasConversation } from "../../api/connectorMessages";
import type { ConversationFamily } from "../../features/connectors/messages/messageForest";
import { ResourceCards } from "../../features/resources/ResourceCard";
import { originLabel } from "./conversationHistoryModel";

export default function ConversationBranchesDialog({family,entries,activeId,onSelect,onClose}:{family:ConversationFamily;entries:MessageCanvasConversation[];activeId:string|null;onSelect:(id:string)=>void;onClose:()=>void}){
  const [selected,setSelected]=useState(activeId&&family.conversationIds.includes(activeId)?activeId:family.id);
  const paths=family.conversationIds.map(id=>entries.find(entry=>entry.id===id)).filter((entry):entry is MessageCanvasConversation=>!!entry);
  const current=paths.find(entry=>entry.id===selected);
  const parent=entries.flatMap(entry=>entry.nodes).find(node=>node.id===current?.branch?.parent_node_id);
  return <Modal open onClose={onClose} title="对话分支" size="xl" className="conversation-branches-dialog" footer={<><Link className="branch-canvas-link" to={`/connectors?conversation=${encodeURIComponent(selected)}`} onClick={onClose}><GitBranch size={15}/>在画布中查看<ArrowUpRight size={13}/></Link><Button onClick={()=>{onSelect(selected);onClose();}}>打开这条路径</Button></>}>
    <div className="branch-dialog-intro"><span className="branch-dialog-mark"><GitBranch size={21}/></span><div><h4>{family.title}</h4><p>{paths.length} 条对话路径 · 各自保留完整记录</p></div></div>
    <div className="branch-dialog-layout">
      <nav className="branch-path-list" aria-label="分支路径列表">{paths.map((path,index)=><button type="button" key={path.id} aria-pressed={selected===path.id} onClick={()=>setSelected(path.id)}><span className="branch-path-heading"><GitBranch size={14}/><strong>{path.id===family.id?"主线":`分支 ${index}`}</strong>{path.id===activeId&&<span className="branch-current"><Check size={12}/>当前</span>}</span><span className="branch-path-title">{path.title}</span><small>{originLabel(path.origin)} · {path.nodes.length} 条消息</small></button>)}</nav>
      <section className="branch-path-preview" aria-label="分支内容预览">
        {parent&&<div className="branch-parent-excerpt"><span>从这条回答分出</span><p>{parent.content}</p></div>}
        {current?.nodes.length?current.nodes.slice(-4).map(node=><article key={node.id} data-role={node.role}><span><MessageSquare size={13}/>{node.role==="user"?"我的提问":"昔涟的回答"}</span><p>{node.content||"（无正文）"}</p>{node.attachments?.length?<ResourceCards files={node.attachments}/>:null}</article>):<p className="branch-preview-empty">这条路径暂时没有新消息。</p>}
      </section>
    </div>
  </Modal>;
}
