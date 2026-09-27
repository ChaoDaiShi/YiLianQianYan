import {useEffect,useState} from 'react';
import {getTaskGraph} from '../../../api/taskWorld';
export function useCapabilityDraft(graphId:string,nodeId:string|undefined,revision:number,persistedRef:string|null|undefined){
 const [input,setInput]=useState<Record<string,unknown>>({}),[argumentsJson,setArgumentsJson]=useState('{}'),[loading,setLoading]=useState(false),[error,setError]=useState('');
 useEffect(()=>{
  let active=true;setInput({});setArgumentsJson('{}');setError('');
  if(!nodeId||!persistedRef?.startsWith('capability://')){setLoading(false);return;}
  setLoading(true);
  void getTaskGraph(graphId).then(result=>{
   if(!active)return;setLoading(false);
   if(!result.ok){setError(result.error.message);return;}
   const node=result.data.nodes.find(n=>n.id===nodeId);
   if(!node||result.data.revision!==revision){setError('任务版本已变化，请刷新后编辑参数。');return;}
   setInput(node.input);setArgumentsJson(JSON.stringify(node.input.capability_input??{},null,2));
  });
  return()=>{active=false;};
 },[graphId,nodeId,revision,persistedRef]);
 return {input,argumentsJson,setArgumentsJson,loading,error};
}
