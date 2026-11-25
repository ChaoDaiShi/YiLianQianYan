import { useCallback, useRef, type MutableRefObject } from "react";
import { getProviderReadiness } from "../../../api/providerConnection";
import type { GlobalVoiceSession } from "../../../api/voice";
import { desktopFloatingAvailable, openFloatingSurface } from "../floatingSurface";

interface Options {
  session: GlobalVoiceSession | null;
  epoch: MutableRefObject<number>;
  startSession: () => Promise<GlobalVoiceSession | null>;
  onStart: () => void;
  onOutputReady: () => void;
  onServicesReady: (stt: boolean, tts: boolean) => void;
  onSessionReady: () => void;
  onFailure: (message: string) => void;
}

/** Establishes one requested call; the existing host remains the only audio owner. */
export function useCallConnection(options: Options) {
  const latest = useRef(options);
  latest.current = options;
  return useCallback(() => {
    const current = latest.current;
    current.onStart();
    const epoch = ++current.epoch.current;
    void (async () => {
      let output: AudioContext | null = null;
      try {
        output = new AudioContext();
        await output.resume();
        await output.close();
        output = null;
        if (current.epoch.current !== epoch) return;
        current.onOutputReady();
        if (desktopFloatingAvailable()) await openFloatingSurface("call", true);
        if (current.epoch.current !== epoch) return;

        const readiness = await getProviderReadiness();
        if (current.epoch.current !== epoch) return;
        if (!readiness) throw new Error("暂时无法读取语音配置，请检查本地服务后重试。");
        const stt = readiness.stt.available;
        const tts = readiness.tts.available;
        current.onServicesReady(stt, tts);
        if (!stt) throw new Error("语音识别尚未就绪，请在基础配置中检查识别服务。");
        if (!tts) throw new Error("语音合成尚未就绪，请在基础配置中检查合成服务。");

        const started = current.session && current.session.state !== "ended"
          ? current.session
          : await current.startSession();
        if (current.epoch.current !== epoch) return;
        if (!started) throw new Error("语音会话未能建立");
        current.onSessionReady();
      } catch (error) {
        if (current.epoch.current === epoch) {
          current.onFailure(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (output) void output.close();
      }
    })();
  }, []);
}
