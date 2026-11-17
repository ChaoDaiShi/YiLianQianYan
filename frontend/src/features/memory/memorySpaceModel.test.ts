import { expect, it } from "vitest";
import type { MemoryRecord, MemoryVersion } from "../../api/memory";
import type { MemoryGraphSnapshot } from "./memoryGraphApi";
import { projectMemoryGraph } from "./memoryGraphModel";
import { memorySpace } from "./memorySpaceModel";

const record = (id: string, content: string): MemoryRecord => ({ id, content, category: "note", source: "manual", created_at: 1, updated_at: 1 });
const version = (record: MemoryRecord, layer: number, content: string): MemoryVersion => ({
  ...record, revision: layer + 1, version_id: `${record.id}:${layer + 1}`, layer, baseline: false,
  content, scope: "profile", status: "active", source_ref: null, reason: "", recorded_at: layer + 1,
  relationship_known: true, valid_from: null, valid_until: null,
});

it("shares memory identity and excludes pending links by default while 3D anchors real historical endpoints", () => {
  const a = record("a", "当前甲"), b = record("b", "当前乙"), c = record("c", "当前丙");
  const snapshot: MemoryGraphSnapshot = {
    memories: [a, b, c], histories: {
      a: [version(a, 2, a.content), version(a, 1, "甲第二版"), version(a, 0, "甲初版")],
      b: [version(b, 0, b.content)], c: [version(c, 0, c.content)],
    },
    current_version_ids: {a:"a:3",b:"b:1",c:"c:1"},
    relations: [
      {id:"confirmed",source_memory_id:"a",target_memory_id:"b",label:"对应",status:"confirmed",revision:2,created_at:1,updated_at:3},
      {id:"pending",source_memory_id:"b",target_memory_id:"c",label:"线索",status:"pending",revision:1,created_at:1,updated_at:1},
    ],
    relation_versions: [
      {relation_id:"confirmed",revision:1,label:"旧称",status:"confirmed",recorded_at:1,source_version_id:"a:1",target_version_id:"b:1"},
      {relation_id:"confirmed",revision:2,label:"对应",status:"confirmed",recorded_at:3,source_version_id:"a:3",target_version_id:"b:1"},
    ], next_offset:null,
  };
  const graph = projectMemoryGraph(snapshot, "network");
  expect(graph.overview).toEqual(["memory:a","memory:b","memory:c"]);
  expect(graph.nodes.get("memory:a")?.label).toBe("当前甲");
  expect(graph.edges.map(edge => edge.id)).toEqual(["confirmed"]);
  expect(projectMemoryGraph(snapshot, "network", true).edges[1]).toMatchObject({id:"pending",inferred:true});
  const scene = memorySpace(snapshot, { positions:new Map(graph.overview.map((id, i) => [id, {x:i*100,y:20}])), edges:graph.edges }, false);
  expect(scene.items.map(item => [item.id, item.versions.length])).toEqual([["a",3],["b",1],["c",1]]);
  expect(scene.edges).toEqual([
    expect.objectContaining({id:"confirmed",directed:false}),
    expect.objectContaining({id:"confirmed:1",historical:true,sourceVersionId:"a:1",targetVersionId:"b:1"}),
  ]);
  const withoutCurrentEdge = memorySpace({...snapshot,relations:[]}, {
    positions:new Map(graph.overview.map((id,i)=>[id,{x:i*100,y:20}])),edges:[],
  },false);
  expect(withoutCurrentEdge.edges).toHaveLength(2);
  expect(withoutCurrentEdge.edges[0]).toMatchObject({sourceVersionId:"a:1",targetVersionId:"b:1",historical:true});
  const purged = { ...snapshot, memories: [b, c], histories: { b: snapshot.histories.b, c: snapshot.histories.c }, relations: snapshot.relations.slice(1), relation_versions: [] };
  expect(projectMemoryGraph(purged, "network").edges).toEqual([]);
});

it("uses the authoritative older head and only draws a linked change with actual version anchors", () => {
  const a = record("a", "当前甲"), b = record("b", "当前乙");
  const snapshot: MemoryGraphSnapshot = {
    memories:[a,b], current_version_ids:{a:"a:1",b:"b:2"},
    histories:{a:[version(a,1,"未来甲"),version(a,0,a.content)],b:[version(b,1,b.content),version(b,0,"原始乙")]},
    relations:[],relation_versions:[],next_offset:null,
    change_events:[{id:"change-b",memory_id:"b",version_id:"b:2",recorded_at:9,kind:"correction",
      related_event_id:"change-a",related_memory_id:"a",related_version_id:"a:1"}],
  };
  const graph = projectMemoryGraph(snapshot,"network");
  expect(graph.nodes.get("memory:a")?.changes).toBe(0);
  const projection = {positions:new Map([["memory:a",{x:0,y:0}],["memory:b",{x:120,y:0}]]),edges:graph.edges};
  const scene = memorySpace(snapshot, projection, false);
  expect(scene.items[0].currentVersionId).toBe("a:1");
  expect(scene.edges).toEqual([expect.objectContaining({event:true,sourceVersionId:"a:1",targetVersionId:"b:2"})]);
  expect(memorySpace({...snapshot,change_events:[]}, projection, false).edges).toEqual([]);
});
