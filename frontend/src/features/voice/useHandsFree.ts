import { useEffect, useRef } from "react";
import { SpeechActivity } from "./speechActivity";
import { openSpeechMonitor, type SpeechMonitor } from "./speechMonitor";
import type { UseVoiceCaptureResult } from "./useVoiceCapture";
import type { CaptureOperationHandle } from "./captureOperation";

interface HandsFreeOptions {
  enabled: boolean;
  sessionId: string | null;
  /** Explicit anchor/session changes only; normal barge-in advances lease generation. */
  identityEpoch: number;
  playing: boolean;
  capture: UseVoiceCaptureResult;
  onError: (message: string) => void;
}

/** The only persistent microphone owner; recordings borrow independent cloned tracks. */
export function useHandsFree(options: HandsFreeOptions): void {
  const latest = useRef(options);
  latest.current = options;
  useEffect(() => {
    if (!options.enabled || !options.sessionId) return;
    const controller = new AbortController();
    const detector = new SpeechActivity();
    let monitor: SpeechMonitor | null = null;
    let starting = false;
    let pendingAction: "stop" | "cancel" | null = null;
    let activeCapture: CaptureOperationHandle | null = null;
    const isOwner = () => !controller.signal.aborted && latest.current.enabled
      && latest.current.sessionId === options.sessionId && latest.current.identityEpoch === options.identityEpoch;
    void openSpeechMonitor(controller.signal, (rms, time) => {
      if (!isOwner() || !monitor) return;
      const current = latest.current;
      const ready = !starting && current.capture.state.status === "idle";
      const event = detector.sample(rms, time, current.playing, ready);
      if (event === "start") {
        starting = true;
        pendingAction = null;
        activeCapture = null;
        // start() interrupts playback synchronously before awaiting the new server lease.
        void current.capture.start("hands-free", monitor.stream).then((handle) => {
          starting = false;
          if (!isOwner()) {
            handle?.cancel();
            return;
          }
          activeCapture = handle ?? null;
          if (pendingAction) activeCapture?.[pendingAction]();
        }).catch(() => {
          starting = false;
          if (isOwner()) latest.current.onError("免手持录音启动失败，请重试。");
        });
      } else if (event === "end") {
        if (starting) pendingAction = "stop";
        else activeCapture?.stop();
      } else if (event === "discard") {
        if (starting) pendingAction = "cancel";
        else activeCapture?.cancel();
      }
    }).then((opened) => { monitor = opened; }).catch(() => {
      if (isOwner()) latest.current.onError("免手持麦克风不可用，请检查设备权限后重新开启。");
    });
    return () => {
      controller.abort();
      activeCapture?.cancel();
      monitor?.close();
    };
  }, [options.enabled, options.sessionId, options.identityEpoch]);
}
