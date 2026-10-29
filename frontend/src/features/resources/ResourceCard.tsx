import { useEffect, useState } from "react";
import { File, FileText, Image as ImageIcon, Loader2, Maximize2, X } from "lucide-react";
import { getResourceContent } from "../../api/resources";
import ResourcePreviewDialog from "./ResourcePreviewDialog";
import type { AttachmentReference } from "../../types";
import "./resourceCards.css";

const isImage=(file:AttachmentReference)=>file.mime_type?.startsWith("image/")||/\.(png|jpe?g)$/i.test(file.name);

export function ResourceCard({file,onRemove,status,error}:{file:AttachmentReference;onRemove?:()=>void;status?:string;error?:string}){
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
  return <div className="resource-card-shell" data-kind={image?"image":"file"}>
    <button type="button" className="resource-card" data-file-type={file.name.split(".").pop()?.toLowerCase()} aria-label={`预览附件 ${file.name}`} disabled={!file.id} onClick={()=>setOpen(true)}>
      <span className="resource-card-visual" data-kind={image?"image":"file"}>
        {url?<img src={url} alt={file.name} loading="lazy"/>:<Icon size={30} strokeWidth={1.4}/>}
        {image&&file.id&&!url&&!imageError&&<Loader2 className="resource-card-spinner" size={18}/>}
        <span className="resource-card-expand"><Maximize2 size={15}/></span>
      </span>
      <span className="resource-card-caption"><Icon size={17}/><span title={file.name}>{file.name}</span></span>
      {status&&<span className="resource-card-status">{status}</span>}
    </button>
    {onRemove&&<button type="button" className="resource-card-remove" aria-label={`移除 ${file.name}`} onClick={onRemove}><X size={13}/></button>}
    {(error||imageError)&&<p className="resource-card-error" role="alert">{error||imageError}</p>}
    {open&&<ResourcePreviewDialog file={file} source={{kind:"resource",id:file.id}} onClose={()=>setOpen(false)}/>}
  </div>;
}

export function ResourceCards({files}:{files:AttachmentReference[]}){
  return files.length?<div className="resource-card-grid" aria-label="消息附件">{files.map(file=><ResourceCard key={file.id} file={file}/>)}</div>:null;
}
