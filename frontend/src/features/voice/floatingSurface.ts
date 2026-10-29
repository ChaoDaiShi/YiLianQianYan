import { useEffect, useRef, useState, type PointerEvent, type MouseEvent } from "react";
import "./floatingSurface.css";
export type FloatingKind = "call" | "monitor";
interface Envelope<T> { kind: FloatingKind; state: T; theme: Record<string,string> }
interface Command {kind:FloatingKind;action:string}
export const desktopFloatingAvailable=()=>typeof window!=="undefined"&&window.__TAURI_INTERNALS__!==undefined;
async function invokeNative<T>(command:string,args:Record<string,unknown>){return(await import("@tauri-apps/api/core")).invoke<T>(command,args);}
async function emitNative(target:string,event:string,payload:unknown){return(await import("@tauri-apps/api/event")).emitTo(target,event,payload);}
async function listenNative<T>(event:string,handler:(event:{payload:T})=>void){return(await import("@tauri-apps/api/event")).listen<T>(event,handler);}
async function nativeWindow(){return(await import("@tauri-apps/api/window")).getCurrentWindow();}
export async function openFloatingSurface(kind:FloatingKind,expanded=true){if(desktopFloatingAvailable()){await invokeNative("open_floating_surface",{kind,expanded});await emitNative(`${kind}-overlay`,"floating:layout",{kind,expanded});}}
export async function resizeFloatingSurface(kind:FloatingKind,expanded:boolean){if(desktopFloatingAvailable()){await invokeNative("resize_floating_surface",{kind,expanded});await emitNative(`${kind}-overlay`,"floating:layout",{kind,expanded});}}
export async function closeFloatingSurface(kind:FloatingKind){if(desktopFloatingAvailable())await invokeNative("close_floating_surface",{kind});}
export async function hideFloatingSurface(kind:FloatingKind){if(desktopFloatingAvailable())await invokeNative("hide_floating_surface",{kind});}
export async function pinFloatingSurface(pinned:boolean){if(desktopFloatingAvailable())await(await nativeWindow()).setAlwaysOnTop(pinned);}
export async function dragFloatingSurface(){if(desktopFloatingAvailable())await(await nativeWindow()).startDragging();}
interface FloatingDragActions {
 startDrag: () => Promise<void>;
 activate?: () => void;
 onError: (cause: unknown) => void;
}
/** A button remains clickable until movement reaches six CSS pixels. Native dragging owns the gesture after that. */
export function createFloatingDragHandlers(actions: FloatingDragActions) {
 let origin: { x: number; y: number; pointerId: number } | null = null;
 let dragged = false;
 return {
  onPointerDown(event: PointerEvent<HTMLElement>) {
   if(event.button!==0)return;
   dragged=false;
   origin={x:event.clientX,y:event.clientY,pointerId:event.pointerId};
   event.currentTarget.setPointerCapture(event.pointerId);
  },
  onPointerMove(event: PointerEvent<HTMLElement>) {
   if(!origin||event.pointerId!==origin.pointerId||Math.hypot(event.clientX-origin.x,event.clientY-origin.y)<6)return;
   dragged=true;origin=null;
   event.preventDefault();
   event.currentTarget.releasePointerCapture(event.pointerId);
   void actions.startDrag().catch(actions.onError);
  },
  onPointerUp(event: PointerEvent<HTMLElement>) {
   if(origin?.pointerId===event.pointerId){origin=null;event.currentTarget.releasePointerCapture(event.pointerId);}
  },
  onPointerCancel() { origin=null; },
  onClick(event: MouseEvent<HTMLElement>) {
   if(dragged&&event.detail!==0){event.preventDefault();event.stopPropagation();return;}
   actions.activate?.();
  },
 };
}
export function useFloatingDrag(activate: (() => void) | undefined, onError: (cause: unknown) => void) {
 const actions=useRef({activate,onError});actions.current={activate,onError};
 const[handlers]=useState(()=>createFloatingDragHandlers({startDrag:dragFloatingSurface,activate:()=>actions.current.activate?.(),onError:cause=>actions.current.onError(cause)}));
 return handlers;
}
function themeSnapshot(){const result:Record<string,string>={};const computed=getComputedStyle(document.documentElement);for(const name of Array.from(computed)){if(name.startsWith("--"))result[name]=computed.getPropertyValue(name);}result["font-family"]=computed.fontFamily;result["color-scheme"]=computed.colorScheme;return result;}
/** The main window publishes facts and owns audio; overlays send control commands only. */
export function useFloatingPublisher<T>(kind:FloatingKind,state:T,onCommand:(action:string)=>void){
 const latest=useRef({state,onCommand});latest.current={state,onCommand};
 useEffect(()=>{
  if(!desktopFloatingAvailable())return;let gone=false;const releases:Array<()=>void>=[];const keep=(fn:()=>void)=>{if(gone)fn();else releases.push(fn)};
  const publish=()=>emitNative(`${kind}-overlay`,"floating:state",{kind,state:latest.current.state,theme:themeSnapshot()} satisfies Envelope<T>);
  void listenNative<Command>("floating:command",event=>{if(event.payload.kind!==kind)return;if(event.payload.action==="sync")void publish().then(()=>latest.current.onCommand("opened"));else latest.current.onCommand(event.payload.action)}).then(keep);
  void listenNative<string>("floating:closed",event=>{if(event.payload===`${kind}-overlay`)latest.current.onCommand(kind==="call"?"end":"close")}).then(keep);
  const observer=new MutationObserver(()=>{void publish()});observer.observe(document.documentElement,{attributes:true});observer.observe(document.head,{subtree:true,childList:true,characterData:true});
  return()=>{gone=true;releases.forEach(fn=>fn());observer.disconnect()};
 },[kind]);
 useEffect(()=>{if(desktopFloatingAvailable())void emitNative(`${kind}-overlay`,"floating:state",{kind,state,theme:themeSnapshot()} satisfies Envelope<T>);},[kind,state]);
}
export function useFloatingSubscriber<T>(kind:FloatingKind){
 const[expanded,setExpanded]=useState(()=>new URLSearchParams(globalThis.location?.search??"").get("expanded")!=="false");const[state,setState]=useState<T|null>(null);const[error,setError]=useState<string|null>(null);
 const send=async(action:string)=>{try{await emitNative("main","floating:command",{kind,action} satisfies Command)}catch(e){setError(String(e));}};
 useEffect(()=>{
  if(!desktopFloatingAvailable()){setError("桌面浮窗需要在桌面应用中打开");return;}let gone=false;const releases:Array<()=>void>=[];const keep=(fn:()=>void)=>{if(gone)fn();else releases.push(fn)};
  void listenNative<Envelope<T>>("floating:state",event=>{if(event.payload.kind!==kind)return;setState(event.payload.state);for(const[name,value]of Object.entries(event.payload.theme))document.documentElement.style.setProperty(name,value);}).then(fn=>{keep(fn);return send("sync")}).catch(e=>setError(String(e)));
  void listenNative<{kind:FloatingKind;expanded:boolean}>("floating:layout",event=>{if(event.payload.kind===kind)setExpanded(event.payload.expanded)}).then(keep).catch(e=>setError(String(e)));
  void nativeWindow().then(async window=>{if(gone)return;keep(await window.onMoved(async()=>{const scale=await window.scaleFactor();const position=(await window.outerPosition()).toLogical(scale);localStorage.setItem(`floating:${kind}:position`,JSON.stringify({x:position.x,y:position.y}));}));const saved=localStorage.getItem(`floating:${kind}:position`);if(saved){const position=JSON.parse(saved) as {x:number;y:number};await invokeNative("position_floating_surface",{kind,...position});}}).catch(e=>setError(String(e)));
  return()=>{gone=true;releases.forEach(fn=>fn())};
 },[kind]);
 return{state,error,send,expanded};
}
