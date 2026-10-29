import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, Check, GitBranch, MessageSquare, Plus } from "lucide-react";
import { Button, Modal } from "../ui";
import { createConversationBranch, loadConversation } from "../../api/conversations";
import type { Conversation } from "../../types";
import type { MessageCanvasConversation } from "../../api/connectorMessages";
import type { ConversationFamily } from "../../features/connectors/messages/messageForest";
import { ResourceCards } from "../../features/resources/ResourceCard";
import { originLabel } from "./conversationHistoryModel";

export default function ConversationBranchesDialog({family,entries,activeId,onSelect,onClose}:{family:ConversationFamily;entries:MessageCanvasConversation[];activeId:string|null;onSelect:(id:string)=>void;onClose:()=>void}) {
  const [selected,setSelected]=useState(activeId&&family.conversationIds.includes(activeId)?activeId:family.id);
  const [preview,setPreview]=useState<Conversation|null>(null);
  const [source,setSource]=useState("");
  const [loading,setLoading]=useState(true);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const paths=family.conversationIds.map(id=>entries.find(entry=>entry.id===id)).filter((entry):entry is MessageCanvasConversation=>!!entry);
  const current=paths.find(entry=>entry.id===selected);
  const answers=preview?.messages.filter(message=>message.role==="assistant"&&message.content&&!message.tool_calls?.length)??[];
  const parent=answers.find(message=>message.id===source);
  useEffect(()=>{
    let active=true;
    setLoading(true);setPreview(null);setSource("");setError("");
    void loadConversation(selected).then(conversation=>{
      if(!active)return;
      setPreview(conversation);
      const replies=conversation.messages.filter(message=>message.role==="assistant"&&message.content&&!message.tool_calls?.length);
      setSource(replies[replies.length-1]?.id??"");
    }).catch(cause=>{if(active)setError(cause instanceof Error?cause.message:String(cause));})
      .finally(()=>{if(active)setLoading(false);});
    return()=>{active=false;};
  },[selected]);
  async function createBranch(){
    if(!source||busy)return;
    setBusy(true);setError("");
    try{const created=await createConversationBranch(selected,source);onSelect(created.id);onClose();}
    catch(cause){setError(cause instanceof Error?cause.message:String(cause));setBusy(false);}
  }
  const running=current?.status==="running"||current?.status==="waiting_approval";
  return <Modal open onClose={()=>{if(!busy)onClose();}} title="对话分支" size="xl" className="conversation-branches-dialog" footer={<>
    <Link className="branch-canvas-link" to={`/connectors?conversation=${encodeURIComponent(selected)}`} aria-disabled={busy} onClick={event=>{if(busy)event.preventDefault();else onClose();}}><GitBranch size={15}/>查看本组画布<ArrowUpRight size={13}/></Link>
    <Button variant="secondary" disabled={busy||loading||running||!source} onClick={()=>void createBranch()}><Plus size={14}/>{busy?"正在创建…":"新建分支"}</Button>
    <Button disabled={busy} onClick={()=>{onSelect(selected);onClose();}}>打开路径继续</Button>
  </>}>
    <div className="branch-dialog-intro"><span className="branch-dialog-mark"><GitBranch size={21}/></span><div><h4>{family.title}</h4><p>{paths.length} 条路径 · 每条路径从自己的最新消息继续，历史回答可新建分支</p></div></div>
    {error&&<p className="branch-dialog-error" role="alert">{error}</p>}
    <div className="branch-dialog-layout">
      <nav className="branch-path-list" aria-label="分支路径列表">{paths.map((path,index)=><button type="button" key={path.id} disabled={busy} aria-pressed={selected===path.id} onClick={()=>setSelected(path.id)}><span className="branch-path-heading"><GitBranch size={14}/><strong>{path.id===family.id?"主线":`分支 ${index}`}</strong>{path.id===activeId&&<span className="branch-current"><Check size={12}/>当前</span>}</span><span className="branch-path-title">{path.title}</span><small>{originLabel(path.origin)} · {path.nodes.length} 条新增消息</small></button>)}</nav>
      <section className="branch-path-preview" aria-label="分支内容预览">
        {loading?<p role="status">正在读取这条路径…</p>:<>
          <label className="branch-source-picker">新分支起点<select aria-label="新分支起点" value={source} disabled={busy||running||!answers.length} onChange={event=>setSource(event.target.value)}>{!answers.length&&<option value="">这条路径还没有回答</option>}{answers.map((answer,index)=><option key={answer.id} value={answer.id}>回答 {index+1} · {answer.content.slice(0,70)}</option>)}</select></label>
          {parent&&<div className="branch-parent-excerpt"><span>从这条回答新建分支，保留此前上下文</span><p>{parent.content}</p></div>}
          {preview?.messages.slice(-4).map(node=><article key={node.id} data-role={node.role}><span><MessageSquare size={13}/>{node.role==="user"?"我的提问":"小涟的回答"}</span><p>{node.content}</p>{node.attachments?.length?<ResourceCards files={node.attachments}/>:null}</article>)}
          {!preview?.messages.length&&<p className="branch-preview-empty">这条路径暂时没有消息，打开路径后即可开始。</p>}
        </>}
      </section>
    </div>
  </Modal>;
}
