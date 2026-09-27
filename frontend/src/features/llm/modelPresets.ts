import {PROVIDER_PRESETS} from './llmModelUtils';
import type {AppConfig} from '../../types';

export type ModelPreset = 'deepseek'|'openai'|'compatible'|'custom';
export function inferModelPreset(url: string): ModelPreset {
  try {
    const host=new URL(url).hostname;
    if(host==='api.deepseek.com') return 'deepseek';
    if(host==='api.openai.com') return 'openai';
  } catch { /* Preserve editable/custom addresses. */ }
  return 'custom';
}
export function modelPresetPatch(preset: ModelPreset): Partial<AppConfig['model']> {
  if(preset==='custom'||preset==='compatible') return {};
  const value=PROVIDER_PRESETS[preset];
  return {provider:'openai',base_url:value.baseUrl,name:value.model};
}
