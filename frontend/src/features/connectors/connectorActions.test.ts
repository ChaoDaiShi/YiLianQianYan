import { beforeEach, describe, expect, it, vi } from "vitest";
import { bindingForResource } from "./connectorActions";
import { activeGraphExecutions, stopAndDeleteHistoricalGraph } from "../workflows/taskGraphActions";
import { cancelTaskExecution, deleteTaskGraph, getTaskGraphDetail } from "../../api/taskWorld";
import type { TaskGraphDetail } from "../../api/taskWorld";
vi.mock("../../api/taskWorld",()=>({cancelTaskCommand:vi.fn(),cancelTaskExecution:vi.fn(),deleteTaskGraph:vi.fn(),getTaskGraphDetail:vi.fn()}));
function detail(running=false):TaskGraphDetail {return {graph_id:"selected-history",revision:8,nodes:running?[{id:"work",state:{command_execution:null},latest_execution:{execution_id:"execution-selected",status:"running"}}]:[]} as TaskGraphDetail;}
beforeEach(()=>{vi.resetAllMocks();});
describe("real connector operations",()=>{
 it("writes model source and service while preserving conversation direction",()=>{
   expect(bindingForResource("agent-a",{id:"model:pool-a",kind:"model",entity_id:"pool-a",label:"Pool A",model_source:"pool"})).toEqual({kind:"agent_model",source_id:"agent:agent-a",target_id:"model:pool-a",service_kind:"chat",model_source:"pool"});
   expect(bindingForResource("agent-a",{id:"conversation:one",kind:"conversation",entity_id:"one",label:"One"})).toEqual({kind:"conversation_agent",source_id:"conversation:one",target_id:"agent:agent-a"});
   expect(bindingForResource("agent-a",{id:"model:vision-a",kind:"model",entity_id:"vision-a",label:"Vision A",model_source:"profile",service_kind:"vision"}).service_kind).toBe("vision");
   expect(()=>bindingForResource("agent-a",{id:"model:missing",kind:"model",entity_id:"missing",label:"Missing"})).toThrow();
 });
 it("deletes the specified historical graph using refreshed revision",async()=>{
   vi.mocked(getTaskGraphDetail).mockResolvedValue({ok:true,data:detail()});vi.mocked(deleteTaskGraph).mockResolvedValue({ok:true,data:undefined});
   await stopAndDeleteHistoricalGraph("selected-history");
   expect(deleteTaskGraph).toHaveBeenCalledWith("selected-history",8);
 });
 it("retains the graph when an active execution cannot be cancelled",async()=>{
   vi.mocked(getTaskGraphDetail).mockResolvedValue({ok:true,data:detail(true)});vi.mocked(cancelTaskExecution).mockResolvedValue({ok:false,error:{status:409,code:"conflict",message:"执行状态发生变化"}});
   expect(activeGraphExecutions(detail(true)).size).toBe(1);
   await expect(stopAndDeleteHistoricalGraph("selected-history")).rejects.toThrow("执行状态发生变化");
   expect(cancelTaskExecution).toHaveBeenCalledWith("selected-history","execution-selected",8);
   expect(deleteTaskGraph).not.toHaveBeenCalled();
 });
});
