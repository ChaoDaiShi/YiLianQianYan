import { useMemo } from "react";
import type { CaptureStatus } from "../useVoiceCapture";
import type { PlaybackStatus } from "../useSpeechPlayback";
import type { VoiceSessionState } from "../../../api/voice";
import type { GlobalVoiceProductState } from "../GlobalVoiceLeaf";
interface Inputs{capture:CaptureStatus;playback:PlaybackStatus;notice:string|null;connecting:boolean;muted:boolean;awaitingConfirmation:boolean;session:VoiceSessionState|undefined}
export function useVoiceProductState({capture,playback,notice,connecting,muted,awaitingConfirmation,session}:Inputs):GlobalVoiceProductState{
 return useMemo(()=>{
  if(capture==="error"||playback==="error"||notice)return "error";
  if(connecting)return "connecting";
  if(muted)return "muted";
  if(awaitingConfirmation)return "confirmation";
  if(playback==="synthesizing"||playback==="playing"||playback==="paused")return "speaking";
  if(capture==="acquiring"||capture==="transcribing"||session==="processing")return "understanding";
  return "listening";
 },[capture,playback,notice,connecting,muted,awaitingConfirmation,session]);
}
