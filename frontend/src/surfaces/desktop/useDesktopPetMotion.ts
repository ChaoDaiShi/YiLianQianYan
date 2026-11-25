import { useCallback, useEffect, useRef, useState } from "react";
import type { ConversationActivity } from "../../features/execution/conversationRuntime";
import durations from "./desktopPetMotions.json";

type Clip = keyof typeof durations;
type Reaction = "happy" | "wink" | "nod" | "startled" | "lifted";
type ConversationLoop = "think" | "speak" | "puzzled";

const CURIOUS_AFTER_MS = 600;
const SHY_AFTER_MS = 3_200;
const WAVE_AFTER_MS = 5_800;
const STRETCH_AFTER_MS = 12_000;
const YAWN_AFTER_MS = 20_000;
const DOZE_AFTER_MS = 35_000;
const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";
const TAP_CLIPS = ["happy", "wink", "nod"] as const;
const CHAT_CLIPS = {
  idle: "idle",
  thinking: "think",
  speaking: "speak",
  waiting: "puzzled",
  succeeded: "cheer",
  failed: "pout",
} as const satisfies Record<ConversationActivity, Clip>;
const LOOPING_CLIPS = new Set<Clip>(["idle", "doze", "think", "speak", "puzzled"]);

export function useDesktopPetMotion(enabled: boolean, reducedByUser = false) {
  const [playback, setPlayback] = useState<{ clip: Clip; sequence: number }>({ clip: "greet", sequence: 0 });
  const [loadedSequence, setLoadedSequence] = useState<number>();
  const [activity, setActivity] = useState(0);
  const [conversationLoop, setConversationLoop] = useState<ConversationLoop | null>(null);
  const [reduced, setReduced] = useState(() => window.matchMedia(REDUCED_MOTION).matches);
  const hoverTimers = useRef<number[]>([]);
  const tapIndex = useRef(0);

  const play = useCallback((clip: Clip) => {
    setPlayback(previous => ({ clip, sequence: previous.sequence + 1 }));
  }, []);

  const cancelHover = useCallback(() => {
    hoverTimers.current.forEach(timer => window.clearTimeout(timer));
    hoverTimers.current = [];
  }, []);

  useEffect(() => {
    const media = window.matchMedia(REDUCED_MOTION);
    const update = () => setReduced(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (enabled) play("greet");
    else setConversationLoop(null);
    return cancelHover;
  }, [cancelHover, enabled, play]);

  useEffect(() => {
    if (!enabled || conversationLoop) return;
    const stretch = window.setTimeout(() => play("stretch"), STRETCH_AFTER_MS);
    const yawn = window.setTimeout(() => play("yawn"), YAWN_AFTER_MS);
    const doze = window.setTimeout(() => play("doze"), DOZE_AFTER_MS);
    return () => {
      window.clearTimeout(stretch);
      window.clearTimeout(yawn);
      window.clearTimeout(doze);
    };
  }, [activity, conversationLoop, enabled, play]);

  useEffect(() => {
    if (!enabled || loadedSequence !== playback.sequence || LOOPING_CLIPS.has(playback.clip)) return;
    const timer = window.setTimeout(() => play(conversationLoop ?? "idle"), durations[playback.clip]);
    return () => window.clearTimeout(timer);
  }, [conversationLoop, enabled, loadedSequence, playback, play]);

  const onChatActivity = useCallback((phase: ConversationActivity) => {
    if (!enabled) return;
    const clip = CHAT_CLIPS[phase];
    cancelHover();
    setActivity(previous => previous + 1);
    setConversationLoop(clip === "think" || clip === "speak" || clip === "puzzled" ? clip : null);
    play(clip);
  }, [cancelHover, enabled, play]);

  function recordActivity() {
    cancelHover();
    setActivity(previous => previous + 1);
  }

  function react(clip: Reaction) {
    if (!enabled) return;
    recordActivity();
    play(playback.clip === "doze" && (clip === "happy" || clip === "wink" || clip === "nod") ? "wake" : clip);
  }

  function enter() {
    if (!enabled) return;
    recordActivity();
    if (conversationLoop) return;
    if (playback.clip === "doze") {
      play("wake");
      return;
    }
    hoverTimers.current = [
      window.setTimeout(() => play("curious"), CURIOUS_AFTER_MS),
      window.setTimeout(() => play("shy"), SHY_AFTER_MS),
      window.setTimeout(() => play("wave"), WAVE_AFTER_MS),
    ];
  }

  return {
    clip: playback.clip,
    sequence: playback.sequence,
    src: `${import.meta.env.BASE_URL}desktop-pet/xilian/${reduced || reducedByUser ? "still" : playback.clip}.webp`,
    loaded: () => setLoadedSequence(playback.sequence),
    onChatActivity,
    enter,
    leave() {
      if (enabled && playback.clip !== "doze") recordActivity();
    },
    tap() {
      if (!enabled) return;
      react(TAP_CLIPS[tapIndex.current]);
      tapIndex.current = (tapIndex.current + 1) % TAP_CLIPS.length;
    },
    startled: () => react("startled"),
    lifted: () => react("lifted"),
  };
}
