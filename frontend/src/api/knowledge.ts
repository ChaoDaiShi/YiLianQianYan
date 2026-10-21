import {requestResult} from "../core/api/http";
import {ingestResource} from "./resources";
export interface KnowledgeDocument{id:string;resource_id:string;name:string;mime_type:string;hash:string;size:number;char_count:number;chunk_count:number;created_at:number}
export interface KnowledgeChunk{memory_id:string;document_id:string;document_name:string;chunk_index:number;content:string}
export interface KnowledgeSearchHit{document_id:string;document_name:string;content:string;start_char:number}
export interface KnowledgeDetail{document:KnowledgeDocument;chunks:KnowledgeChunk[];text:string}
async function call<T>(method:string,path:string,body?:unknown):Promise<T>{const result=await requestResult<T>(method,path,body);if(!result.ok)throw new Error(result.error);return result.data;}
export const listKnowledge=()=>call<{documents:KnowledgeDocument[]}>("GET","/api/knowledge/documents");
export const getKnowledge=(id:string)=>call<KnowledgeDetail>("GET",`/api/knowledge/documents/${encodeURIComponent(id)}`);
export const searchKnowledge=(query:string)=>call<{hits:KnowledgeSearchHit[]}>("GET",`/api/knowledge/search?${new URLSearchParams({q:query})}`);
export const deleteKnowledge=(id:string)=>call<{deleted:boolean}>("DELETE",`/api/knowledge/documents/${encodeURIComponent(id)}`);
export async function importKnowledge(file:File){const resource=await ingestResource(file);if(!resource)throw new Error("文件上传失败，请检查文件格式和大小");return call<{document:KnowledgeDocument;created:boolean}>("POST","/api/knowledge/documents",{resource_id:resource.id});}
