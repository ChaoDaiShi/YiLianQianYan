import {useEffect,useState} from 'react';
import {inferModelPreset,modelPresetPatch,type ModelPreset} from '../model/workbench';

export default function ModelPresetSelect({url,onField,onAdvanced}:{url:string;onField:(key:string,value:unknown)=>void;onAdvanced:()=>void}) {
  const [preset,setPreset]=useState<ModelPreset>(()=>inferModelPreset(url));
  useEffect(()=>setPreset(inferModelPreset(url)),[url]);
  return <label className="u2-provider-field"><span>模型服务</span>
    <select aria-label="模型服务" value={preset} onChange={event=>{
      const selected=event.target.value as ModelPreset;
      setPreset(selected);
      for(const [key,value] of Object.entries(modelPresetPatch(selected)))onField(key,value);
      if(selected==='compatible'||selected==='custom')onAdvanced();
    }}>
      <option value="deepseek">DeepSeek</option><option value="openai">OpenAI</option>
      <option value="compatible">OpenAI Compatible</option><option value="custom">Custom · 自定义</option>
    </select>
  </label>;
}
