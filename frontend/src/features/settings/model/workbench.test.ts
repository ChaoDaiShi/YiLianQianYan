import {describe,expect,it} from 'vitest';
import {modelPresetPatch,inferModelPreset,connectionPresentation} from './workbench';

describe('Workbench model presentation',()=>{
  it('infers known hosts without guessing custom deployments',()=>{
    expect(inferModelPreset('https://api.deepseek.com/v1')).toBe('deepseek');
    expect(inferModelPreset('https://api.openai.com.evil.test/v1')).toBe('custom');
    expect(inferModelPreset('http://localhost:8080/v1')).toBe('custom');
  });
  it('keeps custom and compatible fields and all secrets untouched',()=>{
    expect(modelPresetPatch('custom')).toEqual({});
    expect(modelPresetPatch('compatible')).toEqual({});
    expect(Object.keys(modelPresetPatch('deepseek')).sort()).toEqual(['base_url','name','provider']);
  });
  it('never equates saved/readiness with a successful connection test',()=>{
    expect(connectionPresentation(false,false).label).toBe('未配置');
    expect(connectionPresentation(true,true).label).toBe('已保存 · 未测试');
    expect(connectionPresentation(true,true,'连接正常').label).toBe('连接正常');
    expect(connectionPresentation(true,true,'TIMEOUT').label).toBe('当前不可用');
    expect(connectionPresentation(true,true,'连接正常',true).label).toBe('有未保存修改');
  });
});
