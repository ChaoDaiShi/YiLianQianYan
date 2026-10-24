// Real model + production Agent loop + real Windows Notepad. No model fixture.
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { resolve, join, relative } from 'node:path';
import { randomBytes, createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { desktopGate } from './smoke_contract.mjs';
import { startModelBudgetProxy } from './model_budget_proxy.mjs';

const root = resolve(import.meta.dirname, '../..');
const runRoot = resolve(process.env.YILIAN_SMOKE_DIR ?? `F:/tmp/yilian-adaptive-${new Date().toISOString().replace(/[:.]/g,'-')}`);
const workspace = join(runRoot, 'workspace');
const data = join(runRoot, 'data');
const documentName = `adaptive-${randomBytes(6).toString('hex')}.txt`;
const output = join(workspace, documentName);
const launchShortcut = join(workspace, 'notepad.lnk');
const port = Number(process.env.YILIAN_SMOKE_PORT ?? 9448);
const base = `http://127.0.0.1:${port}`;
const token = randomBytes(32).toString('hex');
const headers = { 'Content-Type':'application/json', 'X-Yilian-Control-Session':token };
const evidence = { schema_version:1, status:'RUNNING', source_commit:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(), real_model:true, real_desktop:true, model_fixture:false, operator:'production_chat_agent', approval_actor:'task_authorized_bounded_test_harness', environment_change_trigger:'explicit_user_event_between_turns', runRoot, checks:[], events:[], approvals:[] };
assert.ok(relative(root,runRoot).startsWith('..'),'Test data must be outside the checkout');
await mkdir(workspace,{recursive:true});await mkdir(data,{recursive:true});
execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',join(import.meta.dirname,'prepare_notepad.ps1'),'-Workspace',workspace,'-DocumentName',documentName],{windowsHide:true,encoding:'utf8'});
const eventsLog = createWriteStream(join(runRoot,'agent-events.jsonl'));
const backendLog = createWriteStream(join(runRoot,'backend.log'));
const env = {...process.env};for(const key of Object.keys(env))if(/KEY|TOKEN|SECRET|PASSWORD/i.test(key))delete env[key];
const backend = spawn(join(root,'target/debug/yilian-server.exe'),[],{cwd:root,env:{...env,YILIAN_DATA_DIR:data,YILIAN_WORKSPACE:workspace,YILIAN_HOST:`127.0.0.1:${port}`,YILIAN_ALLOWED_ORIGINS:`http://127.0.0.1:1448`,YILIAN_CONTROL_SESSION_TOKEN:token},windowsHide:true,stdio:['ignore','pipe','pipe']});
backend.stdout.pipe(backendLog);backend.stderr.pipe(backendLog);
let conversationId, toolCount=0, budgetProxy;
const started=Date.now();
const hash=value=>createHash('sha256').update(value).digest('hex');
async function api(method,path,body){const response=await fetch(base+path,{method,headers,body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(15000)});if(!response.ok)throw new Error(`${method} ${path}: ${response.status} ${await response.text()}`);const raw=await response.text();return raw?JSON.parse(raw):null;}
async function waitReady(){for(let attempt=0;attempt<100;attempt++){try{if((await fetch(base+'/api/health',{signal:AbortSignal.timeout(500)})).ok)return;}catch{}if(backend.exitCode!==null)throw new Error('Isolated backend exited');await new Promise(resolve=>setTimeout(resolve,200));}throw new Error('Isolated backend startup timeout');}
function allowedApproval(event){const a=event.args??{};if(event.tool_name==='open_application')return resolve(a.application??'').toLowerCase()===launchShortcut.toLowerCase();if(event.tool_name!=='keyboard')return false;if(a.action==='type')return /^(notepad(?:\.exe)?|记事本)$/i.test(a.target_application??'')&&typeof a.text==='string'&&a.text.length<=2000&&!a.text.includes(runRoot);if(a.action==='combo')return Array.isArray(a.keys)&&a.keys.length===2&&['ctrl','control'].includes(a.keys[0])&&['a','s'].includes(a.keys[1]);return false;}
async function readEvents(response,phase){if(!response.ok)throw new Error(`SSE ${response.status}: ${await response.text()}`);let buffer='',pending=null;const decoder=new TextDecoder();for await(const chunk of response.body){buffer+=decoder.decode(chunk,{stream:true});let at;while((at=buffer.indexOf('\n\n'))>=0){const frame=buffer.slice(0,at);buffer=buffer.slice(at+2);const raw=frame.split('\n').filter(line=>line.startsWith('data:')).map(line=>line.slice(5).trimStart()).join('\n');if(!raw)continue;const event=JSON.parse(raw);if(event.type==='reasoning')continue;if(event.type==='token')continue;const row={at:new Date().toISOString(),phase,...event};eventsLog.write(JSON.stringify(row)+'\n');evidence.events.push(row);if(event.type==='tool_start'){toolCount++;console.log(JSON.stringify({phase,tool:event.tool_name,args:event.args}));if(toolCount>12)throw new Error('Tool-call budget exceeded');}if(event.type==='approval_required')pending=event;if(event.type==='error')throw new Error(event.error||'Agent error');}}return pending;}
async function turn(message,phase){let response=await fetch(base+'/api/chat',{method:'POST',headers,body:JSON.stringify({conversation_id:conversationId,message,workspace_mode:'work',thinking_mode:'standard'}),signal:AbortSignal.timeout(180000)});let approvals=0;while(true){const pending=await readEvents(response,phase);if(!pending)break;if(!allowedApproval(pending)||++approvals>12)throw new Error('Approval outside bounded Notepad test scope');if(pending.tool_name==='keyboard'&&!desktopGate(await observe('gate-'+phase+'-'+approvals,false)))throw new Error('Active document identity or focused control is outside test scope');evidence.approvals.push({phase,approval_id:pending.approval_id,tool:pending.tool_name,args:pending.args});response=await fetch(base+`/api/approvals/${pending.approval_id}/approve`,{method:'POST',headers,body:JSON.stringify({conversation_id:conversationId}),signal:AbortSignal.timeout(180000)});}const conversation=await api('GET','/api/conversations/'+conversationId);await writeFile(join(runRoot,phase+'-conversation.json'),JSON.stringify(conversation,null,2));return conversation;}
async function observe(phase,capture=true){execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',join(import.meta.dirname,'observe_notepad.ps1'),'-OutputDirectory',runRoot,'-Phase',phase,'-ExpectedDocument',output,...(capture?['-Capture']:[])],{windowsHide:true,encoding:'utf8',timeout:30000});return JSON.parse(await readFile(join(runRoot,phase+'-observation.json'),'utf8'));}
const observedText=observation=>observation.windows.flatMap(window=>window.controls.map(control=>control.text??'')).join('\n');
try{
 await waitReady();
 budgetProxy=await startModelBudgetProxy({upstreamBase:'https://api.deepseek.com/v1',maxCny:Number(process.env.YILIAN_SMOKE_MAX_CNY??0.30),maxRequests:10,maxOutputTokens:512});
 const source=process.env.YILIAN_SMOKE_SOURCE_DATA??join(process.env.APPDATA,'yilianqianyan-v1');
 evidence.model=JSON.parse(execFileSync('python',[join(import.meta.dirname,'seed_model_profile.py'),source,data],{windowsHide:true,encoding:'utf8',env:{...env,YILIAN_SMOKE_PROXY_BASE:budgetProxy.baseUrl,YILIAN_SMOKE_MODEL:'deepseek-flash',YILIAN_SMOKE_MAX_OUTPUT_TOKENS:'512'}}));
 assert.equal(evidence.model.upstream_base_url,'https://api.deepseek.com/v1','The price book requires the verified official upstream');
 const readiness=await api('GET','/api/chat/readiness');assert.ok(readiness.available,'Configured model credential is not available');
 const existing=JSON.parse(execFileSync('powershell.exe',['-NoProfile','-Command',"[Console]::WriteLine((@(Get-Process -Name notepad -ErrorAction SilentlyContinue | Where-Object {$_.MainWindowHandle -ne 0}).Count))"],{windowsHide:true,encoding:'utf8'}));evidence.notepad_windows_before=existing;
 const registered=await api('POST','/api/workspaces',{name:'比赛版轻量桌面实测',root_path:workspace});
 const facts='项目名称：星桥。负责人：林岚。原会议地点：A201。此资料属于隔离测试，最终时间与地点以工作目录中的 current-task.json 最新版本为准。';
 const resourceResponse=await fetch(base+'/api/resources/ingest?name=project-facts.txt',{method:'POST',headers:{...headers,'Content-Type':'text/plain'},body:facts});assert.ok(resourceResponse.ok);const resource=await resourceResponse.json();
 const knowledge=await api('POST','/api/knowledge/documents',{resource_id:resource.id});
 const knowledgeDetail=await api('GET','/api/knowledge/documents/'+knowledge.document.id);
 assert.ok(knowledgeDetail.chunks.length,'Imported knowledge must expose actual chunk identities');
 const agent=await api('POST','/api/agents',{name:'桌面简报执行员',description:'根据项目资料和当前任务状态完成记事本简报',user_prompt:'只处理本次指定测试目录和记事本任务。资料版本变化时重新读取当前任务，冲突时以最新任务约束为准。不得用shell、文件写入工具或其他程序代替要求的桌面键盘操作。不得更改其他窗口。',allowed_tools:['read_file','open_application','keyboard'],max_iterations:8});
 const canvas=await api('POST','/api/connectors',{name:'比赛桌面实测上下文',agent_id:agent.id});
 const conversation=await api('POST','/api/conversations',{title:'真实桌面适应性轻量实测'});conversationId=conversation.id;
 await api('PUT',`/api/conversations/${conversationId}/workspace`,{workspace_id:registered.id});
 await api('POST',`/api/connectors/${canvas.canvas_id}/bindings`,{kind:'conversation_agent',source_id:'conversation:'+conversationId,target_id:'agent:'+agent.id});
 for(const chunk of knowledgeDetail.chunks)await api('POST',`/api/connectors/${canvas.canvas_id}/bindings`,{kind:'agent_knowledge',source_id:'agent:'+agent.id,target_id:'knowledge:'+chunk.memory_id});
 evidence.knowledge_document_id=knowledge.document.id;evidence.knowledge_chunk_ids=knowledgeDetail.chunks.map(chunk=>chunk.memory_id);
 const brief=join(workspace,'current-task.json');
 const revision1={revision:1,deadline:'2026-11-02',location:'A201',marker:'ENV-V1-STARBRIDGE',phase:'draft',output_path:output,software_entry:launchShortcut,initial_document:'precreated_empty_file'};
 await writeFile(brief,JSON.stringify(revision1,null,2));evidence.environment_before=revision1;
 evidence.conversation_id=conversationId;evidence.workspace_id=registered.id;
 await turn(`读取工作目录中的 current-task.json，结合已绑定项目知识，通过 software_entry 指定快捷方式打开已存在的空白测试文件，在记事本键入一行短简报，包含项目名、负责人、截止日期、地点和marker。只能使用给定快捷方式，不能新建其他标签。工具调用串行；本轮不保存，输入后简短结束等待更新。不要调用额外验证工具，独立验收脚本会读回。当前工作目录：${workspace}。`, 'phase1');
 const first=await observe('phase1');assert.ok(desktopGate(first),'First draft is not in the authorized document');const firstText=observedText(first);assert.ok(firstText.includes('ENV-V1-STARBRIDGE')&&firstText.includes('星桥')&&firstText.includes('林岚'),'First draft did not match actual Notepad text');evidence.checks.push('real_model_selected_tools_and_typed_knowledge_based_draft');
 const revision2={...revision1,revision:2,deadline:'2026-11-04',location:'B305',marker:'ENV-V2-STARBRIDGE',phase:'final',change_reason:'会议室与交付日期发生变更'};
 await writeFile(brief,JSON.stringify(revision2,null,2));evidence.environment_after=revision2;evidence.environment_changed_at=new Date().toISOString();
 await turn(`环境更新：current-task.json 已变为新版本。重新读取，用最新日期、地点和marker替换记事本当前简报，保留项目与负责人，全文一行，不保留旧信息。当前打开的文件就是 output_path，直接正常保存同一文件，不能另存或直接写文件。保存后简短结束，不调用额外验证工具；独立验收脚本会核对实际文件和窗口正文。`, 'phase2');
 const second=await observe('phase2');assert.ok(desktopGate(second),'Final observation is not the authorized document');const bytes=await readFile(output);let body=bytes[0]===255&&bytes[1]===254?bytes.subarray(2).toString('utf16le'):bytes.toString('utf8').replace(/^\uFEFF/,'');
 assert.ok(['星桥','林岚','2026-11-04','B305','ENV-V2-STARBRIDGE'].every(value=>body.includes(value)),'Saved output does not satisfy current task');assert.ok(!['ENV-V1-STARBRIDGE','A201','2026-11-02'].some(value=>body.includes(value)),'Saved output still contains superseded facts');
 const secondText=observedText(second);assert.ok(secondText.includes('ENV-V2-STARBRIDGE')&&secondText.includes('B305'),'Final desktop text not independently verified');
 evidence.checks.push('explicit_environment_change_reread_and_desktop_revision','actual_file_saved_via_desktop_and_independently_read','old_facts_removed_and_new_constraints_satisfied');
 evidence.output={path:output,size:bytes.length,sha256:hash(bytes),text:body};evidence.status='PASS';
}catch(error){evidence.status='FAIL';evidence.error=String(error);if(conversationId)await api('POST','/api/chat/stop',{conversation_id:conversationId}).catch(()=>{});try{await observe('failure');}catch(observerError){evidence.observer_error=String(observerError);}process.exitCode=1;}
finally{evidence.duration_ms=Date.now()-started;evidence.tool_calls=toolCount;if(budgetProxy)evidence.budget=await budgetProxy.finish();await writeFile(join(runRoot,'result.json'),JSON.stringify(evidence,null,2));eventsLog.end();backend.kill();console.log(JSON.stringify({status:evidence.status,error:evidence.error,checks:evidence.checks,tool_calls:toolCount,duration_ms:evidence.duration_ms,budget:evidence.budget,evidence:join(runRoot,'result.json')},null,2));}
