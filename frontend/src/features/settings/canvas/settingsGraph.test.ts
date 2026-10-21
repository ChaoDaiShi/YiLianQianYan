import {describe,expect,it} from "vitest";
import {composeSettingsGraph,mergeSettingsGroupLayout,settingsGroups,settingsFocusNodeIds} from "./settingsGraph";
import presetCatalog from "./settingPresets.json";
import type {SettingPreset,SettingsCanvasState} from "../../../api/settingsCanvas";
const presets=presetCatalog as SettingPreset[];
const state:SettingsCanvasState={connected_node_ids:presets.map(preset=>preset.id),expanded_group_ids:[],nodes:[],viewport:{x:0,y:0,zoom:1}};
describe("expandable settings graph",()=>{
 it("places default categories symmetrically within a compact overview",()=>{
  const graph=composeSettingsGraph(null,state,presets);
  const center=graph.nodes.find(node=>node.kind==="agent")!;
  const point=(id:string)=>graph.nodes.find(node=>node.id===`setting-group:${id}`)!;
  expect(point("models").x-center.x).toBe(300);
  expect(center.x-point("agent").x).toBe(300);
  expect(point("models").y-center.y).toBe(-60);
  expect(point("voice").y-center.y).toBe(60);
  expect(point("security").y-center.y).toBe(-180);
  expect(point("extensions").y-center.y).toBe(180);
  expect(Math.max(...graph.nodes.map(node=>node.y))-Math.min(...graph.nodes.map(node=>node.y))).toBeLessThanOrEqual(360);
 });
 it("centers an expanded child column around its category and focuses only that branch",()=>{
  const graph=composeSettingsGraph(null,{...state,expanded_group_ids:["models","voice"]},presets);
  const category=graph.nodes.find(node=>node.id==="setting-group:models")!;
  const leaves=graph.nodes.filter(node=>["model","search","vision","image","embedding"].includes(node.entity_id));
  expect((Math.min(...leaves.map(node=>node.y))+Math.max(...leaves.map(node=>node.y)))/2).toBe(category.y);
  expect(settingsFocusNodeIds(graph,"models")).toEqual(["setting-group:models","setting:model","setting:search","setting:vision","setting:image","setting:embedding"]);
  const overviewIds=settingsFocusNodeIds(graph);
  expect(overviewIds).toHaveLength(7);
  expect(overviewIds.every(id=>!id.startsWith("setting:"))).toBe(true);
 });
 it("starts with six categories and adds actual leaves only for expanded categories",()=>{
  const collapsed=composeSettingsGraph(null,state,presets);
  expect(collapsed.nodes.filter(node=>node.id.startsWith("setting-group:")).map(node=>node.label)).toEqual(["模型与服务","语音","智能体与上下文","安全","界面","扩展"]);
  expect(collapsed.nodes.filter(node=>node.id.startsWith("setting:"))).toHaveLength(0);
  const expanded=composeSettingsGraph(null,{...state,expanded_group_ids:["models"]},presets);
  expect(expanded.nodes.filter(node=>node.id.startsWith("setting:")).map(node=>node.entity_id)).toEqual(["model","search","vision","image","embedding"]);
  expect(expanded.bindings.filter(edge=>edge.kind==="derived_setting_child")).toHaveLength(5);
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
  expect(leaf.x).toBeGreaterThan(category.x+220);
 });
 it("expands declared plugin forms under extensions with their real IDs",()=>{
  const plugin={id:"plugin:example:preferences",label:"插件偏好",description:"配置",side:"right",group:"plugins"} as SettingPreset;
  expect(settingsGroups([...presets,plugin]).find(group=>group.id==="extensions")!.presets.map(preset=>preset.id)).toContain(plugin.id);
  const graph=composeSettingsGraph(null,{...state,connected_node_ids:[plugin.id],expanded_group_ids:["extensions"]},[...presets,plugin]);
  expect(graph.nodes.find(node=>node.id==="setting:plugin:example:preferences")?.entity_id).toBe(plugin.id);
  expect(graph.bindings.find(edge=>edge.target_id==="setting:plugin:example:preferences")?.source_id).toBe("setting-group:extensions");
 });
});
