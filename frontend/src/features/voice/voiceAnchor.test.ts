import { describe,it,expect } from "vitest";
import { projectVoiceContextPatch,selectBrowsingCallContext } from "./context/VoiceContext";
import type { GlobalVoiceContextSnapshot } from "./model/types";
const a:GlobalVoiceContextSnapshot={focused_surface:"conversation",conversational_anchor:{conversation_id:"a",title:"对话 A",updated_at:1},active_task:null};
describe("pinned call conversation",()=>{
 it("keeps active A and its identity while browsing B and then a page without a conversation",()=>{
  const b=projectVoiceContextPatch(a,a,{focused_surface:"conversation",conversational_anchor:{conversation_id:"b",title:"对话 B",updated_at:2}},true);
  expect(b.active).toBe(a);expect(b.anchorChanged).toBe(false);expect(b.browsing.conversational_anchor?.conversation_id).toBe("b");
  const workspace=projectVoiceContextPatch(b.active,b.browsing,{focused_surface:"workspace"},true);
  expect(workspace.active).toBe(a);expect(workspace.anchorChanged).toBe(false);expect(workspace.browsing.focused_surface).toBe("workspace");
 });
 it("applies a new conversation only through explicit selection and never selects a non-conversation page",()=>{
  const patch={focused_surface:"conversation" as const,conversational_anchor:{conversation_id:"b",title:"对话 B",updated_at:2}};
  const browsing=projectVoiceContextPatch(a,a,patch,true);
  const explicit=selectBrowsingCallContext(a,browsing.browsing);
  expect(explicit.conversational_anchor?.conversation_id).toBe("b");
  expect(selectBrowsingCallContext(a,{...browsing.browsing,focused_surface:"workspace"})).toBe(a);
 });
});
