import { useEffect, useState } from "react";
import { File, FileText, Image as ImageIcon, Loader2, Maximize2, RotateCcw, X } from "lucide-react";
import { getResourceContent } from "../../api/resources";
import ResourcePreviewDialog from "./ResourcePreviewDialog";
import type { AttachmentReference } from "../../types";
import "./resourceCards.css";

const isImage=(file:AttachmentReference)=>file.mime_type?.startsWith("image/")||/\.(png|jpe?g|gif|webp)$/i.test(file.name);

export function ResourceCard({file,onRemove,onRetry,actionsDisabled,status,error,upload}:{
  file:AttachmentReference; onRemove?:()=>void; onRetry?:()=>void; actionsDisabled?:boolean; status?:string; error?:string;
  upload?: { loaded?: number; total?: number };
}){
  const image=!!isImage(file);
  const [url,setUrl]=useState(""); const [imageError,setImageError]=useState(""); const [open,setOpen]=useState(false);
  useEffect(()=>{
    setUrl("");setImageError("");
    if(!image||!file.id)return;
    const abort=new AbortController();let objectUrl="";
    void getResourceContent(file.id,abort.signal).then(bytes=>{if(!abort.signal.aborted){objectUrl=URL.createObjectURL(bytes);setUrl(objectUrl);}}).catch(cause=>{if(!abort.signal.aborted)setImageError(cause instanceof Error?cause.message:"图片加载失败");});
    return()=>{abort.abort();if(objectUrl)URL.revokeObjectURL(objectUrl);};
  },[file.id,image]);
  const Icon=image?ImageIcon:/\.docx$/i.test(file.name)?FileText:File;
  const progress = upload?.loaded !== undefined && upload.total ? Math.round(100 * upload.loaded / upload.total) : undefined;
  return <div className="resource-card-shell" data-kind={image?"image":"file"}>
    <button type="button" className="resource-card" data-file-type={file.name.split(".").pop()?.toLowerCase()} aria-label={`预览附件 ${file.name}`} disabled={!file.id} onClick={()=>setOpen(true)}>
      <span className="resource-card-visual" data-kind={image?"image":"file"}>
        {url&&!imageError?<img src={url} alt={file.name} loading="lazy" onError={() => setImageError("图片缩略图无法显示，可尝试打开原文件预览")}/>:<Icon size={30} strokeWidth={1.4}/>}
        {image&&file.id&&!url&&!imageError&&<Loader2 className="resource-card-spinner" size={18}/>}
        <span className="resource-card-expand"><Maximize2 size={15}/></span>
      </span>
      <span className="resource-card-caption"><Icon size={17}/><span title={file.name}>{file.name}</span></span>
      {status&&<span className="resource-card-status">{status}{progress !== undefined ? ` ${progress}%` : ""}</span>}
    </button>
    {upload && <progress className="resource-card-progress" aria-label={`上传 ${file.name}`} max={100} value={progress}/>}
    {onRemove&&<button type="button" className="resource-card-remove" aria-label={`移除 ${file.name}`} disabled={actionsDisabled} onClick={onRemove}><X size={13}/></button>}
    {(error||imageError)&&<p className="resource-card-error" role="alert">{error||imageError}</p>}
    {onRetry && <button type="button" className="resource-card-retry" aria-label={`重试 ${file.name}`} disabled={actionsDisabled} onClick={onRetry}><RotateCcw size={12} />重试</button>}
    {open&&<ResourcePreviewDialog file={file} source={{kind:"resource",id:file.id}} onClose={()=>setOpen(false)}/>}
  </div>;
}

export function ResourceCards({files}:{files:AttachmentReference[]}){
  return files.length?<div className="resource-card-grid" aria-label="消息附件">{files.map(file=><ResourceCard key={file.id} file={file}/>)}</div>:null;
}
