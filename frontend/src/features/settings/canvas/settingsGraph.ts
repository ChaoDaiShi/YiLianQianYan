import type {ConnectorNode,ConnectorView,ConnectorViewport} from "../../../api/connectors";
import type {SettingPreset,SettingsCanvasState} from "../../../api/settingsCanvas";
export const SETTING_PREFIX="setting:";
export const SETTING_GROUP_PREFIX="setting-group:";
export const SETTINGS_NODE_SIZE={width:168,height:54,gap:14} as const;
export const SETTINGS_CENTER_SIZE=100;
export interface SettingsGroup {id:string;label:string;representative:string;side:SettingPreset["side"];presetIds:readonly string[]}
export const SETTINGS_GROUPS:readonly SettingsGroup[]=[
 {id:"models",label:"模型与服务",representative:"model",side:"right",presetIds:["model","search","vision","image","embedding"]},
 {id:"voice",label:"语音",representative:"stt",side:"right",presetIds:["stt","tts"]},
 {id:"agent",label:"智能体与上下文",representative:"agent",side:"left",presetIds:["agent","compaction"]},
 {id:"security",label:"安全",representative:"permissions",side:"top",presetIds:["permissions","sandbox"]},
 {id:"interface",label:"界面",representative:"appearance",side:"left",presetIds:["appearance","navigation"]},
 {id:"extensions",label:"扩展",representative:"extensions",side:"bottom",presetIds:["extensions"]},
];
export function settingsGroups(presets:SettingPreset[]){
 return SETTINGS_GROUPS.map(group=>({...group,presets:presets.filter(preset=>group.presetIds.includes(preset.id)||(group.id==="extensions"&&preset.group==="plugins"))}));
}
export function settingsFocusNodeIds(graph:ConnectorView):string[]{
 return graph.nodes.filter(node=>node.kind==="agent"||node.kind.startsWith("setting_")).map(node=>node.id);
}
export function settingsBranchNodeIds(graph:ConnectorView,groupId:string):string[]{
 const id=SETTING_GROUP_PREFIX+groupId;
 return [id,...graph.bindings.filter(binding=>binding.kind==="derived_setting_child"&&binding.source_id===id).map(binding=>binding.target_id)];
}
export function settingsFocusNeighborhood(graph:ConnectorView,groupId:string):string[]{
 const ids=settingsBranchNodeIds(graph,groupId);
 const [category]=ids;
 for(const {source_id,target_id} of graph.bindings){
  if(source_id===category&&!ids.includes(target_id))ids.push(target_id);
  if(target_id===category&&!ids.includes(source_id))ids.push(source_id);
 }
 return ids;
}
export function mergeSettingsGroupLayout(state:SettingsCanvasState,graph:ConnectorView,positions:Array<{id:string;x:number;y:number}>,viewport:ConnectorViewport):SettingsCanvasState{
 const moved=new Map(positions.map(point=>[point.id,point]));
 const representatives=new Set(SETTINGS_GROUPS.filter(group=>moved.has(SETTING_PREFIX+group.representative)).map(group=>SETTING_GROUP_PREFIX+group.id));
 const settings=graph.nodes.flatMap(node=>{
  const point=moved.get(node.id);
  if(point&&node.kind.startsWith("setting_"))return [{id:node.entity_id,x:point.x,y:point.y}];
  // Record the category separately when a legacy representative becomes a manually placed leaf.
  if(representatives.has(node.id)&&!state.nodes.some(saved=>saved.id===node.entity_id))return [{id:node.entity_id,x:node.x,y:node.y}];
  return [];
 });
 const updated=new Set(settings.map(point=>point.id));
 return {...state,viewport,nodes:[...state.nodes.filter(point=>!updated.has(point.id)),...settings]};
}

export function releaseAutomaticBranchPositions(state:SettingsCanvasState,presets:SettingPreset[],view:ConnectorView|null):SettingsCanvasState{
 const positions=new Map(state.nodes.map(point=>[point.id,point]));
 const groups=settingsGroups(presets).map(group=>({
  ...group,
  presets:group.presets.filter(preset=>state.connected_node_ids.includes(preset.id)),
 })).filter(group=>group.presets.length);
 // Earlier camera saves recorded every visible category and its generated leaves.
 if(!groups.length||!groups.every(group=>positions.has(`group:${group.id}`)))return state;
 const generated=new Set<string>();
 for(const group of groups){
  if(group.side!=="left"&&group.side!=="right")continue;
  const origin=positions.get(`group:${group.id}`)!;
  const direction=group.side==="left"?-1:1;
  for(const [index,preset] of group.presets.entries()){
   const point=positions.get(preset.id);
   if(!point)continue;
   const offset=(point.x-origin.x)*direction;
   const along=index-(group.presets.length-1)/2;
   if((offset>=204&&offset%204===0&&point.y===origin.y+along*68)
    ||(offset>=216&&offset%216===0&&point.y===origin.y+along*76))generated.add(preset.id);
  }
 }
 if(!generated.size)return state;
 const overview=composeSettingsGraph(view,{...state,nodes:[],expanded_group_ids:[]},presets);
 for(const node of overview.nodes){
  const point=positions.get(node.entity_id);
  if(node.entity_id.startsWith("group:")&&point?.x===node.x&&point.y===node.y)generated.add(node.entity_id);
 }
 return {...state,nodes:state.nodes.filter(point=>!generated.has(point.id))};
}
export function composeSettingsGraph(view:ConnectorView|null,state:SettingsCanvasState,presets:SettingPreset[]):ConnectorView {
 const agentId=view?.agent_id??"settings-global";
 const center=view?.nodes.find(node=>node.kind==="agent")??{id:`agent:${agentId}`,kind:"agent",entity_id:agentId,label:"忆涟设置",x:300,y:160};
 const nodes:ConnectorNode[]=view?[...view.nodes]:[center];
 const bindings=view?[...view.bindings]:[];
 if(view){nodes.push({id:`constraint:${agentId}`,entity_id:agentId,kind:"constraint",label:"权限与范围",x:center.x,y:center.y-128});bindings.push({id:`derived:constraint:${agentId}`,kind:"derived_constraint",source_id:`constraint:${agentId}`,target_id:center.id});}
 const groups=settingsGroups(presets).filter(group=>group.presets.some(preset=>state.connected_node_ids.includes(preset.id)));
 const {width,height,gap}=SETTINGS_NODE_SIZE;
 const occupied=nodes.map(node=>({x:node.x,y:node.y,width:node.kind==="agent"?SETTINGS_CENTER_SIZE:width,height:node.kind==="agent"?SETTINGS_CENTER_SIZE:height}));
 const overlaps=(point:{x:number;y:number})=>occupied.some(box=>point.x<box.x+box.width+SETTINGS_NODE_SIZE.gap&&point.x+SETTINGS_NODE_SIZE.width+SETTINGS_NODE_SIZE.gap>box.x&&point.y<box.y+box.height+SETTINGS_NODE_SIZE.gap&&point.y+SETTINGS_NODE_SIZE.height+SETTINGS_NODE_SIZE.gap>box.y);
 const categoryPositions=new Map<string,{x:number;y:number}>();
 const laneGap=36;
 const horizontalStep=width+laneGap;
 const right=SETTINGS_CENTER_SIZE+laneGap;
 const left=-horizontalStep;
 const sideStep=height+20;
 const upper=(SETTINGS_CENTER_SIZE-height-sideStep)/2;
 const lower=upper+sideStep;
 const centerX=(SETTINGS_CENTER_SIZE-width)/2;
 const offsets:Record<string,{x:number;y:number}>={
  models:{x:right,y:upper},voice:{x:right,y:lower},agent:{x:left,y:upper},interface:{x:left,y:lower},
  security:{x:centerX,y:-height-laneGap},extensions:{x:centerX,y:SETTINGS_CENTER_SIZE+laneGap},
 };
 for(const group of groups){
  const saved=state.nodes.find(point=>point.id===`group:${group.id}`)??state.nodes.find(point=>point.id===group.representative);
  if(saved){categoryPositions.set(group.id,{x:saved.x,y:saved.y});occupied.push({x:saved.x,y:saved.y,width:SETTINGS_NODE_SIZE.width,height:SETTINGS_NODE_SIZE.height});}
 }
 for(const group of groups){
  let position=categoryPositions.get(group.id);let slot=0;
  while(!position){const offset=offsets[group.id];const next={x:center.x+offset.x+slot*(group.side==="left"?-horizontalStep:horizontalStep),y:center.y+offset.y};
   if(!overlaps(next))position=next;slot++;
  }
  categoryPositions.set(group.id,position);occupied.push({...position,width:SETTINGS_NODE_SIZE.width,height:SETTINGS_NODE_SIZE.height});
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
   if(saved&&!(preset.id===group.representative&&saved.x===category.x&&saved.y===category.y)&&!overlaps(saved)){
    leafPositions.set(preset.id,{x:saved.x,y:saved.y});occupied.push({x:saved.x,y:saved.y,width:SETTINGS_NODE_SIZE.width,height:SETTINGS_NODE_SIZE.height});
   }
  }
 }
 for(const group of expanded){
  const category=categoryPositions.get(group.id)!;
  const visible=group.presets.filter(preset=>state.connected_node_ids.includes(preset.id));
  const vertical=group.side==="top"||group.side==="bottom";
  const direction=group.side==="left"||group.side==="top"?-1:1;
  const rowStep=height+gap;
  const firstRow=category.y-(visible.length-1)*rowStep/2;
  const neighbor=!vertical&&expanded.find(item=>item.id!==group.id&&item.side===group.side);
  let branchStart=firstRow;
  if(neighbor){
   const other=categoryPositions.get(neighbor.id)!;
   const split=(category.y+other.y+height)/2;
   const above=category.y===other.y?expanded.indexOf(group)<expanded.indexOf(neighbor):category.y<other.y;
   branchStart=above
    ? Math.min(firstRow,split-gap-height-(visible.length-1)*rowStep)
    : Math.max(firstRow,split+gap);
  }
  const childPosition=(index:number,lane:number)=>{
   const along=index-(visible.length-1)/2;
   return vertical
    ? {x:category.x+along*(width+gap),y:category.y+direction*lane*(height+laneGap)}
    : {x:category.x+direction*lane*horizontalStep,y:branchStart+index*rowStep};
  };
  let lane=1;
  while(visible.some((preset,index)=>!leafPositions.has(preset.id)&&overlaps(childPosition(index,lane))))lane++;
  for(const [index,preset] of visible.entries()){
   const position=leafPositions.get(preset.id)??childPosition(index,lane);
   occupied.push({...position,width:SETTINGS_NODE_SIZE.width,height:SETTINGS_NODE_SIZE.height});
   const id=SETTING_PREFIX+preset.id;
   nodes.push({id,entity_id:preset.id,kind:`setting_${group.side}`,label:preset.label,...position});
   bindings.push({id:`derived:setting-child:${preset.id}`,kind:"derived_setting_child",source_id:SETTING_GROUP_PREFIX+group.id,target_id:id});
  }
 }
 return {canvas_id:view?.canvas_id??"global",agent_id:agentId,name:view?.name??"全局设置",nodes,bindings,viewport:state.viewport,display:{mode:"all",visible_kinds:[],hidden_node_ids:view?.display.hidden_node_ids??[],show_labels:true,show_edges:true,density:"compact"}};
}
