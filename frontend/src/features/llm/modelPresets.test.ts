import {describe,expect,it} from 'vitest';
import {modelPresetPatch,inferModelPreset} from './modelPresets';

describe('Model presets',()=>{
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
});
