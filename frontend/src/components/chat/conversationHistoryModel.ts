import type { ConversationOrigin, ConversationSummary } from "../../types";
import type { ConversationFamily } from "../../features/connectors/messages/messageForest";

export function originLabel(origin?: ConversationOrigin) {
  if (origin?.kind !== "channel") return "桌面";
  return ({onebot11:"QQ",wechat_ilink:"微信",feishu:"飞书"} as Record<string,string>)[origin.provider || ""] || "渠道";
}
export function originDescription(origin?:ConversationOrigin){
  const label=originLabel(origin);
  const name=origin?.channel_name;
  return name?(name.startsWith(label)?name:`${label} · ${name}`):`${label}渠道`;
}
export type HistoryEntry = ConversationSummary & { pathCount: number };
export function buildHistoryEntries(rows: ConversationSummary[], families: ConversationFamily[], query = ""): HistoryEntry[] {
  const byId=new Map(rows.map(row=>[row.id,row]));
  const membership=new Map(families.flatMap(f=>f.conversationIds.map(id=>[id,f] as const)));
  const keyword=query.trim().toLocaleLowerCase();
  return rows.filter(row=>!membership.has(row.id)||membership.get(row.id)!.id===row.id).flatMap(row=>{
    const members=membership.get(row.id)?.conversationIds.map(id=>byId.get(id)).filter((x):x is ConversationSummary=>!!x)||[row];
    if(keyword&&!members.some(member=>(member.title+" "+originLabel(member.origin)+" "+(member.origin?.channel_name||"")).toLocaleLowerCase().includes(keyword)))return [];
    return [{...row,pathCount:membership.get(row.id)?.conversationIds.length||1,updated_at:Math.max(...members.map(m=>m.updated_at||m.created_at))}];
  }).sort((a,b)=>b.updated_at-a.updated_at);
}
