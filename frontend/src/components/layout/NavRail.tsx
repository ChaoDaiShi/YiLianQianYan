import { useCallback,useState } from "react";
import { NavLink,useLocation,useNavigate } from "react-router-dom";
import { LayoutGrid,Settings as SettingsIcon,ShieldCheck } from "lucide-react";
import { cn,Tooltip } from "../ui";
import { useTheme } from "../../theme";
import { healthCheck } from "../../api/client";
import { NAV_ENTRIES, orderNavigationEntries } from "./navRegistry";
import { useProductPreferences } from "../system/useProductPreferences";
import { TRUSTED_MODULES } from "../system/productPreferences";
import { useVisibleRefresh } from "../system/useVisibleRefresh";
export default function NavRail(){
 const navigate=useNavigate(),location=useLocation(),{theme}=useTheme(),{preferences}=useProductPreferences();
 const [backendStatus,setBackendStatus]=useState<"connecting"|"healthy"|"unavailable">("connecting");
 const refreshBackendStatus=useCallback(async()=>{const health=await healthCheck();setBackendStatus(health?.status==="healthy"&&health.database==="healthy"?"healthy":"unavailable");},[]);
 useVisibleRefresh(refreshBackendStatus);
 const entries=orderNavigationEntries(preferences.navigation.filter(item=>item.visible&&!TRUSTED_MODULES.some(module=>!module.mandatory&&module.navigation===item.id&&!preferences.enabled_modules.includes(module.id))).flatMap(item=>{const e=NAV_ENTRIES.find(e=>e.id===item.id);return e?[e]:[];}));
 const secondaryActive=location.pathname.startsWith("/more")||entries.some(e=>!e.primary&&!["settings"].includes(e.id)&&location.pathname.startsWith(e.to));
 const statusLabel={connecting:"连接中",healthy:"后端正常",unavailable:"后端不可用"}[backendStatus];
 return <nav aria-label="全局导航" className="nav-rail z-20 flex h-full w-[68px] shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--sidebar-bg)] py-3 text-[var(--sidebar-text)] min-[960px]:w-[80px]">
 <Tooltip content="忆涟千言 · 新对话"><button type="button" onClick={()=>navigate("/chat")} className="mb-2 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl p-1 hover:bg-white/10" aria-label="忆涟千言 · 新对话"><img src="/favicon.png" alt="忆涟千言" className="h-8 w-8 object-contain"/></button></Tooltip>
 <div className="mb-2 w-10 shrink-0 border-t border-white/[0.07]"/>
 <div className="nav-rail-list nav-rail-compact scrollbar-thin flex flex-col items-center gap-1 px-1">
 {entries.filter(e=>e.primary).map(item=>{const Icon=item.icon;return <Tooltip key={item.id} content={item.label}><NavLink to={item.to} aria-label={item.label} className={({isActive})=>cn("nav-rail-item relative flex min-h-14 w-16 flex-col items-center justify-center gap-1 rounded-xl px-1 leading-tight min-[960px]:w-[68px]",isActive?"bg-[var(--sidebar-active)] text-white":"opacity-80 hover:bg-white/10")}>
 {({isActive})=><><Icon className="h-5 w-5"/><span className={cn("nav-rail-item-label whitespace-nowrap text-xs",theme.monoTitles&&"font-mono")}>{item.label}</span>{isActive&&<><span className="absolute left-0 top-1/2 h-7 w-0.5 -translate-y-1/2 rounded-r-full bg-[var(--accent-primary)]"/><span className="absolute right-1 top-1 text-[10px] text-[var(--accent-gold)]">✦</span></>}</>}
 </NavLink></Tooltip>})}
 <Tooltip content="更多功能"><NavLink to="/more" aria-label="更多" className={cn("nav-rail-item relative flex min-h-14 w-16 flex-col items-center justify-center gap-1 rounded-xl px-1 leading-tight min-[960px]:w-[68px]",secondaryActive?"bg-[var(--sidebar-active)] text-white":"opacity-80 hover:bg-white/10")}><LayoutGrid className="h-5 w-5"/><span className="nav-rail-item-label whitespace-nowrap text-xs">更多</span></NavLink></Tooltip>
 </div><div className="flex-1"/><div className="flex shrink-0 flex-col items-center gap-2 pb-1">
 <Tooltip content="安全与审批"><NavLink to="/system?card=approvals" aria-label="安全与审批中心" className="nav-rail-item flex h-11 w-11 items-center justify-center rounded-xl opacity-80 hover:bg-white/10"><ShieldCheck className="h-5 w-5"/></NavLink></Tooltip>
 <Tooltip content="设置"><NavLink to="/settings" aria-label="系统与模型设置" className="nav-rail-item flex h-11 w-11 items-center justify-center rounded-xl opacity-80 hover:bg-white/10"><SettingsIcon className="h-5 w-5"/></NavLink></Tooltip>
 <Tooltip content={`后端状态：${statusLabel}`}><div aria-label={`后端状态：${statusLabel}`} className="nav-rail-status-motif flex h-8 w-8 items-center justify-center"><span className={cn("block h-2.5 w-2.5 rounded-full",backendStatus==="healthy"?"bg-[var(--success)]":backendStatus==="connecting"?"bg-[var(--warning)]":"bg-[var(--danger)]")}/></div></Tooltip>
 </div></nav>;
}
