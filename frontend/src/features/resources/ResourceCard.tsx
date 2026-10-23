import { useEffect, useState } from "react";
import { Download, File, FileText, Image as ImageIcon, Loader2, Maximize2, X } from "lucide-react";
import Modal from "../../components/ui/Modal";
import { getResourceContent, getResourcePreview, type ResourcePreview } from "../../api/resources";
import type { AttachmentReference } from "../../types";
import "./resourceCards.css";

const isImage=(file:AttachmentReference)=>file.mime_type?.startsWith("image/")||/\.(png|jpe?g)$/i.test(file.name);
const kindLabel=(name:string)=>/\.docx$/i.test(name)?"Word 文档":/\.pdf$/i.test(name)?"PDF 文档":/\.xlsx$/i.test(name)?"Excel 表格":/\.pptx$/i.test(name)?"PowerPoint 演示文稿":name.split(".").pop()?.toUpperCase()||"文件";

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
  return <div className="resource-card-shell">
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
    {open&&<ResourcePreviewDialog file={file} imageUrl={url} onClose={()=>setOpen(false)}/>}
  </div>;
}

export function ResourceCards({files}:{files:AttachmentReference[]}){
  return files.length?<div className="resource-card-grid" aria-label="消息附件">{files.map(file=><ResourceCard key={file.id} file={file}/>)}</div>:null;
}

function ResourcePreviewDialog({file,imageUrl,onClose}:{file:AttachmentReference;imageUrl:string;onClose:()=>void}){
  const [preview,setPreview]=useState<ResourcePreview|null>(null);const [error,setError]=useState("");const [downloading,setDownloading]=useState(false);
  useEffect(()=>{
    if(isImage(file))return;
    const abort=new AbortController();
    void getResourcePreview(file.id,abort.signal).then(setPreview).catch(cause=>{if(!abort.signal.aborted)setError(cause instanceof Error?cause.message:"无法读取预览");});
    return()=>abort.abort();
  },[file.id]);
  async function download(){setDownloading(true);try{const bytes=await getResourceContent(file.id);const href=URL.createObjectURL(bytes);const link=document.createElement("a");link.href=href;link.download=file.name;link.click();window.setTimeout(()=>URL.revokeObjectURL(href),1000);}catch(cause){setError(cause instanceof Error?cause.message:"下载失败");}finally{setDownloading(false);}}
  return <Modal open onClose={onClose} title={file.name} size="xl" footer={<button type="button" className="resource-download-button" disabled={downloading} onClick={()=>void download()}><Download size={15}/>{downloading?"正在读取原文件…":"下载原文件"}</button>}>
    <div className="resource-preview-meta">{kindLabel(file.name)}{typeof file.size==="number"?` · ${file.size<1024*1024?`${Math.ceil(file.size/1024)} KB`:`${(file.size/1024/1024).toFixed(1)} MB`}`:""}</div>
    {error&&<p role="alert" className="resource-card-error">{error}</p>}
    {isImage(file)?<div className="resource-image-preview">{imageUrl?<img src={imageUrl} alt={file.name}/>:<p role="status">图片正在加载…</p>}</div>:!preview?<p role="status">正在读取文件内容…</p>:preview.status!=="ready"?<p role="status">{preview.error||"此文件暂不支持内容预览，可下载原文件。"}</p>:<div className="resource-document-preview"><pre>{preview.text||"该文档没有可提取的正文。"}</pre>{preview.truncated&&<p>当前展示部分内容，完整内容请下载原文件。</p>}</div>}
  </Modal>;
}
