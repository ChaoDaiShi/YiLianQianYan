import { describe, expect, it } from "vitest";
import { conversationFamilies, layoutMessageForest } from "./messageForest";

const conversations = [
 {id:"root",title:"Root",nodes:[{id:"u1",parent_id:null},{id:"a1",parent_id:"u1"},{id:"u-next",parent_id:"a1"}],branch:null},
 {id:"branch",title:"Branch",nodes:[{id:"u2",parent_id:"a1"},{id:"a2",parent_id:"u2"}],branch:{parent_node_id:"a1"}},
 {id:"nested",title:"Nested",nodes:[{id:"u3",parent_id:"a2"}],branch:{parent_node_id:"a2"}},
 {id:"other",title:"Other",nodes:[{id:"independent",parent_id:null}],branch:null},
];
describe("message forest",()=>{
 it("lays out only actual message IDs and places alternative paths beside the source path",()=>{
  const result=layoutMessageForest(conversations);
  expect(Object.keys(result).sort()).toEqual(conversations.flatMap(conversation=>conversation.nodes.map(node=>node.id)).sort());
  expect(result.u2.x).toBeGreaterThan(result["u-next"].x);
  expect(result.u2.y).toBe(result["u-next"].y);
  expect(result.independent.x).toBeGreaterThan(result.u3.x);
 });
 it("keeps forks and nested alternatives in one family and independent conversations separate",()=>{
  expect(conversationFamilies(conversations)).toEqual([{id:"root",title:"Root",conversationIds:["root","branch","nested"]},{id:"other",title:"Other",conversationIds:["other"]}]);
 });
});
