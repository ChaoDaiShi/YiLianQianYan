import {useEffect,useRef} from "react";
import {ArrowDown,ArrowUp,X} from "lucide-react";
import {Button} from "../../components/ui";
import type {BrowserFindResult} from "../../api/browser";
export default function BrowserFindBar({query,result,busy,error,onQuery,onFind,onClose}:{query:string;result:BrowserFindResult|null;busy:boolean;error:string;onQuery:(query:string)=>void;onFind:(backwards:boolean)=>void;onClose:()=>void}) {
 const input=useRef<HTMLInputElement>(null);
 useEffect(()=>{input.current?.focus();},[]);
 return <form className="browser-find-bar" onSubmit={event=>{event.preventDefault();onFind(false);}} onKeyDown={event=>{if(event.key==="Escape"){event.preventDefault();onClose();}}}>
  <input ref={input} className="ui-input" aria-label="在网页中查找" value={query} onChange={event=>onQuery(event.target.value)} placeholder="在网页中查找"/>
  <Button iconOnly size="sm" variant="ghost" aria-label="查找上一项" disabled={!query||busy} onClick={()=>onFind(true)}><ArrowUp size={15}/></Button>
  <Button iconOnly size="sm" variant="ghost" aria-label="查找下一项" type="submit" disabled={!query||busy}><ArrowDown size={15}/></Button>
  <span role="status">{error|| (result&&result.query===query?(result.matched?`${result.active_match_index} / ${result.match_count}`:"没有匹配项"):"")}</span>
  <Button iconOnly size="sm" variant="ghost" aria-label="关闭网页查找" onClick={onClose}><X size={15}/></Button>
 </form>;
}
