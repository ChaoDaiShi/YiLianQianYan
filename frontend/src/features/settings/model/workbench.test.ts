import {describe,expect,it} from 'vitest';
import {connectionPresentation} from './workbench';

describe('Workbench connection presentation',()=>{
  it('never equates saved/readiness with a successful connection test',()=>{
    expect(connectionPresentation(false,false).label).toBe('未配置');
    expect(connectionPresentation(true,true).label).toBe('已保存 · 未测试');
    expect(connectionPresentation(true,true,'连接正常').label).toBe('连接正常');
    expect(connectionPresentation(true,true,'TIMEOUT').label).toBe('当前不可用');
    expect(connectionPresentation(true,true,'连接正常',true).label).toBe('有未保存修改');
  });
});
