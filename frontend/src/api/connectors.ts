import { requestResult } from "../core/api/http";
export type ConnectorKind = "conversation"|"agent"|"constraint"|"skill"|"workflow"|"mcp"|"tool"|"model"|"plugin"|"memory"|"knowledge";
export interface ConnectorResource {id:string;kind:ConnectorKind;entity_id:string;label:string;model_source?:"profile"|"pool";service_kind?:"chat"|"vision"|"image"}
export interface ConnectorNode extends Omit<ConnectorResource,"kind"> {kind:string;x:number;y:number;settings_expanded?:boolean}
export interface ConnectorBinding {id:string;kind:string;source_id:string;target_id:string;model_source?:"profile"|"pool";service_kind?:"chat"|"vision"|"image"}
export interface ConnectorDisplay {mode:"all"|"conversations"|"capabilities"|"custom";visible_kinds:string[];hidden_node_ids:string[];show_edges:boolean;show_labels:boolean;density:"compact"|"comfortable"}
export interface ConnectorCanvas {id:string;name:string;agent_id:string;updated_at:number}
export interface ConnectorViewport {x:number;y:number;zoom:number}
export interface ConnectorView {canvas_id:string;name:string;agent_id:string;nodes:ConnectorNode[];bindings:ConnectorBinding[];display:ConnectorDisplay;viewport:ConnectorViewport}
export async function connectorRequest<T>(method:string,path:string,body?:unknown):Promise<T>{const result=await requestResult<T>(method,path,body);if(!result.ok)throw new Error(result.error);return result.data;}
export const listConnectorCanvases=()=>connectorRequest<{canvases:ConnectorCanvas[]}>("GET","/api/connectors");
export const getConnectorView=(id:string)=>connectorRequest<ConnectorView>("GET",`/api/connectors/${encodeURIComponent(id)}`);
export const listConnectorResources=()=>connectorRequest<{resources:ConnectorResource[]}>("GET","/api/connectors/resources");
export const createConnectorCanvas=(name:string,agent_id:string)=>connectorRequest<ConnectorView>("POST","/api/connectors",{name,agent_id});
export const deleteConnectorCanvas=(id:string)=>connectorRequest<void>("DELETE",`/api/connectors/${encodeURIComponent(id)}`);
export const renameConnectorCanvas=(id:string,name:string)=>connectorRequest<ConnectorView>("PATCH",`/api/connectors/${encodeURIComponent(id)}`,{name});
export const saveConnectorLayout=(id:string,nodes:Array<{id:string;x:number;y:number}>,viewport:ConnectorViewport)=>connectorRequest("PUT",`/api/connectors/${encodeURIComponent(id)}/layout`,{nodes,viewport});
export const saveConnectorDisplay=(id:string,display:ConnectorDisplay)=>connectorRequest<ConnectorDisplay>("PUT",`/api/connectors/${encodeURIComponent(id)}/display`,display);
export const bindConnectorResource=(id:string,binding:Omit<ConnectorBinding,"id">)=>connectorRequest<ConnectorBinding>("POST",`/api/connectors/${encodeURIComponent(id)}/bindings`,binding);
export const unbindConnectorResource=(id:string,binding:string)=>connectorRequest<void>("DELETE",`/api/connectors/${encodeURIComponent(id)}/bindings/${encodeURIComponent(binding)}`);
