import {useCallback,useEffect,useRef,useState} from 'react';
import {listCapabilities,refreshCapabilities,type CapabilityDescriptor} from '../../../api/capabilities';
export function useCanvasCapabilities(){
  const [items,setItems]=useState<CapabilityDescriptor[]>([]),[error,setError]=useState(''),[loading,setLoading]=useState(false);
  const epoch=useRef(0);
  const refresh=useCallback(async(rebuild=false)=>{
    const ticket=++epoch.current;setLoading(true);setError('');
    if(rebuild){const result=await refreshCapabilities();if(!result.ok){if(ticket===epoch.current){setError(result.error);setLoading(false);}return;}}
    const all:CapabilityDescriptor[]=[];
    for(let offset=0;offset<10000;offset+=200){
      const result=await listCapabilities({provider:'mcp',limit:200,offset});
      if(ticket!==epoch.current)return;
      if(!result.ok){setError(result.error);setLoading(false);return;}
      all.push(...result.data.capabilities);
      if(all.length>=result.data.total||result.data.capabilities.length===0)break;
    }
    if(ticket===epoch.current){setItems(all);setLoading(false);}
  },[]);
  useEffect(()=>{void refresh();return()=>{epoch.current++;};},[refresh]);
  return {items,error,loading,refresh};
}
