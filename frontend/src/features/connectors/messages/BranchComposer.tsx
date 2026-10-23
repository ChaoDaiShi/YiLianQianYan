import { useEffect, type RefObject } from "react";
import { GitBranch, MessageSquare, Send, Square, X } from "lucide-react";
import { Button } from "../../../components/ui";
import type { ConnectorMessage } from "../../../api/connectorMessages";

export default function BranchComposer({parent,draft,onChange,onSend,onStop,onCancel,busy,disabled,inputRef,conversationTitle}:{parent?:ConnectorMessage;draft:string;onChange:(text:string)=>void;onSend:()=>void;onStop:()=>void;onCancel?:()=>void;busy:boolean;disabled?:boolean;inputRef:RefObject<HTMLTextAreaElement>;conversationTitle?:string}){
  useEffect(()=>{if(parent&&!busy)inputRef.current?.focus();},[parent?.id]);
  return <form className="message-branch-composer branch-composer-card" aria-label={parent?"创建分支对话":"画布新提问"} onSubmit={event=>{event.preventDefault();onSend();}}>
    <header className="branch-composer-heading"><span className="branch-composer-icon">{parent?<GitBranch size={18}/>:<MessageSquare size={18}/>}</span><div><strong>{parent?"从这条回答展开新分支":"开始一条对话"}</strong><p>{conversationTitle||"当前消息画布"} · {parent?"新问题将沿用此前的对话上下文":"提问和回答会依次出现在画布中"}</p></div>{onCancel&&<button type="button" className="branch-composer-close" aria-label="取消创建分支" disabled={busy} onClick={onCancel}><X size={17}/></button>}</header>
    {parent&&<blockquote className="branch-composer-source"><span>源回答</span><p>{parent.content}</p></blockquote>}
    <div className="branch-composer-input"><textarea ref={inputRef} aria-label="分支对话提问" placeholder={parent?"写下新的问题，探索另一种思路…":"输入你的问题…"} value={draft} onChange={event=>onChange(event.target.value)} disabled={busy||disabled} rows={2} onKeyDown={event=>{if((event.ctrlKey||event.metaKey)&&event.key==="Enter"&&!event.nativeEvent.isComposing&&!busy&&!disabled&&draft.trim()){event.preventDefault();onSend();}}}/>
      <div className="message-branch-send"><span>Ctrl + Enter 发送</span>{busy?<Button type="button" variant="danger" size="sm" onClick={onStop}><Square size={14}/>停止生成</Button>:<Button type="submit" size="sm" disabled={disabled||!draft.trim()}><Send size={14}/>{parent?"发送到新分支":"发送提问"}</Button>}</div>
    </div>
  </form>;
}
