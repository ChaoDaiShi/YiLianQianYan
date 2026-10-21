import { useEffect,useState } from "react";
import { listLlmModels } from "../../api/system";
import type { AgentEvent } from "../../api/chat";
import { listProviderAccounts,poolRequest } from "./modelPools";
interface ModelLabel {id:string;label:string;provider:string}
const reasonLabels:Record<string,string>={network:"连接失败",timeout:"连接超时",rate_limit:"请求限流",quota:"额度不足",authentication:"认证失败",model_unavailable:"模型不可用",service_error:"服务错误"};
export const modelRouteReasonLabel=(reason:string)=>reasonLabels[reason]??"候选不可用";
export default function ModelRouteNotice({route}:{route:AgentEvent|null}){
 const[models,setModels]=useState<ModelLabel[]>([]);useEffect(()=>{let active=true;void Promise.all([listLlmModels(),poolRequest<Array<{id:string;label:string;provider_account_id:string}>>("GET","/api/service-profiles"),listProviderAccounts()]).then(([chat,services,accounts])=>{if(active)setModels([...(chat??[]),...services.map(profile=>({id:profile.id,label:profile.label,provider:accounts.find(account=>account.id===profile.provider_account_id)?.provider_id??"厂商信息不可用"}))]);}).catch(()=>{if(active)setModels([]);});return()=>{active=false;};},[route?.to_profile_id,route?.from_profile_id]);
 if(!route)return null;const model=models.find(m=>m.id===route.to_profile_id);const from=models.find(m=>m.id===route.from_profile_id);const label=model?`${model.provider} / ${model.label}`:"模型档案信息暂不可用";
 const message=route.phase==="exhausted"?"本轮模型池的所有候选均不可用":route.phase==="switched"?`已${route.from_tier!==route.to_tier?"降级":"切换"}到第 ${route.to_tier} 级 · ${label}`:route.phase==="recovered"?`已恢复第 ${route.to_tier} 级 · ${label}`:`当前模型 · 第 ${route.to_tier} 级 · ${label}`;
 return <section className="connector-model-route mx-4 mt-2" role="status"><strong>{message}</strong>{route.reason&&<p className="mt-1">{from?.label??"上个候选"}：{modelRouteReasonLabel(route.reason)}</p>}{route.interrupted_content&&<details className="mt-2"><summary>查看中断片段</summary><pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap">{route.interrupted_content}</pre></details>}</section>;
}
