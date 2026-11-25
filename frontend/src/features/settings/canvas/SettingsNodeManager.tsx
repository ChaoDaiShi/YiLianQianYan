import {PanelRightClose,PanelRightOpen,ChevronRight} from "lucide-react";
import {Button} from "../../../components/ui";
import type {SettingPreset} from "../../../api/settingsCanvas";
import {settingsGroups} from "./settingsGraph";
interface Props {open:boolean;onToggle:()=>void;presets:SettingPreset[];connected:string[];busy:boolean;onSettingConnect:(id:string)=>void;onSettingDisconnect:(id:string)=>void;onSettingEdit:(id:string)=>void}
export default function SettingsNodeManager({open,onToggle,presets,connected,busy,onSettingConnect,onSettingDisconnect,onSettingEdit}:Props){
 return <aside className={`settings-node-manager ${open?"is-open":"is-collapsed"}`} aria-label="节点管理器">
  <button type="button" className="settings-node-manager-toggle focus-ring-token" aria-label={open?"收起节点管理器":"展开节点管理器"} title={open?"收起节点管理器":"展开节点管理器"} aria-expanded={open} onClick={onToggle}>
   {open?<><PanelRightClose size={18} aria-hidden="true"/><span>节点管理器</span></>:<PanelRightOpen size={18} aria-hidden="true"/>}
  </button>
  {open&&<div className="settings-node-manager-scroll">
   {settingsGroups(presets).map(group=><details key={group.id} className="settings-manager-group" data-settings-group={group.id}>
    <summary className="focus-ring-token" aria-label={`展开${group.label}设置`}><ChevronRight size={14} aria-hidden="true"/><strong>{group.label}</strong><span>{group.presets.filter(preset=>connected.includes(preset.id)).length}/{group.presets.length}</span></summary>
    <div className="settings-manager-node-list">{group.presets.map(preset=>{
     const isConnected=connected.includes(preset.id);
     return <article key={preset.id} className="settings-manager-node" data-connected={isConnected}>
      <button type="button" className="settings-manager-node-name focus-ring-token" aria-label={`配置${preset.label}`} onClick={()=>onSettingEdit(preset.id)}>{preset.label}</button>
      <Button size="sm" variant="ghost" className="settings-manager-connection" disabled={busy} aria-label={`${isConnected?"断开":"连接"}${preset.label}`}
       onClick={()=>isConnected?onSettingDisconnect(preset.id):onSettingConnect(preset.id)}>{isConnected?"断开":"连接"}</Button>
     </article>;
    })}</div>
   </details>)}
  </div>}
 </aside>;
}
