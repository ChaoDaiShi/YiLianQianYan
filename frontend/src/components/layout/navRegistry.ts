import type { WorkspaceIconName } from "../ui/WorkspaceIcon";
import { Activity, BookOpen, Bot, Boxes, Brain, FolderKanban, MessageSquare, Network, ScrollText, Settings, UserCircle, type LucideIcon } from "lucide-react";
import type { NavGroup } from "./navGroups";
export type NavGroupType = "core" | "capability" | "system";
export interface NavEntry { readonly id:string;readonly to:string;readonly label:string;readonly description:string;readonly icon:LucideIcon;readonly workspaceIcon:WorkspaceIconName;readonly group:NavGroupType;readonly primary:boolean;readonly shortcut?:string;readonly moduleId?:string;readonly keywords?:readonly string[]; }
export const NAV_ENTRIES: readonly NavEntry[] = [
 {id:"chat",workspaceIcon:"home",to:"/chat",label:"主页",description:"与小涟交谈与协作",icon:MessageSquare,group:"core",primary:true,shortcut:"Alt+1",keywords:["对话","主页","chat"]},
 {id:"connectors",workspaceIcon:"canvas",to:"/connectors",label:"画布",description:"连接对话、智能体与能力资源",icon:Network,group:"core",primary:true,shortcut:"Alt+2",keywords:["连接器","画布","connectors"]},
 {id:"workspaces",workspaceIcon:"workspace-switch",to:"/workspaces",label:"工作空间",description:"管理目录、文件与项目",icon:FolderKanban,group:"core",primary:true,shortcut:"Alt+3"},
 {id:"capabilities",workspaceIcon:"capabilities",to:"/capabilities",label:"能力",description:"Skill、工作流、MCP、工具与插件",icon:Boxes,group:"core",primary:true,shortcut:"Alt+4"},
 {id:"agents",workspaceIcon:"agent",to:"/agents",label:"智能体",description:"角色、用户提示词与能力绑定",icon:Bot,group:"capability",primary:false},
 {id:"memory",workspaceIcon:"memory-record",to:"/memory",label:"记忆",description:"管理偏好、事实与长期上下文",icon:Brain,group:"capability",primary:false},
 {id:"knowledge",workspaceIcon:"knowledge-base",to:"/knowledge",label:"知识库",description:"组织项目文档与知识来源",icon:BookOpen,group:"capability",primary:false},
 {id:"system",workspaceIcon:"monitor",to:"/system",label:"监控",description:"资源状态与 Token 用量",icon:Activity,group:"system",primary:false},
 {id:"logs",workspaceIcon:"run-history",to:"/logs",label:"日志",description:"执行与运行记录",icon:ScrollText,group:"system",primary:false},
 {id:"settings",workspaceIcon:"settings-hub",to:"/settings",label:"设置",description:"基础配置、安全控制与外观",icon:Settings,group:"system",primary:false,shortcut:"Ctrl+,"},
 {id:"account",workspaceIcon:"account",to:"/account",label:"账号",description:"身份、同步与个人偏好",icon:UserCircle,group:"system",primary:false},
];
export const DERIVED_NAV_GROUPS: readonly NavGroup[] = ([ ["核心","core"],["能力","capability"],["系统","system"] ] as const).map(([label,group])=>({label,items:NAV_ENTRIES.filter(e=>e.group===group).map(({to,id,label,icon})=>({to,id,label,icon}))}));
export function getNavEntryById(id:string){return NAV_ENTRIES.find(e=>e.id===id);}
export function getNavEntryByPath(path:string){return NAV_ENTRIES.find(e=>path===e.to||path.startsWith(e.to+"/"));}
export function orderNavigationEntries(entries: readonly NavEntry[]): NavEntry[] {
 return [...entries.filter(entry => entry.id === "chat"), ...entries.filter(entry => entry.id !== "chat")];
}
