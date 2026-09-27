import type {CapabilityDescriptor} from '../../../api/capabilities';
import {hasHeaderBinding,HEADER_UNSUPPORTED} from '../schema-form/schema';
export function capabilityAvailability(tool?:CapabilityDescriptor){
  if(!tool)return {kind:'unavailable' as const,label:'能力当前不可用',reason:'连接或刷新能力来源后重试，节点绑定会保留。'};
  if(tool.kind!=='mcp_tool'||tool.provider!=='mcp')return {kind:'unavailable' as const,label:'暂不支持',reason:'当前类型暂不支持作为执行节点。'};
  if(hasHeaderBinding(tool.input_schema))return {kind:'unavailable' as const,label:'需要安全凭据绑定',reason:HEADER_UNSUPPORTED};
  if(!tool.enabled||tool.status!=='ready'||!tool.metadata.runtime_ready)return {kind:'unavailable' as const,label:'能力当前不可用',reason:'请检查能力来源连接状态。'};
  return {kind:'configured' as const,label:'可用 · 高风险',reason:'运行后通过安全网关申请审批。'};
}
