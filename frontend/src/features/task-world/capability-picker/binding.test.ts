import {describe,it,expect} from 'vitest';
import {capabilityAvailability} from './binding';
import type {CapabilityDescriptor} from '../../../api/capabilities';
const tool:CapabilityDescriptor={id:'mcp.a_echo',name:'echo',description:'local',kind:'mcp_tool',provider:'mcp',risk:'high',permissions:[],status:'ready',enabled:true,metadata:{runtime_ready:true,tags:[],extra:{}},input_schema:{type:'object',properties:{}}};
describe('canvas capability availability',()=>{
 it('admits ready MCP tools only',()=>{expect(capabilityAvailability(tool).kind).toBe('configured');expect(capabilityAvailability({...tool,kind:'workflow'}).kind).toBe('unavailable');});
 it('fails closed for removed, disabled, disconnected and header-bound tools',()=>{for(const candidate of [undefined,{...tool,enabled:false},{...tool,metadata:{...tool.metadata,runtime_ready:false}},{...tool,status:'unavailable' as const},{...tool,input_schema:{'x-mcp-header':'X-Token'}}])expect(capabilityAvailability(candidate).kind).toBe('unavailable');});
});
