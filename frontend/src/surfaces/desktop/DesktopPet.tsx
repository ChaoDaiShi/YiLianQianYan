import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { useFloatingDrag } from "../../features/voice/floatingSurface";
import { useDesktopPetChatActivity } from "./desktopPetChat";
import { useDesktopPetMotion } from "./useDesktopPetMotion";
import type { PetCanvasControl } from "./Live2DPetCanvas";
import { loadPetModel, type PetModelPackage } from "./petModel";
import { PET_ACTIVITY_LABELS, PET_MODEL_CHANGE, PET_PREFERENCES, readPetPreferences } from "./petPreferences";
import "./desktopPet.css";

const Live2DPetCanvas = lazy(() => import("./Live2DPetCanvas"));

export default function DesktopPet() {
  const [error, setError] = useState("");
  const [companion, setCompanion] = useState<{display_name: string; avatar: string | null}>({display_name: "小涟", avatar: null});
  const [preferences, setPreferences] = useState(readPetPreferences);
  const [lowMemory, setLowMemory] = useState(false);
  const renderPreferences=useMemo(()=>lowMemory ? {...preferences,reducedMotion:true} : preferences,[preferences,lowMemory]);
  const [modelPackage, setModelPackage] = useState<PetModelPackage | null>(null);
  const [live2DReady, setLive2DReady] = useState(false);
  const [modelError, setModelError] = useState("");
  const [taskNotice, setTaskNotice] = useState<{ label: string } | null>(null);
  const canvas = useRef<PetCanvasControl | null>(null);
  const motion = useDesktopPetMotion(!companion.avatar || Boolean(modelPackage), renderPreferences.reducedMotion);
  const onTask = useCallback((notice: { label: string }) => setTaskNotice({ label: notice.label }), []);
  useDesktopPetChatActivity(motion.onChatActivity, onTask,setLowMemory);
  useEffect(() => {
    if (!taskNotice) return;
    const timer = window.setTimeout(() => setTaskNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [taskNotice]);
  const drag = useFloatingDrag(() => {
    motion.startled();
    void showMain();
  }, () => setError("无法拖动，请重试。"), "double-click", {
    onTap: () => { canvas.current?.tap(); motion.tap(); },
    onDragStart: motion.lifted,
    canDrag: () => !preferences.locked,
  });

  useEffect(() => {
    let active = true;
    let request = 0;
    function reloadModel() {
      const id = ++request;
      void loadPetModel().then(next => {
        if (!active || id !== request) return;
        setLive2DReady(false);
        setModelError("");
        setModelPackage(next);
      }).catch(() => { if (active) setModelError("无法读取 Live2D 模型。"); });
    }
    function refresh(event: StorageEvent) {
      if (event.key === PET_MODEL_CHANGE) reloadModel();
      if (event.key === PET_PREFERENCES) setPreferences(readPetPreferences());
    }
    reloadModel();
    window.addEventListener("storage", refresh);
    return () => { active = false; window.removeEventListener("storage", refresh); };
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("desktop_pet_resize", { scale: preferences.scale }))
      .catch(() => setError("无法调整桌宠大小。"));
  }, [preferences.scale]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !navigator.userAgent.includes("Windows")) return;
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("desktop_pet_fullscreen_policy", { enabled: preferences.hideOnFullscreen }))
      .catch(() => setError("无法更新桌宠全屏设置。"));
  }, [preferences.hideOnFullscreen]);

  useEffect(() => {
    let active = true;
    let request = 0;
    async function refreshCompanion() {
      const current = ++request;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const display = await invoke<{display_name: string; avatar: string | null}>("desktop_pet_companion_display");
        if (active && current === request) setCompanion(display);
      } catch {
        // The local profile can be unavailable while the backend initializes.
      }
    }
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webviewWindow")
      .then(({ getCurrentWebviewWindow }) => getCurrentWebviewWindow().listen("companion:profile-changed", () => void refreshCompanion()))
      .then(dispose => { if (active) unlisten = dispose; else dispose(); })
      .catch(() => undefined)
      .finally(() => { if (active) void refreshCompanion(); });
    window.addEventListener("focus", refreshCompanion);
    return () => { active = false; unlisten?.(); window.removeEventListener("focus", refreshCompanion); };
  }, []);

  async function showMain() {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("show_main_from_desktop_pet");
      setError("");
    } catch {
      setError("无法打开主窗口，请重试。");
    }
  }

  async function close() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
    } catch {
      setError("无法关闭，请重试。");
    }
  }

  async function openMenu() {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("show_desktop_pet_menu");
      setError("");
    } catch {
      setError("无法打开菜单，请重试。");
    }
  }

  const activity = taskNotice?.label ?? PET_ACTIVITY_LABELS[motion.clip as keyof typeof PET_ACTIVITY_LABELS];
  return (
    <main className="desktop-pet" aria-label={`${companion.display_name}桌宠`} onContextMenu={event => {
      event.preventDefault();
      void openMenu();
    }} onKeyDown={event => {
      if (event.key === "Escape") void close();
    }}>
      <button
        className="desktop-pet-character"
        type="button"
        aria-label={`双击${companion.display_name}打开主窗口${preferences.locked ? "，位置已锁定" : "，按住拖动"}`}
        title={`${companion.display_name} · 双击打开主窗口`}
        data-position-locked={preferences.locked}
        onPointerEnter={event => { if (event.pointerType === "mouse") motion.enter(); }}
        onPointerLeave={event => { if (event.pointerType === "mouse") motion.leave(); }}
        {...drag}
        onPointerMove={event => {
          const bounds = event.currentTarget.getBoundingClientRect();
          canvas.current?.point(event.clientX - bounds.left, event.clientY - bounds.top);
          drag.onPointerMove(event);
        }}
      >
        {!live2DReady && <img
          key={companion.avatar ? "avatar" : motion.sequence}
          src={companion.avatar ?? motion.src}
          data-pet-clip={companion.avatar ? undefined : motion.clip}
          onLoad={companion.avatar ? undefined : motion.loaded}
          alt=""
          draggable={false}
        />}
        {modelPackage && !modelError && <Suspense fallback={null}>
          <Live2DPetCanvas modelPackage={modelPackage} preferences={renderPreferences}
            clip={motion.clip} sequence={motion.sequence} onReady={() => setLive2DReady(true)}
            onLoadClip={motion.loaded} onControl={control => { canvas.current = control; }}
            onFailure={message => { setLive2DReady(false); setModelError(message); }} />
        </Suspense>}
      </button>
      <button className="desktop-pet-close" type="button" aria-label="关闭桌宠" title="关闭桌宠" onClick={() => void close()}>
        <X size={14} aria-hidden="true" />
      </button>
      {activity && !error && !modelError && <p role="status" className="desktop-pet-bubble">{activity}</p>}
      {(error || modelError) && <p role="alert" title={error || modelError} className="desktop-pet-error">{error || "Live2D 无法加载"}</p>}
    </main>
  );
}
