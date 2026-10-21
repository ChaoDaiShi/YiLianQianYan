import type {ConnectorNode,ConnectorView,ConnectorViewport} from "../../../api/connectors";
import type {SettingPreset,SettingsCanvasState} from "../../../api/settingsCanvas";
export const SETTING_PREFIX="setting:";
export const SETTING_GROUP_PREFIX="setting-group:";
export interface SettingsGroup {id:string;label:string;description:string;representative:string;side:SettingPreset["side"];presetIds:readonly string[]}
export const SETTINGS_GROUPS:readonly SettingsGroup[]=[
 {id:"models",label:"模型与服务",description:"对话、搜索、图像与向量服务",representative:"model",side:"right",presetIds:["model","search","vision","image","embedding"]},
 {id:"voice",label:"语音",description:"语音识别与语音合成",representative:"stt",side:"right",presetIds:["stt","tts"]},
 {id:"agent",label:"智能体与上下文",description:"智能体行为与长对话管理",representative:"agent",side:"left",presetIds:["agent","compaction"]},
 {id:"security",label:"安全",description:"权限审批与执行沙箱",representative:"permissions",side:"top",presetIds:["permissions","sandbox"]},
 {id:"interface",label:"界面",description:"主题外观与导航布局",representative:"appearance",side:"left",presetIds:["appearance","navigation"]},
 {id:"extensions",label:"扩展",description:"本地目录与插件提供的设置",representative:"extensions",side:"bottom",presetIds:["extensions"]},
];
export function settingsGroups(presets:SettingPreset[]){
 return SETTINGS_GROUPS.map(group=>({...group,presets:presets.filter(preset=>group.presetIds.includes(preset.id)||(group.id==="extensions"&&preset.group==="plugins"))}));
}
export function settingsFocusNodeIds(graph:ConnectorView,groupId?:string):string[]{
 if(groupId){const id=SETTING_GROUP_PREFIX+groupId;return [id,...graph.bindings.filter(binding=>binding.kind==="derived_setting_child"&&binding.source_id===id).map(binding=>binding.target_id)];}
 return graph.nodes.filter(node=>node.kind==="agent"||node.entity_id.startsWith("group:")).map(node=>node.id);
}
export function mergeSettingsGroupLayout(state:SettingsCanvasState,graph:ConnectorView,positions:Array<{id:string;x:number;y:number}>,viewport:ConnectorViewport):SettingsCanvasState{
 const moved=new Map(positions.map(point=>[point.id,point]));
 const settings=graph.nodes.filter(node=>node.kind.startsWith("setting_")).map(node=>({id:node.entity_id,x:moved.get(node.id)?.x??node.x,y:moved.get(node.id)?.y??node.y}));
 const updated=new Set(settings.map(point=>point.id));
 return {...state,viewport,nodes:[...state.nodes.filter(point=>!updated.has(point.id)),...settings]};
}
export function composeSettingsGraph(view:ConnectorView|null,state:SettingsCanvasState,presets:SettingPreset[]):ConnectorView {
 const agentId=view?.agent_id??"settings-global";
 const center=view?.nodes.find(node=>node.kind==="agent")??{id:`agent:${agentId}`,kind:"agent",entity_id:agentId,label:"忆涟设置",x:360,y:186};
 const nodes:ConnectorNode[]=view?[...view.nodes]:[center];
 const bindings=view?[...view.bindings]:[];
 if(view){nodes.push({id:`constraint:${agentId}`,entity_id:agentId,kind:"constraint",label:"权限与范围",x:center.x,y:center.y-128});bindings.push({id:`derived:constraint:${agentId}`,kind:"derived_constraint",source_id:`constraint:${agentId}`,target_id:center.id});}
 const groups=settingsGroups(presets).filter(group=>group.presets.some(preset=>state.connected_node_ids.includes(preset.id)));
 const occupied=nodes.map(node=>({x:node.x,y:node.y,width:220,height:node.kind==="agent"?142:74}));
 const categoryPositions=new Map<string,{x:number;y:number}>();
 const offsets:Record<string,{x:number;y:number}>={models:{x:300,y:-60},voice:{x:300,y:60},agent:{x:-300,y:-60},interface:{x:-300,y:60},security:{x:0,y:-180},extensions:{x:0,y:180}};
 for(const group of groups){
  const saved=state.nodes.find(point=>point.id===`group:${group.id}`)??state.nodes.find(point=>point.id===group.representative);
  if(saved){categoryPositions.set(group.id,{x:saved.x,y:saved.y});occupied.push({x:saved.x,y:saved.y,width:220,height:74});}
 }
 for(const group of groups){
  let position=categoryPositions.get(group.id);let slot=0;
  while(!position){const offset=offsets[group.id];const next={x:center.x+offset.x+slot*(group.side==="left"?-292:292),y:center.y+offset.y};
   if(!occupied.some(box=>next.x<box.x+box.width+16&&next.x+236>box.x&&next.y<box.y+box.height+16&&next.y+90>box.y))position=next;slot++;
  }
  categoryPositions.set(group.id,position);occupied.push({...position,width:220,height:74});
  const id=SETTING_GROUP_PREFIX+group.id;
  nodes.push({id,entity_id:`group:${group.id}`,kind:`setting_${group.side}`,label:group.label,settings_expanded:state.expanded_group_ids.includes(group.id),...position});
  const incoming=group.side==="top"||group.side==="left";
  bindings.push({id:`derived:${id}`,kind:group.side==="top"?"derived_constraint":group.side==="left"?"conversation_agent":group.side==="bottom"?"agent_memory":"agent_capability",source_id:incoming?id:center.id,target_id:incoming?center.id:id});
 }
 const expanded=groups.filter(group=>state.expanded_group_ids.includes(group.id));
 const leafPositions=new Map<string,{x:number;y:number}>();
 for(const group of expanded){
  const category=categoryPositions.get(group.id)!;
  for(const preset of group.presets.filter(preset=>state.connected_node_ids.includes(preset.id))){
   const saved=state.nodes.find(point=>point.id===preset.id);
   // The former representative position belongs to the category after splitting the hierarchy.
   if(saved&&!(preset.id===group.representative&&saved.x===category.x&&saved.y===category.y)){
    leafPositions.set(preset.id,{x:saved.x,y:saved.y});occupied.push({x:saved.x,y:saved.y,width:220,height:74});
   }
  }
 }
 for(const group of expanded){
  const category=categoryPositions.get(group.id)!;
  const visible=group.presets.filter(preset=>state.connected_node_ids.includes(preset.id));
  const direction=group.side==="left"?-1:1;
  let column=category.x+direction*292;
  const rowY=(index:number)=>category.y+(index-(visible.length-1)/2)*98;
  while(visible.some((preset,index)=>!leafPositions.has(preset.id)&&occupied.some(box=>column<box.x+box.width+16&&column+236>box.x&&rowY(index)<box.y+box.height+16&&rowY(index)+90>box.y)))column+=direction*292;
  for(const [index,preset] of visible.entries()){
   const position=leafPositions.get(preset.id)??{x:column,y:rowY(index)};
   occupied.push({...position,width:220,height:74});
   const id=SETTING_PREFIX+preset.id;
   nodes.push({id,entity_id:preset.id,kind:group.side==="left"?"setting_left":"setting_right",label:preset.label,...position});
   bindings.push({id:`derived:setting-child:${preset.id}`,kind:"derived_setting_child",source_id:SETTING_GROUP_PREFIX+group.id,target_id:id});
  }
 }
 return {canvas_id:view?.canvas_id??"global",agent_id:agentId,name:view?.name??"全局设置",nodes,bindings,viewport:state.viewport,display:{mode:"all",visible_kinds:[],hidden_node_ids:view?.display.hidden_node_ids??[],show_labels:true,show_edges:true,density:"compact"}};
}
