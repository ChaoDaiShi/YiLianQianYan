import { useCallback,useRef,type MutableRefObject } from "react";
import { verifyProviderConnection } from "../../../api/providerConnection";
import type { GlobalVoiceSession } from "../../../api/voice";
import { desktopFloatingAvailable,openFloatingSurface } from "../floatingSurface";
const connectionErrorLabel=(error:string)=>({INVALID_CREDENTIAL:"凭据无效，请在基础配置中检查密钥",PROVIDER_UNREACHABLE:"暂时无法连接语音服务，请检查基础配置",MODEL_NOT_FOUND:"所选语音模型不存在",RATE_LIMITED:"语音服务请求过于频繁",TIMEOUT:"语音服务连接超时",INVALID_CONFIGURATION:"语音服务配置不完整"} as Record<string,string>)[error]??"语音服务连接未完成";
interface Options {
 session:GlobalVoiceSession|null;
 epoch:MutableRefObject<number>;
 startSession:()=>Promise<GlobalVoiceSession|null>;
 onStart:()=>void;
 onOutputReady:()=>void;
 onServicesReady:(stt:boolean,tts:boolean)=>void;
 onSessionReady:()=>void;
 onFailure:(message:string)=>void;
}
/** Establishes one requested call; the existing host remains the only audio owner. */
export function useCallConnection(options:Options){
 const latest=useRef(options);latest.current=options;
 return useCallback(()=>{
  const current=latest.current;current.onStart();const epoch=++current.epoch.current;
  void(async()=>{
   let output:AudioContext|null=null;
   try{
    output=new AudioContext();await output.resume();await output.close();output=null;
    if(current.epoch.current!==epoch)return;current.onOutputReady();
    if(desktopFloatingAvailable())await openFloatingSurface("call",true);
    if(current.epoch.current!==epoch)return;
    const[stt,tts]=await Promise.all([verifyProviderConnection("stt"),verifyProviderConnection("tts")]);
    if(current.epoch.current!==epoch)return;current.onServicesReady(stt.ok,tts.ok);
    if(!stt.ok||!tts.ok)throw new Error(!stt.ok?`语音识别连接失败：${connectionErrorLabel(stt.error)}`:`语音合成连接失败：${!tts.ok?connectionErrorLabel(tts.error):""}`);
    const started=current.session&&current.session.state!=="ended"?current.session:await current.startSession();
    if(current.epoch.current!==epoch)return;if(!started)throw new Error("语音会话未能建立");current.onSessionReady();
   }catch(error){if(current.epoch.current===epoch)current.onFailure(error instanceof Error?error.message:String(error));}
   finally{if(output)void output.close();}
  })();
 },[]);
}
