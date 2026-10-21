import { ArrowRight, Puzzle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import PageShell from "../components/layout/PageShell";
import { NAV_ENTRIES } from "../components/layout/navRegistry";
import { useProductPreferences } from "../components/system/useProductPreferences";
import { usePluginFeatures } from "../features/plugins/PluginFeaturePage";
import { Button, ErrorState } from "../components/ui";
export default function MorePage(){
 const navigate=useNavigate(),{preferences}=useProductPreferences(),{features,error,reload}=usePluginFeatures();
 const entries=NAV_ENTRIES.filter(e=>!e.primary&&e.id!=="settings"&&preferences.navigation.some(n=>n.id===e.id&&n.visible));
 return <PageShell title="更多" description="功能导航与插件新增功能" maxWidth="none"><div className="navigation-page"><div className="navigation-page-header"><h2>需要的功能，都在这里</h2><p>选择入口，进入对应工作页面。</p></div>{error&&<ErrorState title="插件功能入口无法加载" description={error} action={<Button onClick={()=>void reload()}>重试</Button>}/>}<div className="navigation-card-grid" aria-label="更多功能导航">{entries.map(entry=>{const Icon=entry.icon;return <button key={entry.id} type="button" className="navigation-entry" onClick={()=>navigate(entry.to)}><span className="navigation-entry-icon"><Icon size={22}/></span><h3>{entry.label}</h3><p>{entry.description}</p><ArrowRight className="navigation-entry-arrow" size={17}/></button>;})}{features.map(feature=><button key={`${feature.plugin_id}:${feature.id}`} type="button" className="navigation-entry" onClick={()=>navigate(`/more/plugins/${encodeURIComponent(feature.plugin_id)}/${encodeURIComponent(feature.id)}`)}><span className="navigation-entry-icon"><Puzzle size={22}/></span><h3>{feature.title}</h3><p>{feature.description}</p><p>来自 {feature.plugin_name}</p><ArrowRight className="navigation-entry-arrow" size={17}/></button>)}</div></div></PageShell>;
}
