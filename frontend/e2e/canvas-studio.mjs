import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function canvasStudio({page,backendPort,frontendPort,controlToken,evidenceDir,stage}) {
  const records=[];
  const api=async(method,path,body)=>{
    const r=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{"Content-Type":"application/json","X-Yilian-Control-Session":controlToken},body:body===undefined?undefined:JSON.stringify(body)});
    const data=await r.json(); assert.ok(r.ok,`${path}: ${r.status}`);return data;
  };
  const workflow=await api("POST","/api/workflow-graphs",{name:"Studio local output",definition:{schema_version:1,entry_node_id:"out",nodes:[{id:"out",kind:"output",config:{type:"output",template:"Studio verified local result"}}],edges:[]}});
  const id="stable-studio-c2",path=`/api/task-world/graphs/${id}`;
  const definitions=[
    {id:"A",kind:"work",title:"准备发布内容",input:{instruction:"整理已确认的内容，准备交给下一步处理。",executor_ref:`workflow://${workflow.id}`}},
    {id:"B",kind:"work",title:"生成结果摘要",input:{instruction:"汇总关键结果，保留清晰的执行记录。",executor_ref:`workflow://${workflow.id}`}},
    {id:"C",kind:"approval",title:"发布前审批",input:{instruction:"核对内容与范围，再由现有审批流程确认。"}},
    {id:"D",kind:"user_checkpoint",title:"人工确认",input:{instruction:"等待用户确认最终结果。"}},
  ].map(n=>({...n,retry_policy:{max_attempts:1}}));
  await api("POST","/api/task-world/graphs",{id,nodes:definitions,edges:[{from:"A",to:"B"},{from:"B",to:"C"},{from:"B",to:"D"}]});
  const view=(await api("GET",`${path}/canvas-view`)).view;
  await api("PUT",`${path}/canvas-view`,{...view,expected_view_revision:view.view_revision,viewport:{x:20,y:0,zoom:1},node_layouts:definitions.map((n,i)=>({node_id:n.id,x:40+Math.min(i,2)*320,y:i===3?380:180,width:240,height:128}))});
  for(const [width,height] of [[1280,720],[1366,768],[1920,1080],[2560,1440]]) {
    await page.setViewportSize({width,height});
    await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${id}`,{waitUntil:"domcontentloaded"});
    await page.locator('.task-world-node').first().waitFor();
    await page.evaluate(()=>document.fonts.ready);
    await page.screenshot({path:join(evidenceDir,`${stage}-${width}x${height}.png`)});
    records.push({width,height,...await studioGeometry(page)});
    await writeFile(join(evidenceDir,"studio-geometry.json"),JSON.stringify(records,null,2));
  }
  return {status:"passed",stage,records:records.length,real_backend:true};
}

export async function studioGeometry(page) {
  return page.evaluate(()=>{
    const rect=el=>{if(!el)return null;const r=el.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};};
    return {
      viewport:{width:innerWidth,height:innerHeight},overflow:document.documentElement.scrollWidth>innerWidth,
      canvas:rect(document.querySelector('.task-world-canvas')),header:rect(document.querySelector('.studio-header,.page-header')),
      toolbar:rect(document.querySelector('.studio-toolbar,[aria-label="画布布局与镜头"]')),
      camera:document.querySelector('.react-flow__viewport')?.style.transform,
      nodes:[...document.querySelectorAll('.react-flow__node')].map(el=>({id:el.dataset.id,position:el.style.transform,card:rect(el.querySelector('.task-world-node')),width:el.querySelector('.task-world-node')?.offsetWidth,height:el.querySelector('.task-world-node')?.offsetHeight})),
      panels:[...document.querySelectorAll('.studio-panel')].map(el=>({name:el.getAttribute('aria-label'),hidden:el.hidden,box:rect(el)})),
    };
  });
}
