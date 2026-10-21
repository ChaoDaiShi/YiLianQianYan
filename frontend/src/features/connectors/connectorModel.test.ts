import { describe, expect, it } from "vitest";
import { connectorPosition, visibleConnectorNodes, nextCanvasAfterDelete,planConnectorDeletion } from "./connectorModel";
import type { ConnectorNode, ConnectorDisplay } from "../../api/connectors";
const display: ConnectorDisplay = {mode:"conversations",visible_kinds:[],hidden_node_ids:[],show_edges:true,show_labels:true,density:"compact"};
const node=(kind:string,id=kind):ConnectorNode=>({id:`${kind}:${id}`,kind,entity_id:id,label:id,x:0,y:0});
describe("connector view behavior",()=>{
 it("filters only the view and retains the central agent",()=>{const source=[node("agent"),node("conversation"),node("tool")];expect(visibleConnectorNodes(source,display).map(n=>n.kind)).toEqual(["agent","conversation"]);expect(source).toHaveLength(3);});
 it("custom hiding cannot remove the center",()=>expect(visibleConnectorNodes([node("agent"),node("conversation")],{...display,mode:"custom",visible_kinds:[],hidden_node_ids:["agent:agent"]}).map(n=>n.kind)).toEqual(["agent"]));
 it("deleting a noncurrent canvas retains the active identity",()=>expect(nextCanvasAfterDelete(["a","b","c"],"a","b")).toBe("a"));
 it("selects next then previous, and leaves last deletion empty",()=>{expect(nextCanvasAfterDelete(["a","b","c"],"b","b")).toBe("c");expect(nextCanvasAfterDelete(["a","b"],"b","b")).toBe("a");expect(nextCanvasAfterDelete(["a"],"a","a")).toBeNull();});
 it("uses compact side gaps and deterministic per-column positions",()=>{expect(connectorPosition("agent",0,"compact")).toEqual({x:360,y:186});expect(connectorPosition("conversation",0,"compact")).toEqual({x:84,y:148});expect(connectorPosition("tool",0,"compact").x).toBe(636);});
});

describe("canvas deletion intent",()=>{
 it("hides a node without unbinding incident edges, while deleting independent selected edges",()=>{
  expect(planConnectorDeletion([{id:"memory:m",kind:"memory"}],[{id:"binding-m",source:"agent:a",target:"memory:m"},{id:"binding-tool",source:"agent:a",target:"tool:t"}])).toEqual({hiddenNodeIds:["memory:m"],unboundEdgeIds:["binding-tool"]});
 });
 it("unbinding an edge does not hide either endpoint and cannot remove the derived constraint",()=>{
  expect(planConnectorDeletion([],[{id:"binding",source:"agent:a",target:"tool:t"},{id:"derived:c",source:"constraint:a",target:"agent:a"}])).toEqual({hiddenNodeIds:[],unboundEdgeIds:["binding"]});
 });
});
