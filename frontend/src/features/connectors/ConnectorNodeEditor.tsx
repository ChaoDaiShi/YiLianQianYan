import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ConnectorNode, ConnectorResource } from "../../api/connectors";
import { connectorRequest } from "../../api/connectors";
import { loadConversation } from "../../api/conversations";
import { getMemory, updateMemory } from "../../api/memory";
import { listLlmModels, updateLlmModel } from "../../api/system";
import type { LlmModel, LlmModelPayload } from "../../types";
import { Button, ErrorState, Input, Modal, Textarea } from "../../components/ui";
import PersonaDialog from "../agents/PersonaDialog";
import { AccessMatrix } from "../security/AccessMatrix";
import SkillsPage from "../../pages/SkillsPage";
import PluginsPage from "../../pages/PluginsPage";
import WorkflowsPage from "../../pages/WorkflowsPage";
import ToolManager from "../capabilities/ToolManager";
import ManagedImports from "../capabilities/ManagedImports";
import BasicServicePanel from "../settings/sections/BasicServicePanel";
import ModelPoolPanel from "../llm/ModelPoolPanel";
export default function ConnectorNodeEditor({ node, onClose, onSaved, resources, onBind, bindingActions }: { node: ConnectorNode | null; onClose: () => void; onSaved: () => void; resources: ConnectorResource[]; onBind: (resource: ConnectorResource) => Promise<void>; bindingActions: React.ReactNode }) {
 if (!node) return null;
 if (node.kind === "agent") return <PersonaDialog open agentId={node.entity_id} onClose={onClose} onSaved={onSaved} />;
 return <Modal open onClose={onClose} title={node.label} size="xl" className="min-h-[50dvh] max-h-[90dvh]"><div className="min-h-0 space-y-4">{node.kind === "constraint" ? <AccessMatrix agentId={node.entity_id} agentLabel={resources.find(resource=>resource.kind==="agent"&&resource.entity_id===node.entity_id)?.label??node.entity_id} /> : node.kind === "conversation" ? <ConversationEditor node={node} onSaved={onSaved} /> : node.kind === "memory" ? <MemoryEditor node={node} onSaved={onSaved} /> : node.kind === "model" && (node.service_kind === "vision" || node.service_kind === "image") ? <BasicServicePanel kind={node.service_kind} /> : node.kind === "model" ? <ModelEditor node={node} resources={resources.filter(r=>r.kind==="model"&&(!r.service_kind||r.service_kind==="chat"))} onBind={onBind} /> : node.kind === "skill" ? <div className="h-[65dvh]"><SkillsPage embedded initialSkill={node.entity_id} /></div> : node.kind === "mcp" ? <div className="h-[65dvh]"><PluginsPage embedded initialServerId={node.entity_id} /></div> : node.kind === "workflow" ? <div className="h-[68dvh]"><WorkflowsPage embedded initialGraphId={node.entity_id} /></div> : node.kind === "plugin" ? <ManagedImports kind="plugin" /> : node.kind === "tool" ? <div className="h-[65dvh]"><ToolManager initialTool={node.entity_id} /></div> : node.kind === "knowledge" ? <KnowledgePreview node={node} onSaved={onSaved} /> : <p>此资源尚无可用编辑器。</p>}{bindingActions}</div></Modal>;
}
function ConversationEditor({ node, onSaved }: { node: ConnectorNode; onSaved: () => void }) {
 const [title,setTitle]=useState(node.label);const [busy,setBusy]=useState(false);const [error,setError]=useState("");const navigate=useNavigate();
 useEffect(()=>{void loadConversation(node.entity_id).then(c=>setTitle(c.title)).catch(cause=>setError(String(cause)));},[node.entity_id]);
 return <div className="space-y-4"><Input label="对话名称" value={title} onChange={event=>setTitle(event.target.value)}/><div className="flex gap-2"><Button disabled={busy||!title.trim()} onClick={()=>{setBusy(true);setError("");void connectorRequest("PUT",`/api/conversations/${encodeURIComponent(node.entity_id)}`,{title}).then(onSaved).catch(cause=>setError(String(cause))).finally(()=>setBusy(false));}}>保存名称</Button><Button variant="secondary" onClick={()=>navigate(`/chat/${encodeURIComponent(node.entity_id)}`)}>继续此对话</Button></div>{error&&<ErrorState title="对话未更新" description={error}/>}</div>;
}
function MemoryEditor({ node, onSaved }: { node: ConnectorNode; onSaved: () => void }) {
 const [content,setContent]=useState("");const [category,setCategory]=useState("");const [ready,setReady]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
 useEffect(()=>{let active=true;void getMemory(node.entity_id).then(m=>{if(active){setContent(m.content);setCategory(m.category);setReady(true);}}).catch(cause=>{if(active)setError(String(cause));});return()=>{active=false;};},[node.entity_id]);
 return <div className="space-y-4"><Input label={node.kind==="knowledge"?"知识分类":"记忆分类"} value={category} onChange={event=>setCategory(event.target.value)}/><Textarea label={node.kind==="knowledge"?"知识内容":"记忆内容"} rows={9} value={content} onChange={event=>setContent(event.target.value)}/><Button disabled={!ready||busy||!content.trim()} onClick={()=>{setBusy(true);setError("");void updateMemory(node.entity_id,{content,category}).then(onSaved).catch(cause=>setError(String(cause))).finally(()=>setBusy(false));}}>{node.kind==="knowledge"?"保存知识":"保存记忆"}</Button>{error&&<ErrorState title="记忆未更新" description={error}/>}</div>;
}
function KnowledgePreview({ node,onSaved }: { node: ConnectorNode;onSaved:()=>void }) {
 return <MemoryEditor node={node} onSaved={onSaved} />;
}

function ModelEditor({ node, resources, onBind }: { node: ConnectorNode; resources: ConnectorResource[]; onBind: (resource: ConnectorResource) => Promise<void> }) {
 const [models,setModels]=useState<LlmModel[]>([]);const [selected,setSelected]=useState(node.entity_id);const [error,setError]=useState("");const [busy,setBusy]=useState(false);
 useEffect(()=>{void listLlmModels().then(value=>{if(value)setModels(value);else setError("模型档案无法加载。");});},[]);
 return <div className="space-y-5"><label className="block text-sm">本智能体的对话模型<select aria-label="绑定模型" value={resources.find(r=>r.entity_id===selected)?.id??""} onChange={event=>{const resource=resources.find(r=>r.id===event.target.value);if(resource){setSelected(resource.entity_id);setBusy(true);void onBind(resource).catch(cause=>setError(String(cause))).finally(()=>setBusy(false));}}} disabled={busy} className="settings-select"><option value="">选择真实模型档案或模型池</option>{resources.map(resource=><option value={resource.id} key={resource.id}>{resource.label} · {resource.model_source==="pool"?"模型池":"模型档案"}</option>)}</select></label>{error&&<ErrorState title="模型操作未完成" description={error}/>}<ModelPoolPanel modelManager={<div className="space-y-3">{models.map(model=><ModelProfileForm key={model.id} model={model}/>)}</div>}/></div>;
}
function ModelProfileForm({ model }: { model: LlmModel }) {
 const [draft,setDraft]=useState<LlmModelPayload>({provider:model.provider,label:model.label,model:model.model,base_url:model.base_url,api_format:"openai",temperature:model.temperature,max_tokens:model.max_tokens,invoke_timeout_ms:model.invoke_timeout_ms});const [open,setOpen]=useState(false);const [error,setError]=useState("");const [busy,setBusy]=useState(false);
 return <article className="rounded-xl border border-[var(--border-soft)] p-3"><div className="flex items-center justify-between"><span>{model.label} · {model.model}</span><Button size="sm" onClick={()=>setOpen(!open)}>{open?"收起":"编辑档案"}</Button></div>{open&&<div className="space-y-3 mt-3">{(["label","model","base_url"] as const).map(key=><Input key={key} label={({label:"显示名称",model:"模型 ID",base_url:"API 地址"})[key]} value={draft[key]} onChange={event=>setDraft({...draft,[key]:event.target.value})}/>)}<Button disabled={busy} onClick={()=>{setBusy(true);setError("");void updateLlmModel(model.id,draft).then(result=>{if(!result)throw new Error("模型档案未保存。");setOpen(false);}).catch(cause=>setError(String(cause))).finally(()=>setBusy(false));}}>保存档案</Button>{error&&<p role="alert">{error}</p>}</div>}</article>;
}
