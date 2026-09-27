import {renderToStaticMarkup} from 'react-dom/server';
import {describe,expect,it} from 'vitest';
import Input from './Input';
describe('Input label ownership',()=>{
  it('gives repeated labels distinct matching ids across settings sections',()=>{
    const html=renderToStaticMarkup(<><Input label="模型名称"/><Input label="模型名称"/></>);
    const ids=[...html.matchAll(/<input id="([^"]+)"/g)].map(match=>match[1]);
    const labels=[...html.matchAll(/for="([^"]+)"/g)].map(match=>match[1]);
    expect(ids).toHaveLength(2);expect(new Set(ids).size).toBe(2);expect(labels).toEqual(ids);
  });
  it('keeps explicitly supplied ids',()=>{
    expect(renderToStaticMarkup(<Input id="model-name" label="模型"/>)).toContain('for="model-name"');
  });
});
