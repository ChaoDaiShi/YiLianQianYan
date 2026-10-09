import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {verifyWorkbenchFlows} from './workbench-flows.mjs';

export async function workbenchU2({page,frontendPort,backendPort,controlToken,evidenceDir,stage}) {
  const records=[];
  if(stage==='flows')return {status:'passed',stage,real_backend:true,flows:await verifyWorkbenchFlows({page,frontendPort,backendPort,controlToken,evidenceDir})};
  for(const [width,height] of stage==='before'||stage==='inspect' ? [[1280,720],[1920,1080]] : [[1280,720],[1366,768],[1920,1080],[2560,1440]]) {
    await page.setViewportSize({width,height});
    for(const [name,route,selector] of [
      ['chat','/chat','.composer-card'],['settings','/settings?section=model','.settings-form'],
      ['capability','/capabilities','.capability-list-row'],['system','/system','.system-center-page'],
    ]) {
      await page.goto(`http://127.0.0.1:${frontendPort}${route}`,{waitUntil:'domcontentloaded'});
      await page.locator(selector).first().waitFor();
      if(name==='system')await page.locator('.system-metric-card').first().waitFor({timeout:30000});
      await page.evaluate(()=>document.fonts.ready);
      const geometry=await page.evaluate(()=>{
        const selectors=['.page-header','.composer-card','.settings-form','.capability-list-panel','.system-monitor-grid'];
        return {overflow:document.documentElement.scrollWidth>innerWidth,visible_inputs:[...document.querySelectorAll('.settings-form input')].filter(el=>!el.closest('details:not([open])')&&el.getBoundingClientRect().height>0).map(el=>({label:el.labels?.[0]?.textContent,type:el.type})),elements:selectors.map(selector=>{
          const el=document.querySelector(selector);if(!el)return null;
          const r=el.getBoundingClientRect(),s=getComputedStyle(el);
          return {selector,box:{x:r.x,y:r.y,width:r.width,height:r.height},style:{fontSize:s.fontSize,color:s.color,background:s.backgroundColor,border:s.border,shadow:s.boxShadow,blur:s.backdropFilter}};
        }).filter(Boolean)};
      });
      await page.screenshot({path:join(evidenceDir,`${stage}-${name}-${width}.png`)});
      records.push({name,width,height,...geometry});
      await writeFile(join(evidenceDir,'geometry.json'),JSON.stringify(records,null,2));
      assert.equal(geometry.overflow,false,`${name} horizontal overflow`);
      if(stage==='after'&&name==='settings') {
        assert.equal(await page.locator('.u2-model-basic').getByLabel('API 地址',{exact:true}).isVisible(),false);
        const model=page.locator('.u2-model-basic').getByLabel('模型名称',{exact:true});
        await model.fill(`${await model.inputValue()}-layout-check`);
        const save=page.getByRole('button',{name:'保存设置',exact:true});await save.scrollIntoViewIfNeeded();
        assert.ok(await save.evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2));}),'save reachable');
      }
      if(stage==='after'&&name==='chat') {
        for(const control of [page.locator('.composer-card textarea'),page.getByRole('button',{name:'与小涟语音对话',exact:true}),page.getByTitle('选择、拖入或粘贴文件（最多 8 个，每个 25 MiB）')]) {
          await control.scrollIntoViewIfNeeded();assert.ok(await control.evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2));}),'composer control reachable');
        }
      }
    }
  }
  let flows;
  if(stage==='after') {
    flows=await verifyWorkbenchFlows({page,frontendPort,backendPort,controlToken,evidenceDir});
    await page.goto(`http://127.0.0.1:${frontendPort}/settings?section=appearance`,{waitUntil:'domcontentloaded'});
    await page.getByRole('radio',{name:/夜间/}).click();
    await page.setViewportSize({width:1280,height:720});
    for(const [name,route,selector] of [['chat','/chat','.composer-card'],['settings','/settings?section=model','.settings-form'],['capability','/capabilities','.capability-list-row']]) {
      await page.goto(`http://127.0.0.1:${frontendPort}${route}`,{waitUntil:'domcontentloaded'});
      await page.locator(selector).first().waitFor();assert.equal(await page.locator('.app-shell').getAttribute('data-color-scheme'),'dark');
      await page.screenshot({path:join(evidenceDir,`after-dark-${name}-1280.png`)});
    }
  }
  return {status:'passed',stage,records:records.length,real_backend:true,flows};
}
