import { useEffect } from "react";
import { useReactFlow, useStore } from "@xyflow/react";

/** Keep the selected message visible when its detail or composer changes the canvas size. */
export function useSelectedMessageFocus(selected:string|null,initialized:boolean){
  const flow=useReactFlow();
  const width=useStore(state=>state.width);
  const height=useStore(state=>state.height);
  useEffect(()=>{
    if(!selected||!initialized)return;
    const node=flow.getNode(selected);
    if(node?.measured?.width&&node.measured.height){
      void flow.setCenter(node.position.x+node.measured.width/2,node.position.y+node.measured.height/2,{zoom:flow.getZoom(),duration:180});
    }
  },[selected,initialized,width,height,flow]);
}
