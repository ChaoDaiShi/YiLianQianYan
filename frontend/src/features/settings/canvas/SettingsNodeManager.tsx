import {PanelRightClose,PanelRightOpen,ChevronRight} from "lucide-react";
import {Button} from "../../../components/ui";
import type {SettingPreset} from "../../../api/settingsCanvas";
import {settingsGroups} from "./settingsGraph";
import SettingsIcon,{settingsIconName} from "../components/SettingsIcon";
interface Props {open:boolean;onToggle:()=>void;presets:SettingPreset[];connected:string[];busy:boolean;onSettingConnect:(id:string)=>void;onSettingDisconnect:(id:string)=>void;onSettingEdit:(id:string)=>void}
export default function SettingsNodeManager({open,onToggle,presets,connected,busy,onSettingConnect,onSettingDisconnect,onSettingEdit}:Props){
 return <aside className={`settings-node-manager ${open?"is-open":"is-collapsed"}`} aria-label="节点管理器">
  <button type="button" className="settings-node-manager-toggle" aria-label={open?"收起节点管理器":"展开节点管理器"} aria-expanded={open} onClick={onToggle}>{open?<PanelRightClose size={19}/>:<PanelRightOpen size={19}/>}<span>{open?"节点管理器":"节点"}</span></button>
  {open&&<div className="settings-node-manager-scroll"><p className="settings-manager-hint">展开分类，管理具体设置。连接状态控制分组在画布中的显示。</p>
   {settingsGroups(presets).map(group=><details key={group.id} className="settings-manager-group" data-settings-group={group.id}>
    <summary aria-label={`展开${group.label}设置`}><ChevronRight size={14}/><SettingsIcon name={settingsIconName(`group:${group.id}`)!} size={28}/><strong>{group.label}</strong><span>{group.presets.filter(preset=>connected.includes(preset.id)).length}/{group.presets.length}</span></summary>
    <div>{group.presets.map(preset=>{const isConnected=connected.includes(preset.id);return <article key={preset.id} className="settings-manager-node" data-connected={isConnected}><SettingsIcon name={settingsIconName(preset.id)!}/><div><strong>{preset.label}</strong><p>{preset.description}</p><div className="settings-manager-node-actions"><Button size="sm" variant="secondary" aria-label={`配置${preset.label}`} onClick={()=>onSettingEdit(preset.id)}>配置</Button>{isConnected?<Button size="sm" variant="ghost" disabled={busy} aria-label={`断开${preset.label}`} onClick={()=>onSettingDisconnect(preset.id)}>断开</Button>:<Button size="sm" variant="ghost" disabled={busy} aria-label={`连接${preset.label}`} onClick={()=>onSettingConnect(preset.id)}>连接节点</Button>}</div></div></article>;})}</div>
   </details>)}
  </div>}
 </aside>;
}
