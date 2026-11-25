import {useState} from "react";
import {Button,Modal} from "../../components/ui";
import type {BrowserTab} from "../../api/browser";
export default function BrowserTools({tab,zoom,busy,error,onZoom,onFind,onBookmark,onClear,onClose}:{tab?:BrowserTab;zoom:number;busy:boolean;error:string;onZoom:(factor:number)=>void;onFind:()=>void;onBookmark:()=>void;onClear:()=>Promise<void>;onClose:()=>void}) {
 const [confirmClear,setConfirmClear]=useState(false),[understood,setUnderstood]=useState(false);
 return <Modal open title="浏览器工具" onClose={onClose}>
  <div className="browser-dialog-body">
   <p className="text-sm">{tab?tab.ephemeral?"当前网页：临时浏览，关闭后丢弃登录状态。":"当前网页：保留浏览数据，支持网站允许的持续登录。网站退出登录、凭据过期或仅使用会话 Cookie 时，重开应用仍可能需要重新登录。":"打开网页后可使用缩放、查找和清除数据。"}</p>
   <div className="browser-tool-actions"><Button variant="secondary" size="sm" aria-label="缩小网页" disabled={!tab||busy||zoom<=0.25} onClick={()=>onZoom(Math.max(0.25,Math.round((zoom-0.1)*100)/100))}>−</Button><span role="status">{Math.round(zoom*100)}%</span><Button variant="secondary" size="sm" aria-label="放大网页" disabled={!tab||busy||zoom>=5} onClick={()=>onZoom(Math.min(5,Math.round((zoom+0.1)*100)/100))}>＋</Button><Button variant="ghost" size="sm" disabled={!tab||busy} onClick={()=>onZoom(1)}>重置缩放</Button></div>
   <div className="flex flex-wrap gap-2"><Button size="sm" variant="secondary" disabled={!tab||busy} onClick={onFind}>在网页中查找</Button><Button size="sm" variant="secondary" disabled={!tab} onClick={onBookmark}>收藏当前页面</Button></div>
   <p className="text-sm">网页上传请点击网页自己的文件选择按钮，手动选择需要上传的文件。下载请求会出现在“网页请求”中，由你选择保存位置。</p>
   {error&&<p role="alert" className="browser-tool-error">{error}</p>}
   {!confirmClear?<Button size="sm" variant="danger" disabled={!tab||busy} onClick={()=>setConfirmClear(true)}>清除当前浏览数据…</Button>:<div className="browser-clear-confirm"><p className="text-sm">{tab?.ephemeral?"清除此临时会话的网站登录、存储与缓存，并重新加载网页。":"清除此项目（未关联项目时为个人浏览环境）的网页数据，并重新加载相关标签页。同一环境共享登录的其他普通标签页也会退出账号。"}此操作不可撤销。</p><label className="text-sm flex gap-2"><input type="checkbox" checked={understood} onChange={event=>setUnderstood(event.target.checked)}/>我理解网站登录和本地网站数据将被清除</label><div className="flex gap-2"><Button size="sm" variant="danger" disabled={!understood||busy} onClick={()=>void onClear()}>确认清除浏览数据</Button><Button size="sm" variant="secondary" disabled={busy} onClick={()=>{setConfirmClear(false);setUnderstood(false);}}>取消</Button></div></div>}
  </div>
 </Modal>;
}
