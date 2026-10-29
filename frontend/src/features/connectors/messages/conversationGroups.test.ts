import { describe, expect, it } from "vitest";
import { conversationGroupEntries, layoutConversationGroups } from "./messageForest";

const groups = [
  {id:"root",title:"First",nodes:[{id:"u",parent_id:null},{id:"a",parent_id:"u"}],branch:null},
  {id:"branch",title:"Alternative",nodes:[{id:"bu",parent_id:"a"},{id:"ba",parent_id:"bu"}],branch:{parent_node_id:"a"}},
  {id:"empty",title:"Empty fork",nodes:[],branch:{parent_node_id:"a"}},
  {id:"other",title:"Separate",nodes:[{id:"ou",parent_id:null},{id:"oa",parent_id:"ou"}],branch:null},
];

describe("isolated conversation groups",()=>{
  it("shows only the selected family, including its empty forks",()=>{
    expect(conversationGroupEntries(groups,"root").map(entry=>entry.id)).toEqual(["root","branch","empty"]);
    expect(conversationGroupEntries(groups,"other").map(entry=>entry.id)).toEqual(["other"]);
  });
  it("does not turn an empty or missing selection into every conversation",()=>{
    expect(conversationGroupEntries(groups,"")).toEqual([]);
    expect(conversationGroupEntries(groups,"deleted")).toEqual([]);
  });
  it("gives unrelated groups independent layout space while preserving fork separation",()=>{
    const positions=layoutConversationGroups(groups);
    expect(positions.u).toEqual(positions.ou);
    expect(positions.a).toEqual(positions.oa);
    expect(positions.bu.y).toBeGreaterThan(positions.a.y);
    expect(Object.keys(positions).sort()).toEqual(["u","a","bu","ba","ou","oa"].sort());
  });
});
