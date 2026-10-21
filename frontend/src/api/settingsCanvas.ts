import { connectorRequest, type ConnectorViewport } from "./connectors";
export interface SettingPreset {id:string;label:string;description:string;group:string;side:"left"|"right"|"top"|"bottom"}
export interface SettingsCanvasState {connected_node_ids:string[];expanded_group_ids:string[];nodes:Array<{id:string;x:number;y:number}>;viewport:ConnectorViewport}
export interface SettingsCanvasData {state:SettingsCanvasState;presets:SettingPreset[]}
const path=(canvas:string)=>`/api/settings/canvas${canvas?`?canvas_id=${encodeURIComponent(canvas)}`:""}`;
export const getSettingsCanvas=(canvas:string)=>connectorRequest<SettingsCanvasData>("GET",path(canvas));
export const putSettingsCanvas=(canvas:string,state:SettingsCanvasState)=>connectorRequest<SettingsCanvasData>("PUT",path(canvas),state);
