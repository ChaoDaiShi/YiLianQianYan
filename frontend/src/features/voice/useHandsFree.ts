import { useEffect, useRef } from "react";
import { SpeechActivity } from "./speechActivity";
import { openSpeechMonitor, type SpeechMonitor } from "./speechMonitor";
import type { UseVoiceCaptureResult } from "./useVoiceCapture";

interface HandsFreeOptions {
  enabled: boolean;
  sessionId: string | null;
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
    let endpointPending = false;
    void openSpeechMonitor(controller.signal, (rms, time) => {
      if (controller.signal.aborted || !monitor) return;
      const current = latest.current;
      if (!current.enabled || current.sessionId !== options.sessionId) return;
      const ready = !starting && current.capture.state.status === "idle";
      const event = detector.sample(rms, time, current.playing, ready);
      if (event === "start") {
        starting = true;
        endpointPending = false;
        // start() interrupts playback synchronously before awaiting the new server lease.
        void current.capture.start("hands-free", monitor.stream).finally(() => {
          starting = false;
          if (!controller.signal.aborted && endpointPending) latest.current.capture.stop();
        });
      } else if (event === "end") {
        if (starting) endpointPending = true;
        else current.capture.stop();
      } else if (event === "discard") {
        endpointPending = false;
        current.capture.cancel();
      }
    }).then((opened) => { monitor = opened; }).catch(() => {
      if (!controller.signal.aborted) latest.current.onError("免手持麦克风不可用，请检查设备权限后重新开启。");
    });
    return () => {
      controller.abort();
      monitor?.close();
    };
  }, [options.enabled, options.sessionId]);
}
