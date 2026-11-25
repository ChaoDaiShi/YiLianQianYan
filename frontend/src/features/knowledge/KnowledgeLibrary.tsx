import WorkspaceIcon from "../../components/ui/WorkspaceIcon";
import {useCallback,useEffect,useMemo,useRef,useState} from "react";
import {useSearchParams} from "react-router-dom";
import {Download,FileText,FolderOpen,Plus,RefreshCw,Sparkles,Trash2,Upload} from "lucide-react";
import {Button,ConfirmDialog,EmptyState,ErrorState,Input,Modal} from "../../components/ui";
import {deleteKnowledge,ensureKnowledgeCollection,getKnowledge,listKnowledge,searchKnowledge,type KnowledgeDocument,type KnowledgeDetail,type KnowledgeSearchHit,type KnowledgeCollection,type KnowledgeMembership} from "../../api/knowledge";
import {getResourceContent} from "../../api/resources";
import KnowledgeBuildPanel from "./KnowledgeBuildPanel";
import KnowledgeImportDialog from "./KnowledgeImportDialog";
import KnowledgeDocumentReader from "./KnowledgeDocumentReader";
import {anchorForSearchHit,knowledgeSourceParams,readKnowledgeAnchor} from "./knowledgeAnchor";
import {KNOWLEDGE_FILE_FORMATS,useKnowledgeImport} from "./useKnowledgeImport";
import "./knowledgeLibrary.css";
const message=(error:unknown)=>error instanceof Error?error.message:String(error);
export default function KnowledgeLibrary(){
 const[params,setParams]=useSearchParams();
 const sourceAnchor=useMemo(()=>readKnowledgeAnchor(params),[params]);
 const[buildOpen,setBuildOpen]=useState(false);const[buildId,setBuildId]=useState<string|undefined>();
 const[collectionId,setCollectionId]=useState("");const[collections,setCollections]=useState<KnowledgeCollection[]>([]);const[memberships,setMemberships]=useState<KnowledgeMembership[]>([]);
 const[newCollectionOpen,setNewCollectionOpen]=useState(false),[collectionName,setCollectionName]=useState(""),[creatingCollection,setCreatingCollection]=useState(false),[collectionError,setCollectionError]=useState("");
 const[documents,setDocuments]=useState<KnowledgeDocument[]>([]);const[loading,setLoading]=useState(true);const[listError,setListError]=useState("");
 const[query,setQuery]=useState("");const[hits,setHits]=useState<KnowledgeSearchHit[]>([]);const[searching,setSearching]=useState(false);const[searchError,setSearchError]=useState("");
 const[removeError,setRemoveError]=useState("");
 const[importPickerOpen,setImportPickerOpen]=useState(false);
 const[selected,setSelected]=useState<string|null>(()=>params.get("document"));const[detail,setDetail]=useState<KnowledgeDetail|null>(null);const[detailError,setDetailError]=useState("");const[remove,setRemove]=useState<KnowledgeDocument|null>(null);
 const fileInput=useRef<HTMLInputElement>(null);const folderInput=useRef<HTMLInputElement>(null);const mounted=useRef(true);
 const refreshVersion=useRef(0);
 const refresh=useCallback(async()=>{
  const version=++refreshVersion.current;setListError("");setLoading(true);
  try{const result=await listKnowledge(collectionId||undefined);if(mounted.current&&version===refreshVersion.current){setDocuments(result.documents);setCollections(result.collections);setMemberships(result.memberships);}}
  catch(cause){if(mounted.current&&version===refreshVersion.current)setListError(message(cause));}
  finally{if(mounted.current&&version===refreshVersion.current)setLoading(false);}
 },[collectionId]);
 const imports=useKnowledgeImport(refresh,collectionId);
 const{importing,addFiles}=imports;
 const searchQuery=query.trim();
 const resultsLoading=searchQuery ? searching : loading;
 const resultsError=searchQuery ? searchError : listError;

 function importFiles(files: File[]) {
  if (!files.length || importing) return;
  setImportPickerOpen(false);
  void addFiles(files);
 }

 function closeImport() {
  setImportPickerOpen(false);
  imports.setOpen(false);
 }
 const membershipByDocument=useMemo(()=>{
  const result=new Map<string,KnowledgeMembership[]>();
  for(const membership of memberships){const previous=result.get(membership.document_id)||[];previous.push(membership);result.set(membership.document_id,previous);}
  return result;
 },[memberships]);
 useEffect(()=>{mounted.current=true;folderInput.current?.setAttribute("webkitdirectory","");void refresh();return()=>{mounted.current=false;};},[refresh]);
 useEffect(() => {
  setHits([]);
  setSearchError("");
  setSearching(Boolean(searchQuery));
  if (!searchQuery) return;

  let active = true;
  const timer = window.setTimeout(() => {
   void searchKnowledge(searchQuery, collectionId || undefined)
    .then(result => { if (active) setHits(result.hits); })
    .catch(cause => { if (active) setSearchError(message(cause)); })
    .finally(() => { if (active) setSearching(false); });
  }, 220);
  return () => { active = false; window.clearTimeout(timer); };
 }, [searchQuery, documents, collectionId]);
 useEffect(()=>{setSelected(params.get("document"));},[params]);
 useEffect(()=>{let active=true;setDetail(null);setDetailError("");if(selected)void getKnowledge(selected, false).then(result=>{if(active)setDetail(result);}).catch(cause=>{if(active)setDetailError(message(cause));});return()=>{active=false;};},[selected]);
 async function download(){if(!detail)return;try{const blob=await getResourceContent(detail.document.resource_id);const url=URL.createObjectURL(blob);const link=document.createElement("a");link.href=url;link.download=detail.document.name;link.click();window.setTimeout(()=>URL.revokeObjectURL(url),1000);}catch(cause){setDetailError(message(cause));}}
 async function createCollection(){if(!collectionName.trim()||creatingCollection)return;setCreatingCollection(true);setCollectionError("");try{const result=await ensureKnowledgeCollection(collectionName.trim());setCollectionId(result.collection.id);setNewCollectionOpen(false);setCollectionName("");await refresh();}catch(cause){setCollectionError(message(cause));}finally{setCreatingCollection(false);}}
 return <div className="knowledge-library knowledge-collections-layout" onDragOver={event=>event.preventDefault()} onDrop={event=>{event.preventDefault();importFiles(Array.from(event.dataTransfer.files));}}>
  <aside className="knowledge-collections" aria-label="知识集合"><h2>我的集合</h2><button type="button" aria-current={!collectionId?"page":undefined} onClick={()=>setCollectionId("")}><WorkspaceIcon name="knowledge-base" size={20}/><span>全部资料</span></button>{collections.map(collection=><button type="button" key={collection.id} aria-current={collectionId===collection.id?"page":undefined} onClick={()=>setCollectionId(collection.id)}><FolderOpen size={17}/><span>{collection.name}</span><small>{collection.document_count}</small></button>)}<Button variant="ghost" onClick={()=>{setCollectionError("");setNewCollectionOpen(true);}}><Plus size={15}/>新建集合</Button></aside>
  <div className="knowledge-collection-content">
   <div className="knowledge-library-toolbar">
    <Button size="sm" variant="secondary" className="knowledge-new-collection-mobile" onClick={()=>{setCollectionError("");setNewCollectionOpen(true);}}><Plus size={15}/>新建集合</Button>
    <Button size="sm" onClick={()=>importing ? imports.setOpen(true) : setImportPickerOpen(true)}><Upload size={15}/>{importing ? "查看导入进度" : "导入资料"}</Button>
    <Button size="sm" variant="secondary" onClick={()=>{setBuildId(undefined);setBuildOpen(true);}}><Sparkles size={15}/>AI 构建知识</Button>
    {!importing && imports.progress.total > 0 && <Button size="sm" variant="ghost" onClick={()=>imports.setOpen(true)}>查看导入结果</Button>}
    <Button size="sm" iconOnly variant="ghost" onClick={()=>void refresh()} aria-label="刷新知识库"><RefreshCw size={15}/></Button>
    <select aria-label="知识分类" className="knowledge-collection-filter" value={collectionId} onChange={event=>setCollectionId(event.target.value)}><option value="">全部资料</option>{collections.map(collection=><option key={collection.id} value={collection.id}>{collection.name}（{collection.document_count}）</option>)}</select>
    <Input aria-label="搜索知识内容" placeholder="搜索文档内容或名称…" value={query} onChange={event=>setQuery(event.target.value)}/>
    <input ref={fileInput} type="file" accept={KNOWLEDGE_FILE_FORMATS} multiple hidden aria-label="选择知识文档" onChange={event=>{importFiles(Array.from(event.target.files??[]));event.target.value="";}}/>
    <input ref={folderInput} type="file" multiple hidden aria-label="选择知识文件夹" onChange={event=>{importFiles(Array.from(event.target.files??[]));event.target.value="";}}/>
   </div>
   {!loading && !listError && <div className="knowledge-library-count"><WorkspaceIcon name="knowledge-base" size={20}/>{documents.length} 个文档 · {documents.reduce((count,item)=>count+item.chunk_count,0)} 个知识片段</div>}
   {removeError && <p role="alert" className="knowledge-library-error">{removeError}</p>}
   <KnowledgeImportDialog
    open={imports.open}
    pickerOpen={importPickerOpen}
    onChooseFiles={()=>fileInput.current?.click()}
    onChooseFolder={()=>folderInput.current?.click()}
    onClose={closeImport}
    importing={importing}
    progress={imports.progress}
    onStop={imports.requestStop}
   />
   <div className="knowledge-library-scroll">
    {resultsLoading ? <p role="status">{searchQuery ? "正在检索…" : "正在读取知识库…"}</p> : resultsError ?
     <ErrorState
      title={searchQuery ? "知识检索失败" : "知识库暂时无法加载"}
      description={resultsError}
      className="break-words"
     />
    : searchQuery ? (
     <section aria-label="知识检索结果">
      {hits.length ? hits.map(hit=><button className="knowledge-search-hit" key={`${hit.document_id}:${hit.start_char}`} onClick={()=>setParams(knowledgeSourceParams(anchorForSearchHit(hit)))}><strong>{hit.document_name} · 正文摘录（第 {hit.start_char+1} 个字符起）</strong><p>{hit.content}</p></button>) : <EmptyState title="没有匹配的知识内容"/>}
     </section>
    ) : documents.length ? (
     <div className="knowledge-documents">{documents.map(item=><article className="knowledge-document" key={item.id}><FileText size={22}/><button className="knowledge-document-open" onClick={()=>setSelected(item.id)}><strong>{item.name}</strong><span>{item.char_count.toLocaleString()} 字符 · {item.chunk_count} 个知识片段 · {new Date(item.created_at).toLocaleDateString()}</span>{(membershipByDocument.get(item.id)||[]).filter(membership=>!collectionId||membership.collection_id===collectionId).map(membership=><small className="knowledge-document-folders" key={`${membership.collection_id}:${membership.relative_path}`}>{membership.relative_path}</small>)}</button><Button size="sm" variant="ghost" aria-label={`移出知识库 ${item.name}`} onClick={()=>{setRemoveError("");setRemove(item);}}><Trash2 size={15}/></Button></article>)}</div>
    ) : <EmptyState icon={<WorkspaceIcon name="knowledge-base" size={28}/>} title="导入第一份知识文档" description="使用上方的导入入口，或将资料拖入此处。"/>}
   </div>
  <Modal open={Boolean(selected)} onClose={()=>{setSelected(null);setParams({});}} title={detail?.document.name??"知识文档"} size="lg" footer={detail&&<>{detail.build_id&&<Button variant="secondary" onClick={()=>{setBuildId(detail.build_id!);setSelected(null);setBuildOpen(true);}}>查看知识结构与来源</Button>}<Button variant="secondary" onClick={()=>void download()}><Download size={15}/>下载原文件</Button></>}>
   {detailError?<p role="alert" className="knowledge-library-error">{detailError}</p>:detail?<KnowledgeDocumentReader key={detail.document.id} detail={detail} anchor={sourceAnchor}/>:<p role="status">正在读取文档内容…</p>}
  </Modal>
  <ConfirmDialog open={Boolean(remove)} title="移出资料库" description={`将移除“${remove?.name??""}”的提取正文、切片与索引。原文件保留，引用此资料的知识条目会显示“来源已移除”。`} confirmLabel="移出资料库" tone="danger" onCancel={()=>setRemove(null)} onConfirm={async()=>{if(!remove)return;try{await deleteKnowledge(remove.id);setRemove(null);if(selected===remove.id){setSelected(null);setParams({});}await refresh();}catch(cause){setRemoveError(message(cause));setRemove(null);}}}/>
  <KnowledgeBuildPanel initialBuildId={buildId} open={buildOpen} onClose={()=>setBuildOpen(false)} onImported={id=>{setBuildOpen(false);setParams({document:id});setSelected(id);void refresh();}}/>
 <Modal open={newCollectionOpen} onClose={()=>{if(!creatingCollection)setNewCollectionOpen(false);}} title="新建知识集合" footer={<Button disabled={creatingCollection||!collectionName.trim()} onClick={()=>void createCollection()}>{creatingCollection?"创建中…":"创建集合"}</Button>}><Input label="集合名称" value={collectionName} onChange={event=>setCollectionName(event.target.value)} placeholder="例如：项目资料"/>{collectionError&&<p role="alert" className="knowledge-library-error">{collectionError}</p>}</Modal></div></div>;
}
