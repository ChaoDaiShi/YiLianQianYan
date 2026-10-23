import {requestResult} from "../core/api/http";
import {ingestResource} from "./resources";
export interface KnowledgeDocument{id:string;resource_id:string;name:string;mime_type:string;hash:string;size:number;char_count:number;chunk_count:number;created_at:number}
export interface KnowledgeCollection{id:string;name:string;document_count:number;path_count:number;created_at:number}
export interface KnowledgeMembership{collection_id:string;collection_name:string;document_id:string;relative_path:string}
export interface KnowledgeImportLocation{collection_id:string;relative_path:string}
export interface KnowledgeList{documents:KnowledgeDocument[];collections:KnowledgeCollection[];memberships:KnowledgeMembership[]}
export interface KnowledgeChunk{memory_id:string;document_id:string;document_name:string;chunk_index:number;content:string}
export interface KnowledgeSearchHit{document_id:string;document_name:string;content:string;start_char:number}
export interface KnowledgeDetail{memberships:KnowledgeMembership[];build_id:string|null;document:KnowledgeDocument;chunks:KnowledgeChunk[];text:string}
async function call<T>(method:string,path:string,body?:unknown):Promise<T>{const result=await requestResult<T>(method,path,body);if(!result.ok)throw new Error(result.error);return result.data;}
export const listKnowledge=(collectionId?:string)=>call<KnowledgeList>("GET",`/api/knowledge/documents${collectionId?`?${new URLSearchParams({collection_id:collectionId})}`:""}`);
export const ensureKnowledgeCollection=(name:string)=>call<{collection:KnowledgeCollection}>("POST","/api/knowledge/collections",{name});
export const getKnowledge=(id:string)=>call<KnowledgeDetail>("GET",`/api/knowledge/documents/${encodeURIComponent(id)}`);
export const searchKnowledge=(query:string,collectionId?:string)=>call<{hits:KnowledgeSearchHit[]}>("GET",`/api/knowledge/search?${new URLSearchParams({q:query,...(collectionId?{collection_id:collectionId}:{})})}`);
export const deleteKnowledge=(id:string)=>call<{deleted:boolean}>("DELETE",`/api/knowledge/documents/${encodeURIComponent(id)}`);
export async function importKnowledge(file:File,location?:KnowledgeImportLocation){if(file.size>25*1024*1024)throw new Error("文件超过25 MiB上传上限");const resource=await ingestResource(file);if(!resource)throw new Error("文件上传失败，请检查文件格式和大小");return call<{document:KnowledgeDocument;created:boolean}>("POST","/api/knowledge/documents",{resource_id:resource.id,...(location?{location}:{})});}
