import {useCallback,useEffect,useMemo,useRef,useState} from "react";
import {useSearchParams} from "react-router-dom";
import {BookOpen,Download,FileText,FolderOpen,RefreshCw,Sparkles,Trash2,Upload} from "lucide-react";
import {Button,ConfirmDialog,EmptyState,Input,Modal} from "../../components/ui";
import {deleteKnowledge,getKnowledge,listKnowledge,searchKnowledge,type KnowledgeDocument,type KnowledgeDetail,type KnowledgeSearchHit,type KnowledgeCollection,type KnowledgeMembership} from "../../api/knowledge";
import {getResourceContent} from "../../api/resources";
import KnowledgeBuildPanel from "./KnowledgeBuildPanel";
import KnowledgeImportDialog from "./KnowledgeImportDialog";
import {KNOWLEDGE_FILE_FORMATS,useKnowledgeImport} from "./useKnowledgeImport";
import "./knowledgeLibrary.css";
const message=(error:unknown)=>error instanceof Error?error.message:String(error);
export default function KnowledgeLibrary(){
 const[params,setParams]=useSearchParams();
 const[buildOpen,setBuildOpen]=useState(false);const[buildId,setBuildId]=useState<string|undefined>();
 const[collectionId,setCollectionId]=useState("");const[collections,setCollections]=useState<KnowledgeCollection[]>([]);const[memberships,setMemberships]=useState<KnowledgeMembership[]>([]);
 const[documents,setDocuments]=useState<KnowledgeDocument[]>([]);const[loading,setLoading]=useState(true);const[error,setError]=useState("");const[query,setQuery]=useState("");const[hits,setHits]=useState<KnowledgeSearchHit[]>([]);const[searching,setSearching]=useState(false);
 const[selected,setSelected]=useState<string|null>(()=>params.get("document"));const[detail,setDetail]=useState<KnowledgeDetail|null>(null);const[detailError,setDetailError]=useState("");const[remove,setRemove]=useState<KnowledgeDocument|null>(null);
 const fileInput=useRef<HTMLInputElement>(null);const folderInput=useRef<HTMLInputElement>(null);const mounted=useRef(true);
 const refreshVersion=useRef(0);
 const refresh=useCallback(async()=>{
  const version=++refreshVersion.current;setError("");setLoading(true);
  try{const result=await listKnowledge(collectionId||undefined);if(mounted.current&&version===refreshVersion.current){setDocuments(result.documents);setCollections(result.collections);setMemberships(result.memberships);}}
  catch(cause){if(mounted.current&&version===refreshVersion.current)setError(message(cause));}
  finally{if(mounted.current&&version===refreshVersion.current)setLoading(false);}
 },[collectionId]);
 const imports=useKnowledgeImport(refresh);
 const{importing,addFiles}=imports;
 const membershipByDocument=useMemo(()=>{
  const result=new Map<string,KnowledgeMembership[]>();
  for(const membership of memberships){const previous=result.get(membership.document_id)||[];previous.push(membership);result.set(membership.document_id,previous);}
  return result;
 },[memberships]);
 useEffect(()=>{mounted.current=true;folderInput.current?.setAttribute("webkitdirectory","");void refresh();return()=>{mounted.current=false;};},[refresh]);
 useEffect(()=>{let active=true;if(!query.trim()){setHits([]);setSearching(false);return;}setHits([]);setSearching(true);const timer=window.setTimeout(()=>{void searchKnowledge(query.trim(),collectionId||undefined).then(result=>{if(active){setHits(result.hits);setError("");}}).catch(cause=>{if(active){setHits([]);setError(message(cause));}}).finally(()=>{if(active)setSearching(false);});},220);return()=>{active=false;window.clearTimeout(timer);};},[query,documents,collectionId]);
 useEffect(()=>{setSelected(params.get("document"));},[params]);
 useEffect(()=>{let active=true;setDetail(null);setDetailError("");if(selected)void getKnowledge(selected).then(result=>{if(active)setDetail(result);}).catch(cause=>{if(active)setDetailError(message(cause));});return()=>{active=false;};},[selected]);
 async function download(){if(!detail)return;try{const blob=await getResourceContent(detail.document.resource_id);const url=URL.createObjectURL(blob);const link=document.createElement("a");link.href=url;link.download=detail.document.name;link.click();window.setTimeout(()=>URL.revokeObjectURL(url),1000);}catch(cause){setDetailError(message(cause));}}
 return <div className="knowledge-library" onDragOver={event=>event.preventDefault()} onDrop={event=>{event.preventDefault();void addFiles(Array.from(event.dataTransfer.files));}}>
  <div className="knowledge-library-toolbar"><Button disabled={importing} onClick={()=>fileInput.current?.click()}><Upload size={15}/>{importing?"正在导入…":"导入文档"}</Button><Button variant="secondary" disabled={importing} onClick={()=>folderInput.current?.click()}><FolderOpen size={15}/>导入文件夹</Button><Button variant="ghost" onClick={()=>void refresh()} aria-label="刷新知识库"><RefreshCw size={15}/></Button><select aria-label="知识分类" className="knowledge-collection-filter" value={collectionId} onChange={event=>setCollectionId(event.target.value)}><option value="">全部资料</option>{collections.map(collection=><option key={collection.id} value={collection.id}>{collection.name}（{collection.document_count}）</option>)}</select>{imports.progress.total>0&&<Button variant="secondary" onClick={()=>imports.setOpen(true)}>{importing?"查看导入进度":"查看导入结果"}</Button>}<Input aria-label="搜索知识内容" placeholder="搜索文档内容或名称…" value={query} onChange={event=>setQuery(event.target.value)}/><input ref={fileInput} type="file" accept={KNOWLEDGE_FILE_FORMATS} multiple hidden aria-label="选择知识文档" onChange={event=>{void addFiles(Array.from(event.target.files??[]));event.target.value="";}}/><input ref={folderInput} type="file" multiple hidden aria-label="选择知识文件夹" onChange={event=>{void addFiles(Array.from(event.target.files??[]));event.target.value="";}}/></div>
  <div className="knowledge-library-ai-entry"><div><strong>让 AI 帮你建立知识体系</strong><p>描述研究主题，检索来源、对比观点，再将整理结果导入知识库。</p></div><Button variant="secondary" onClick={()=>{setBuildId(undefined);setBuildOpen(true);}}><Sparkles size={15}/>AI 构建知识</Button></div>
  <p className="knowledge-library-hint">导入 TXT、Markdown、PDF 文本层、Word、PowerPoint、Excel 等资料。原文件保存到本地，全文分块后进入知识检索，可在对话中使用。</p>
  <div className="knowledge-library-count"><BookOpen size={16}/>{documents.length} 个文档 · {documents.reduce((count,item)=>count+item.chunk_count,0)} 个知识片段</div>
  {error&&<p role="alert" className="knowledge-library-error">{error}</p>}
  <KnowledgeImportDialog open={imports.open} onClose={()=>imports.setOpen(false)} importing={importing} progress={imports.progress} onStop={imports.requestStop}/>
  <div className="knowledge-library-scroll">{query.trim()?<section aria-label="知识检索结果">{searching?<p role="status">正在检索…</p>:hits.length?hits.map(hit=><button className="knowledge-search-hit" key={`${hit.document_id}:${hit.start_char}`} onClick={()=>setSelected(hit.document_id)}><strong>{hit.document_name} · 正文摘录（第 {hit.start_char+1} 个字符起）</strong><p>{hit.content}</p></button>):<EmptyState title="没有匹配的知识内容"/>}</section>:loading?<p role="status">正在读取知识库…</p>:documents.length?<div className="knowledge-documents">{documents.map(item=><article className="knowledge-document" key={item.id}><FileText size={22}/><button className="knowledge-document-open" onClick={()=>setSelected(item.id)}><strong>{item.name}</strong><span>{item.char_count.toLocaleString()} 字符 · {item.chunk_count} 个知识片段 · {new Date(item.created_at).toLocaleDateString()}</span>{(membershipByDocument.get(item.id)||[]).filter(membership=>!collectionId||membership.collection_id===collectionId).map(membership=><small className="knowledge-document-folders" key={`${membership.collection_id}:${membership.relative_path}`}>{membership.relative_path}</small>)}</button><Button size="sm" variant="ghost" aria-label={`移出知识库 ${item.name}`} onClick={()=>setRemove(item)}><Trash2 size={15}/></Button></article>)}</div>:<EmptyState icon={<BookOpen size={28}/>} title="导入第一份知识文档" description="选择文件、选择文件夹，或将资料拖入此处。"/>}</div>
  <Modal open={Boolean(selected)} onClose={()=>{setSelected(null);setParams({});}} title={detail?.document.name??"知识文档"} size="lg" footer={detail&&<>{detail.build_id&&<Button variant="secondary" onClick={()=>{setBuildId(detail.build_id!);setSelected(null);setBuildOpen(true);}}>查看知识结构与来源</Button>}<Button variant="secondary" onClick={()=>void download()}><Download size={15}/>下载原文件</Button></>}>
   {detailError?<p role="alert" className="knowledge-library-error">{detailError}</p>:detail?<><p className="knowledge-library-hint">{detail.document.char_count.toLocaleString()} 字符 · {detail.document.chunk_count} 个知识片段</p>{detail.memberships.map(membership=><p className="knowledge-document-folders" key={`${membership.collection_id}:${membership.relative_path}`}>{membership.collection_name} · {membership.relative_path}</p>)}<pre className="knowledge-document-text" aria-label="知识文档全文">{detail.text}</pre></>:<p role="status">正在读取文档内容…</p>}
  </Modal>
  <ConfirmDialog open={Boolean(remove)} title="移出知识库" description={`移除“${remove?.name??""}”及其知识片段？`} confirmLabel="移除" tone="danger" onCancel={()=>setRemove(null)} onConfirm={async()=>{if(!remove)return;try{await deleteKnowledge(remove.id);setRemove(null);if(selected===remove.id)setSelected(null);await refresh();}catch(cause){setError(message(cause));setRemove(null);}}}/>
  <KnowledgeBuildPanel initialBuildId={buildId} open={buildOpen} onClose={()=>setBuildOpen(false)} onImported={id=>{setBuildOpen(false);setParams({document:id});setSelected(id);void refresh();}}/>
 </div>;
}
