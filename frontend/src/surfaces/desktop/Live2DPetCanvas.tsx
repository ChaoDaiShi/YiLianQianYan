import { useEffect, useRef } from "react";
import type { Application } from "pixi.js";
import type { Cubism4InternalModel, Live2DModel } from "pixi-live2d-display/cubism4";
import { loadCubismCore, type PetModelPackage } from "./petModel";
import type { DesktopPetPreferences } from "./petPreferences";
import { SPEECH_MOUTH_ACTIVE, SPEECH_MOUTH_CHANNEL } from "../../features/voice/speechMouth";

type Model = Live2DModel<Cubism4InternalModel>;
export interface PetCanvasControl {
  tap: () => void;
  point: (x: number, y: number) => void;
}

interface Props {
  modelPackage: PetModelPackage;
  preferences: DesktopPetPreferences;
  clip: string;
  sequence: number;
  onReady: () => void;
  onFailure: (message: string) => void;
  onLoadClip: () => void;
  onControl: (control: PetCanvasControl | null) => void;
}

export default function Live2DPetCanvas(props: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const runtime = useRef<{ app: Application; model: Model }>();
  const latest = useRef(props);
  latest.current = props;

  useEffect(() => {
    const canvas = canvasRef.current!;
    const urls = new Map<string, string>();
    let released = false;
    let app: Application | undefined;
    let model: Model | undefined;
    let disposeSizing: (() => void) | undefined;
    function release() {
      if (released) return;
      released = true;
      disposeSizing?.();
      latest.current.onControl(null);
      app?.stage.removeChildren();
      model?.destroy({ texture: true, baseTexture: true });
      app?.destroy(false, { children: true });
      runtime.current = undefined;
      urls.forEach(url => URL.revokeObjectURL(url));
    }
    async function initialize() {
      await loadCubismCore();
      const [{ Application }, { Cubism4ModelSettings, Live2DFactory, Live2DModel }] = await Promise.all([
        import("pixi.js"), import("pixi-live2d-display/cubism4"),
      ]);
      if (released) return;
      const settings = new Cubism4ModelSettings({
        ...props.modelPackage.manifest,
        url: `${location.origin}/local-model.model3.json`,
      });
      for (const file of props.modelPackage.files) urls.set(file.reference, URL.createObjectURL(file.asset));
      settings.resolveURL = reference => {
        const url = urls.get(reference);
        if (!url) throw new Error(`缺少模型资源 ${reference}`);
        return url;
      };
      const options = {
        autoUpdate: false, autoFocus: false, autoHitTest: false,
        autoInteract: false, checkMocConsistency: true,
      };
      model = new Live2DModel<Cubism4InternalModel>(options);
      await Live2DFactory.setupLive2DModel(model, settings, options);
      if (released) return;
      app = new Application({ view: canvas, backgroundAlpha: 0, autoStart: false, antialias: true });
      app.stage.addChild(model);
      model.anchor.set(0.5);
      runtime.current = { app, model };
      const size = () => {
        if (!app || !model || !canvas.parentElement) return;
        const width = canvas.parentElement.clientWidth;
        const height = canvas.parentElement.clientHeight;
        app.renderer.resize(width, height);
        const bounds = model.getLocalBounds();
        model.scale.set(Math.min(width / Math.max(1, bounds.width), height / Math.max(1, bounds.height)));
        model.position.set(width / 2, height / 2);
      };
      const reduced = matchMedia("(prefers-reduced-motion: reduce)");
      const activity = () => {
        if (!app) return;
        app.ticker.maxFPS = reduced.matches || latest.current.preferences.reducedMotion ? 12 : 30;
        if (document.hidden) app.stop();
        else app.start();
      };
      const observer = new ResizeObserver(size);
      observer.observe(canvas.parentElement!);
      document.addEventListener("visibilitychange", activity);
      reduced.addEventListener("change", activity);
      const lipSync = settings.getLipSyncParameters() ?? [];
      const channel = lipSync.length ? new BroadcastChannel(SPEECH_MOUTH_CHANNEL) : null;
      let mouth = 0;
      let lastSample = 0;
      if (channel) {
        localStorage.setItem(SPEECH_MOUTH_ACTIVE, "true");
        channel.onmessage = ({ data }) => {
          if (typeof data !== "number") return;
          mouth = data;
          lastSample = performance.now();
        };
        // The renderer emits this hook after physics. Its generated event
        // emitter declarations omit the inherited listener signature.
        const internal = model.internalModel as Cubism4InternalModel & {
          on(event: "beforeModelUpdate", apply: () => void): void;
        };
        internal.on("beforeModelUpdate", () => {
          if (performance.now() - lastSample > 200) return;
          for (const parameter of lipSync) model?.internalModel.coreModel.setParameterValueById(parameter, mouth, 0.8);
        });
      }
      disposeSizing = () => {
        observer.disconnect();
        document.removeEventListener("visibilitychange", activity);
        reduced.removeEventListener("change", activity);
        if (channel) {
          channel.close();
          localStorage.removeItem(SPEECH_MOUTH_ACTIVE);
        }
      };
      app.ticker.add(() => {
        if (!model || !app) return;
        model.update(Math.min(100, app.ticker.deltaMS));
      });
      let lastPoint: [number, number] = [0, 0];
      latest.current.onControl({
        point: (x, y) => {
          lastPoint = [x, y];
          model?.focus(x, y);
        },
        tap: () => model?.tap(...lastPoint),
      });
      model.on("hit", (hitAreas: string[]) => {
        const groups = model?.internalModel.settings.motions ?? {};
        const group = hitAreas.find(hit => Object.prototype.hasOwnProperty.call(groups, hit));
        if (group) void model?.motion(group).catch(() => undefined);
      });
      size();
      activity();
      canvas.dataset.modelReady = "true";
      latest.current.onReady();
      playClip(model, latest.current);
    }
    void initialize().catch(error => {
      if (!released) {
        latest.current.onFailure(error instanceof Error ? error.message : "Live2D 模型无法加载。");
        release();
      }
    });
    return release;
  }, [props.modelPackage]);

  useEffect(() => {
    if (runtime.current) {
      runtime.current.app.ticker.maxFPS =
        props.preferences.reducedMotion || matchMedia("(prefers-reduced-motion: reduce)").matches ? 12 : 30;
      playClip(runtime.current.model, latest.current);
    }
  }, [props.clip, props.sequence, props.preferences]);

  return <canvas ref={canvasRef} aria-hidden="true" className="desktop-pet-cubism" />;
}

function playClip(model: Model, props: Props) {
  const settings = model.internalModel.settings;
  const groups = Object.keys(settings.motions ?? {});
  const group = props.preferences.motionGroups?.[props.clip] ??
    groups.find(name => name.toLowerCase() === props.clip.toLowerCase());
  const expression = props.preferences.expressions?.[props.clip];
  if (group && groups.includes(group)) void model.motion(group, undefined, 3).catch(() => undefined);
  if (expression && settings.expressions?.some(spec => spec.Name === expression)) {
    void model.expression(expression).catch(() => undefined);
  } else {
    model.internalModel.motionManager.expressionManager?.resetExpression();
  }
  props.onLoadClip();
}
