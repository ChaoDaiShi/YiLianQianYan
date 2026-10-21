import { poolRequest, type ProviderAccount } from "../llm/modelPools";
export type BasicServiceKind="vision"|"image";
export interface BasicServiceProfile {id:string;label:string;provider_account_id:string;service_kind:BasicServiceKind;model:string;adapter:"openai"|"dashscope"|"minimax";parameters:Record<string,unknown>;revision:number;test:{revision:number;ok:boolean;tested_at:string;message:string}|null}
export interface BasicServiceResult {profile_id:string;service_kind:BasicServiceKind;text?:string;image_url?:string;usage?:unknown}
export const listBasicServices=()=>poolRequest<BasicServiceProfile[]>("GET","/api/service-profiles");
export const listBasicAccounts=()=>poolRequest<ProviderAccount[]>("GET","/api/provider-accounts");
export const testBasicService=(id:string,prompt:string,image_url?:string)=>poolRequest<BasicServiceResult>("POST",`/api/service-profiles/${id}/test`,{prompt,image_url});
