import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function canvasStudio({page,backendPort,frontendPort,controlToken,evidenceDir,stage}) {
  const records=[];
  const checks=[];
  const camera=()=>page.locator('.react-flow__viewport').evaluate(el=>el.style.transform);
  const positions=()=>page.locator('.react-flow__node').evaluateAll(els=>els.map(el=>[el.dataset.id,el.style.transform]));
  const button=name=>page.getByRole('button',{name,exact:true});
  const until=async(read,predicate,label)=>{
    const deadline=Date.now()+15000;
    do { const value=await read(); if(predicate(value))return value; await new Promise(resolve=>setTimeout(resolve,100)); } while(Date.now()<deadline);
    throw new Error(label);
  };
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
    const geometry=await studioGeometry(page);
    records.push({width,height,...geometry});
    await writeFile(join(evidenceDir,"studio-geometry.json"),JSON.stringify(records,null,2));
    if(stage==='before')continue;
    assert.equal(geometry.overflow,false);
    assert.equal(geometry.header.height,60);
    assert.equal(geometry.canvas.width,width-geometry.canvas.x);
    assert.equal(geometry.camera,'translate(20px, 0px) scale(1)');
    assert.ok(geometry.panels.every(p=>p.hidden),'panels start collapsed');
    assert.ok(geometry.nodes.every(n=>n.width===240&&n.height===128),'fixed node geometry');
    assert.equal(await button('发布 · 未接入').isDisabled(),true);
    const controls=await page.locator('.studio-toolbar button').evaluateAll(els=>els.map(el=>{
      const r=el.getBoundingClientRect(),top=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
      return {name:el.getAttribute('aria-label')||el.textContent,reachable:el.contains(top)&&r.x>=0&&r.y>=0&&r.right<=innerWidth&&r.bottom<=innerHeight};
    }));
    assert.deepEqual(controls.map(c=>c.name),['Zoom Out','Zoom In','适应全部','100%','自动布局','添加任务节点','从能力添加','运行选中节点']);
    assert.ok(controls.every(c=>c.reachable),JSON.stringify(controls));
    geometry.controls=controls;
    const originalPositions=await positions(),originalCamera=await camera();
    await page.locator('.react-flow__node[data-id="A"]').click();
    assert.equal(await camera(),originalCamera,'select does not move camera');
    assert.equal(await page.locator('.studio-panel:visible').count(),0,'selection does not open panels');
    await button('节点属性').click();
    const title=page.getByLabel('标题',{exact:true});
    await title.fill(`草稿 ${width}`);
    await button('执行轨迹').click();
    assert.equal(await page.locator('.studio-panel:visible').count(),1);
    await button('节点属性').click();
    assert.equal(await title.inputValue(),`草稿 ${width}`,'panel switch preserves draft');
    const extension=page.getByTestId('studio-extension-slots');
    await extension.locator('summary').click();
    assert.equal(await button('发布动作 · 未接入').isDisabled(),true);
    await extension.locator('summary').click();
    await title.scrollIntoViewIfNeeded();
    await page.screenshot({path:join(evidenceDir,`${stage}-inspector-${width}.png`)});
    await button('保存语义').click({trial:true});
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.studio-panel:visible').count(),0);
    assert.equal(await button('节点属性').evaluate(el=>el===document.activeElement),true,'Escape returns focus');
    for(const name of ['执行轨迹','更多']) {
      await button(name).click();
      const box=await page.locator('.studio-panel:visible').boundingBox();
      assert.ok(box.x>=0&&box.y>=60&&box.x+box.width<=width&&box.y+box.height<=geometry.toolbar.y,'panel fits above toolbar');
      await page.screenshot({path:join(evidenceDir,`${stage}-${name==='更多'?'tools':'trail'}-${width}.png`)});
      await page.keyboard.press('Escape');
    }
    assert.equal(await camera(),originalCamera,'all panel operations preserve camera');
    assert.deepEqual(await positions(),originalPositions,'no implicit layout');
    checks.push({viewport:`${width}x${height}`,controls,panels:'collapsed, exclusive, draft retained, Escape focus restored',camera_unchanged:true,node_geometry_unchanged:true});
    await writeFile(join(evidenceDir,'studio-checks.json'),JSON.stringify(checks,null,2));
  }
  if(stage!=='before') {
    const originalCamera=await camera(),originalPositions=await positions();
    let release,entered;
    const held=new Promise(resolve=>{release=resolve;});
    const pending=new Promise(resolve=>{entered=resolve;});
    await page.route(`**${path}/canvas-view`,async route=>{
      if(route.request().method()==='PUT'){entered();await held;} await route.continue();
    });
    await page.locator('.react-flow__node[data-id="B"]').click();
    try {
      await Promise.race([pending,new Promise((_,reject)=>setTimeout(()=>reject(new Error('selection did not save')),10000))]);
      await page.locator('[data-save-state="saving"]').waitFor();
      assert.equal(await camera(),originalCamera);
    } finally {release();}
    await page.locator('[data-save-state="synced"]').waitFor();
    await page.unroute(`**${path}/canvas-view`);
    await page.locator('.react-flow__node[data-id="A"]').click();
    await page.locator('[data-save-state="synced"]').waitFor();
    await button('运行选中节点').click();
    await page.locator('.react-flow__node[data-id="A"] [data-execution-status="succeeded"]').waitFor({timeout:15000});
    const detail=(await api('GET',`${path}/detail`)).detail;
    await writeFile(join(evidenceDir,'real-execution.json'),JSON.stringify(detail,null,2));
    assert.equal(await camera(),originalCamera,'toolbar execution preserves camera');
    assert.deepEqual(await positions(),originalPositions,'toolbar execution preserves positions');
    await button('Zoom Out').click();
    await until(camera,value=>value!==originalCamera,'zoom out moves camera explicitly');
    await button('100%').click();
    await until(camera,value=>value.endsWith('scale(1)'),'100% restores scale');
    await page.screenshot({path:join(evidenceDir,'after-real-execution.png')});
    checks.push({save_state:'real held PUT: saving then synced',toolbar_execution:'succeeded',execution_camera_and_positions_unchanged:true,zoom_out_and_100_percent:'passed'});
    await button('节点属性').click();
    await page.getByLabel('标题',{exact:true}).fill('已完成内容的复核');
    const saveResponse=page.waitForResponse(r=>r.url().endsWith(`${path}/nodes/A`)&&r.request().method()==='PUT');
    await button('保存语义').click();
    assert.equal((await saveResponse).ok(),true);
    const focusAfterSave=await page.evaluate(()=>document.activeElement?.tagName);
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.studio-panel:visible').count(),0,'Escape closes panel after save disables the focused button');
    assert.equal(await button('节点属性').evaluate(el=>el===document.activeElement),true);
    checks.push({inspector_save:'real PUT succeeded',focus_after_save:focusAfterSave,escape_after_save:'panel closed and focus returned'});
    await page.goto(`http://127.0.0.1:${frontendPort}/settings?section=appearance`,{waitUntil:'domcontentloaded'});
    await page.getByRole('radio',{name:/夜间/}).click();
    await page.goto(`http://127.0.0.1:${frontendPort}/chat`,{waitUntil:'domcontentloaded'});
    await button('与小涟语音对话').click();
    await page.locator('[data-testid="voice-pill"]').waitFor();
    await page.locator('.global-voice-expand').click();
    // Client navigation retains the genuine voice host, unlike a full reload.
    await page.getByRole('link',{name:'任务',exact:true}).click();
    await page.getByRole('button',{name:/stable-studio-c2/}).click();
    await page.locator('.task-world-node').first().waitFor();
    await page.setViewportSize({width:1280,height:720});
    assert.equal(await page.locator('.app-shell').getAttribute('data-color-scheme'),'dark');
    const voice=await page.locator('[data-testid="voice-pill"]').boundingBox();
    const toolbar=await page.locator('.studio-toolbar').boundingBox();
    assert.ok(voice&&toolbar);
    assert.ok(voice.x>=toolbar.x+toolbar.width||voice.x+voice.width<=toolbar.x||voice.y>=toolbar.y+toolbar.height||voice.y+voice.height<=toolbar.y,'voice does not cover tools');
    await page.screenshot({path:join(evidenceDir,'after-dark-voice-1280.png')});
    const colors=await page.locator('.studio-node').first().evaluate(el=>({color:getComputedStyle(el).color,background:getComputedStyle(el).backgroundColor}));
    checks.push({night_theme:'actual Settings radio',voice:'actual global voice host',toolbar_not_covered:true,colors});
    await writeFile(join(evidenceDir,'studio-checks.json'),JSON.stringify(checks,null,2));
  }
  return {status:"passed",stage,records:records.length,real_backend:true,checks};
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
