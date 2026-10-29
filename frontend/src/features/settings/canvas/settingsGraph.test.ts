import {describe,expect,it} from "vitest";
import {composeSettingsGraph,mergeSettingsGroupLayout,settingsGroups,settingsFocusNodeIds,SETTINGS_NODE_SIZE} from "./settingsGraph";
import presetCatalog from "./settingPresets.json";
import type {SettingPreset,SettingsCanvasState} from "../../../api/settingsCanvas";
const presets=presetCatalog as SettingPreset[];
const state:SettingsCanvasState={connected_node_ids:presets.map(preset=>preset.id),expanded_group_ids:[],nodes:[],viewport:{x:0,y:0,zoom:1}};
describe("expandable settings graph",()=>{
 it("keeps all simultaneously expanded setting cards apart",()=>{
  const graph=composeSettingsGraph(null,{...state,expanded_group_ids:settingsGroups(presets).map(group=>group.id)},presets);
  const cards=graph.nodes.filter(node=>node.kind.startsWith("setting_"));
  for(let a=0;a<cards.length;a++)for(let b=a+1;b<cards.length;b++){
   const left=cards[a],right=cards[b];
   const separated=Math.abs(left.x-right.x)>=SETTINGS_NODE_SIZE.width+SETTINGS_NODE_SIZE.gap||Math.abs(left.y-right.y)>=SETTINGS_NODE_SIZE.height+SETTINGS_NODE_SIZE.gap;
   expect(separated,`${left.id} overlaps ${right.id}`).toBe(true);
  }
 });
 it("relocates conflicting saved voice leaves while preserving dragged nonconflicting coordinates",()=>{
  const prior:SettingsCanvasState={...state,expanded_group_ids:["models","voice"],nodes:[
   {id:"group:models",x:660,y:126},{id:"group:voice",x:660,y:246},
   {id:"model",x:952,y:-70},{id:"search",x:952,y:28},{id:"vision",x:1800,y:900},
   {id:"image",x:952,y:224},{id:"embedding",x:952,y:322},
   {id:"stt",x:952,y:197},{id:"tts",x:952,y:295},
  ]};
  const graph=composeSettingsGraph(null,prior,presets);
  const point=(id:string)=>graph.nodes.find(node=>node.entity_id===id)!;
  expect({x:point("vision").x,y:point("vision").y}).toEqual({x:1800,y:900});
  expect({x:point("image").x,y:point("image").y}).toEqual({x:952,y:224});
  expect(point("stt").x).toBeGreaterThan(point("image").x+SETTINGS_NODE_SIZE.width);
  expect(point("tts").x).toBeGreaterThan(point("embedding").x+SETTINGS_NODE_SIZE.width);
  const saved=mergeSettingsGroupLayout(prior,graph,[],prior.viewport);
  const reloaded=composeSettingsGraph(null,saved,presets);
  expect(reloaded.nodes.map(({id,x,y})=>({id,x,y}))).toEqual(graph.nodes.map(({id,x,y})=>({id,x,y})));
  const dragged=mergeSettingsGroupLayout(saved,graph,[{id:"setting:vision",x:2020,y:1040}],prior.viewport);
  const afterDrag=composeSettingsGraph(null,dragged,presets).nodes.find(node=>node.id==="setting:vision")!;
  expect({x:afterDrag.x,y:afterDrag.y}).toEqual({x:2020,y:1040});
 });
 it("places default categories symmetrically within a compact overview",()=>{
  const graph=composeSettingsGraph(null,state,presets);
  const center=graph.nodes.find(node=>node.kind==="agent")!;
  const point=(id:string)=>graph.nodes.find(node=>node.id===`setting-group:${id}`)!;
  const centerSize=104;
  const leftGap=center.x-(point("agent").x+SETTINGS_NODE_SIZE.width);
  const rightGap=point("models").x-(center.x+centerSize);
  expect(leftGap).toBe(rightGap);
  expect(leftGap).toBeGreaterThan(0);
  expect(point("models").y+SETTINGS_NODE_SIZE.height).toBeLessThan(point("voice").y);
  expect(point("security").y+SETTINGS_NODE_SIZE.height).toBeLessThan(center.y);
  expect(point("extensions").y).toBeGreaterThan(center.y+centerSize);
  expect(Math.max(...graph.nodes.map(node=>node.x+SETTINGS_NODE_SIZE.width))-Math.min(...graph.nodes.map(node=>node.x))).toBeLessThan(600);
  expect(Math.max(...graph.nodes.map(node=>node.y+SETTINGS_NODE_SIZE.height))-Math.min(...graph.nodes.map(node=>node.y))).toBeLessThan(340);
 });
 it("centers an expanded child column around its category and focuses only that branch",()=>{
  const graph=composeSettingsGraph(null,{...state,expanded_group_ids:["models","voice"]},presets);
  const category=graph.nodes.find(node=>node.id==="setting-group:models")!;
  const leaves=graph.nodes.filter(node=>["model","model-pools","search","vision","image","embedding"].includes(node.entity_id));
  expect((Math.min(...leaves.map(node=>node.y))+Math.max(...leaves.map(node=>node.y)))/2).toBe(category.y);
  expect(settingsFocusNodeIds(graph,"models")).toEqual(["setting-group:models","setting:model","setting:model-pools","setting:search","setting:vision","setting:image","setting:embedding"]);
  const overviewIds=settingsFocusNodeIds(graph);
  expect(overviewIds).toHaveLength(7);
  expect(overviewIds.every(id=>!id.startsWith("setting:"))).toBe(true);
 });
 it("starts with six categories and adds actual leaves only for expanded categories",()=>{
  const collapsed=composeSettingsGraph(null,state,presets);
  expect(collapsed.nodes.filter(node=>node.id.startsWith("setting-group:")).map(node=>node.label)).toEqual(["模型与服务","语音","智能体与上下文","安全","界面","扩展"]);
  expect(collapsed.nodes.filter(node=>node.id.startsWith("setting:"))).toHaveLength(0);
  const expanded=composeSettingsGraph(null,{...state,expanded_group_ids:["models"]},presets);
  expect(expanded.nodes.filter(node=>node.id.startsWith("setting:")).map(node=>node.entity_id)).toEqual(["model","model-pools","search","vision","image","embedding"]);
  expect(expanded.bindings.filter(edge=>edge.kind==="derived_setting_child")).toHaveLength(6);
  expect(expanded.bindings.find(edge=>edge.target_id==="setting:search")?.source_id).toBe("setting-group:models");
  expect(collapsed.bindings).toHaveLength(6);
 });
 it("preserves per-setting connection choices when expanding and collapsing",()=>{
  const graph=composeSettingsGraph(null,{...state,connected_node_ids:["search","navigation"],expanded_group_ids:["models"]},presets);
  expect(graph.nodes.filter(node=>node.id.startsWith("setting-group:")).map(node=>node.id)).toEqual(["setting-group:models","setting-group:interface"]);
  expect(graph.nodes.filter(node=>node.id.startsWith("setting:")).map(node=>node.entity_id)).toEqual(["search"]);
 });
 it("retains category and hidden leaf/plugin coordinates when a different category moves",()=>{
  const prior={...state,nodes:[{id:"search",x:900,y:100},{id:"model",x:600,y:300},{id:"plugin:disabled:preferences",x:1200,y:500}]};
  const graph=composeSettingsGraph(null,prior,presets);
  const saved=mergeSettingsGroupLayout(prior,graph,[{id:"setting-group:models",x:800,y:420}],{x:40,y:20,zoom:.7});
  expect(saved.nodes.find(node=>node.id==="group:models")).toEqual({id:"group:models",x:800,y:420});
  expect(saved.nodes.find(node=>node.id==="model")).toEqual(prior.nodes[1]);
  expect(saved.nodes.find(node=>node.id==="search")).toEqual(prior.nodes[0]);
  expect(saved.nodes.find(node=>node.id==="plugin:disabled:preferences")).toEqual(prior.nodes[2]);
  expect(saved.connected_node_ids).toEqual(prior.connected_node_ids);
 });
 it("separates the old representative position into category and actual leaf positions",()=>{
  const graph=composeSettingsGraph(null,{...state,expanded_group_ids:["models"],nodes:[{id:"model",x:600,y:300}]},presets);
  const category=graph.nodes.find(node=>node.id==="setting-group:models")!;
  const leaf=graph.nodes.find(node=>node.id==="setting:model")!;
  expect({x:category.x,y:category.y}).toEqual({x:600,y:300});
  expect(leaf.x).toBeGreaterThan(category.x+SETTINGS_NODE_SIZE.width);
 });
 it("expands declared plugin forms under extensions with their real IDs",()=>{
  const plugin={id:"plugin:example:preferences",label:"插件偏好",description:"配置",side:"right",group:"plugins"} as SettingPreset;
  expect(settingsGroups([...presets,plugin]).find(group=>group.id==="extensions")!.presets.map(preset=>preset.id)).toContain(plugin.id);
  const graph=composeSettingsGraph(null,{...state,connected_node_ids:[plugin.id],expanded_group_ids:["extensions"]},[...presets,plugin]);
  expect(graph.nodes.find(node=>node.id==="setting:plugin:example:preferences")?.entity_id).toBe(plugin.id);
  expect(graph.bindings.find(edge=>edge.target_id==="setting:plugin:example:preferences")?.source_id).toBe("setting-group:extensions");
 });
});
