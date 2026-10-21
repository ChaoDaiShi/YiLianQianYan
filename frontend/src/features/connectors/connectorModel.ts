import type { ConnectorNode,ConnectorDisplay } from "../../api/connectors";
export function visibleConnectorNodes(nodes:ConnectorNode[],display:ConnectorDisplay){
 const allowed=display.mode==="conversations"?["agent","conversation"]:display.mode==="capabilities"?["agent","skill","workflow","mcp","tool","model","plugin"]:display.mode==="custom"?["agent",...display.visible_kinds]:null;
 return nodes.filter(node=>node.kind==="agent"||((!allowed||allowed.includes(node.kind))&&!display.hidden_node_ids.includes(node.id)));
}
export function connectorPosition(kind:string,index:number,density:ConnectorDisplay["density"]){const extra=density==="comfortable"?32:0;
 if(kind==="agent")return{x:360,y:186};if(kind==="constraint")return{x:360,y:58};
 if(kind==="conversation")return{x:84-extra,y:148+index*94};if(kind==="memory"||kind==="knowledge")return{x:260+index*240,y:380};
 return{x:636+extra,y:108+index*96};
}
export function nextCanvasAfterDelete(ids:string[],active:string|null,deleted:string):string|null{if(active!==deleted)return active;const i=ids.indexOf(deleted);return ids[i+1]??ids[i-1]??null;}

export function planConnectorDeletion(nodes:Array<{id:string;kind:string}>,edges:Array<{id:string;source:string;target:string}>){
 const hiddenNodeIds=nodes.filter(node=>node.kind!=="agent").map(node=>node.id);const hidden=new Set(hiddenNodeIds);
 return {hiddenNodeIds,unboundEdgeIds:edges.filter(edge=>!edge.id.startsWith("derived:")&&!hidden.has(edge.source)&&!hidden.has(edge.target)).map(edge=>edge.id)};
}
