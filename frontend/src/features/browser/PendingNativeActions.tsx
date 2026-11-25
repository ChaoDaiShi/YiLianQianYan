import {useState} from "react";
import {Button, ErrorState, Modal} from "../../components/ui";
import {resolveBrowserRequest, type BrowserUiRequest} from "../../api/browser";

const labels={popup:"打开新窗口",download:"下载文件",permission:"网站权限"};
export default function PendingNativeActions({requests,onClose,onResolved}:{requests:BrowserUiRequest[];onClose:()=>void;onResolved:()=>Promise<void>}) {
 const [pending,setPending]=useState(""),[error,setError]=useState(""),[status,setStatus]=useState("");
 async function resolve(request:BrowserUiRequest,decision:"open"|"save"|"allow"|"deny") {
  setPending(request.id);setError("");setStatus("");
  try {
   const reply=await resolveBrowserRequest(request.id,decision);
   if(reply.cancelled){setStatus("已取消保存，下载请求仍保留，可重新选择位置。");return;}
   if(reply.saved)setStatus("已选择保存位置，下载已开始；完成状态以系统下载结果为准。");
   await onResolved();
  } catch(cause){setError(cause instanceof Error?cause.message:String(cause));}
  finally{setPending("");}
 }
 return <Modal open title="网页请求" size="lg" onClose={onClose}>
  <p className="text-sm mb-3">仅处理下列明确请求。允许本次操作不会永久开放网站权限。</p>
  {error&&<ErrorState title="网页请求未完成" description={error}/>}
  {status&&<p role="status" className="text-sm">{status}</p>}
  {!requests.length?<p role="status">没有待处理的网页请求。</p>:requests.map(request=><article className="browser-site-row" key={request.id}>
   <strong>{labels[request.kind]}</strong><p>{request.message}</p>
   {request.url&&<p className="browser-request-url">目标：{request.url}</p>}
   {request.suggested_name&&<p>文件：{request.suggested_name}</p>}
   <div className="flex flex-wrap gap-2">
    <Button size="sm" disabled={!!pending} loading={pending===request.id} onClick={()=>void resolve(request,request.kind==="popup"?"open":request.kind==="download"?"save":"allow")}>{request.kind==="popup"?"打开页面":request.kind==="download"?"选择保存位置":"允许本次"}</Button>
    <Button size="sm" variant="secondary" disabled={!!pending} onClick={()=>void resolve(request,"deny")}>{request.kind==="popup"?"忽略":request.kind==="download"?"拒绝下载":"拒绝"}</Button>
   </div>
  </article>)}
 </Modal>;
}
