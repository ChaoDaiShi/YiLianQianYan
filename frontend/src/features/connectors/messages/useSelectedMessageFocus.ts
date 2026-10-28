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
      const viewport=flow.getViewport();
      const left=node.position.x*viewport.zoom+viewport.x;
      const top=node.position.y*viewport.zoom+viewport.y;
      const right=left+node.measured.width*viewport.zoom;
      const bottom=top+node.measured.height*viewport.zoom;
      if(left<28||top<28||right>width-28||bottom>height-28){
        void flow.setCenter(node.position.x+node.measured.width/2,node.position.y+node.measured.height/2,{zoom:viewport.zoom,duration:180});
      }
    }
  },[selected,initialized,width,height,flow]);
}
