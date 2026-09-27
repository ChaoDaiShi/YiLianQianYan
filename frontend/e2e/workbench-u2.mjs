import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';

export async function workbenchU2({page,frontendPort,evidenceDir,stage}) {
  const records=[];
  for(const [width,height] of [[1280,720],[1920,1080]]) {
    await page.setViewportSize({width,height});
    for(const [name,route,selector] of [
      ['chat','/chat','.composer-card'],['settings','/settings?section=model','.settings-form'],
      ['capability','/capabilities','.capability-list-row'],['system','/system','.system-center-page'],
    ]) {
      await page.goto(`http://127.0.0.1:${frontendPort}${route}`,{waitUntil:'domcontentloaded'});
      await page.locator(selector).first().waitFor();
      await page.evaluate(()=>document.fonts.ready);
      const geometry=await page.evaluate(()=>{
        const selectors=['.page-header','.composer-card','.settings-form','.capability-list-panel','.system-monitor-grid'];
        return {overflow:document.documentElement.scrollWidth>innerWidth,elements:selectors.map(selector=>{
          const el=document.querySelector(selector);if(!el)return null;
          const r=el.getBoundingClientRect(),s=getComputedStyle(el);
          return {selector,box:{x:r.x,y:r.y,width:r.width,height:r.height},style:{fontSize:s.fontSize,color:s.color,background:s.backgroundColor,border:s.border,shadow:s.boxShadow,blur:s.backdropFilter}};
        }).filter(Boolean)};
      });
      await page.screenshot({path:join(evidenceDir,`${stage}-${name}-${width}.png`)});
      records.push({name,width,height,...geometry});
      await writeFile(join(evidenceDir,'geometry.json'),JSON.stringify(records,null,2));
      assert.equal(geometry.overflow,false,`${name} horizontal overflow`);
    }
  }
  return {status:'passed',stage,records:records.length,real_backend:true};
}
