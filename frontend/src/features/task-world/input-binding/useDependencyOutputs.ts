import {useEffect,useState} from 'react';
import {getTaskNodeSuccessfulOutput} from '../../../api/taskWorld';
import type {TaskNodeProjection} from '../taskGraphProjection';

export interface DependencyOutput {loading:boolean;output?:unknown;error?:string}
export function useDependencyOutputs(graphId:string,nodes:TaskNodeProjection[]) {
  const [outputs,setOutputs]=useState<Record<string,DependencyOutput>>({});
  const key=nodes.map(n=>`${n.id}:${n.latest_execution?.execution_id}:${n.latest_execution?.status}:${n.latest_execution?.updated_at}`).join('|');
  useEffect(()=>{
    let active=true;
    setOutputs(Object.fromEntries(nodes.map(n=>[n.id,{loading:true}])));
    for(const node of nodes)void getTaskNodeSuccessfulOutput(graphId,node.id).then(result=>{
      if(active)setOutputs(current=>({...current,[node.id]:result.ok?{loading:false,output:result.data}:{loading:false,error:result.error.message}}));
    });
    return()=>{active=false;};
  },[graphId,key]);
  return outputs;
}
