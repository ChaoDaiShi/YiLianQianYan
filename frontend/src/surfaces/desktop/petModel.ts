import type { CubismSpec } from "pixi-live2d-display/cubism4";
import { PET_MODEL_CHANGE } from "./petPreferences";

export interface PetModelPackage {
  name: string;
  manifest: CubismSpec.ModelJSON;
  files: Array<{ reference: string; asset: File }>;
}

const MODEL_DB = "yilian-desktop-pet-model-v1";
const MODEL_STORE = "models";
const MODEL_KEY = "active";
const CORE_URL = `${import.meta.env.BASE_URL}desktop-pet/cubism/live2dcubismcore.min.js`;
const OFFICIAL_CORE_URL = "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js";
// This published Core was verified with the pinned renderer. A different Core
// revision must be verified before executing it in the desktop WebView.
const CORE_SHA256 = "25ae938cb4fe282ce189b357bcc97e603d1e1f7ec78bf04150d401c23cdc792f";
let runtime: Promise<void> | undefined;

function localReference(path: unknown): string {
  if (typeof path !== "string" || !path || path.split("/").some(part => !part || part === "..") ||
      /[\\:#?]/.test(path)) {
    throw new Error("Live2D 模型资源必须使用文件夹内的本地相对路径。");
  }
  return path;
}

export async function parseModelPackage(files: File[]): Promise<PetModelPackage> {
  const descriptors = files.filter(file => file.name.toLowerCase().endsWith(".model3.json"));
  if (descriptors.length !== 1) throw new Error("所选文件夹需包含一份 .model3.json。");
  const descriptor = descriptors[0];
  const path = descriptor.webkitRelativePath;
  const folder = path.slice(0, path.lastIndexOf("/") + 1);
  const manifest = JSON.parse((await descriptor.text()).replace(/^\uFEFF/, "")) as CubismSpec.ModelJSON;
  const refs = manifest?.FileReferences;
  if (manifest?.Version !== 3 || !refs?.Moc?.toLowerCase().endsWith(".moc3") || !Array.isArray(refs.Textures) || !refs.Textures.length) {
    throw new Error("模型清单需要声明 Cubism 3+ 的 .moc3 和纹理资源。");
  }
  const references = [
    refs.Moc, ...refs.Textures,
    ...[refs.Physics, refs.Pose, refs.UserData, refs.DisplayInfo].filter(Boolean),
    ...Object.values(refs.Motions ?? {}).flatMap(motions =>
      motions.flatMap(motion => [motion.File, ...motion.Sound ? [motion.Sound] : []])),
    ...(refs.Expressions ?? []).map(expression => expression.File),
  ];
  const byPath = new Map(files.map(file => [file.webkitRelativePath || file.name, file]));
  const assets = [...new Set(references.map(localReference))].map(reference => {
    const asset = byPath.get(folder + reference.replace(/^\.\//, ""));
    if (!asset) throw new Error(`缺少模型资源 ${reference}`);
    return { reference, asset };
  });
  return { name: descriptor.name.slice(0, -".model3.json".length), manifest, files: assets };
}

function openModelStore(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(MODEL_DB, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(MODEL_STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function readModelRecord<T>(key: string): Promise<T | null> {
  const db = await openModelStore();
  try {
    return await new Promise((resolve, reject) => {
      const request = db.transaction(MODEL_STORE).objectStore(MODEL_STORE).get(key);
      request.onsuccess = () => resolve(request.result ?? null);
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}

async function saveModelRecord(key: string, record: PetModelPackage | Blob | null): Promise<void> {
  const db = await openModelStore();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction(MODEL_STORE, "readwrite");
      const store = transaction.objectStore(MODEL_STORE);
      if (record) store.put(record, key);
      else store.delete(key);
      transaction.oncomplete = () => resolve();
      transaction.onabort = () => reject(transaction.error);
    });
  } finally {
    db.close();
  }
}

export function loadPetModel(): Promise<PetModelPackage | null> {
  return readModelRecord<PetModelPackage>(MODEL_KEY);
}

export async function savePetModel(model: PetModelPackage | null): Promise<void> {
  await saveModelRecord(MODEL_KEY, model);
  localStorage.setItem(PET_MODEL_CHANGE, crypto.randomUUID());
}

async function verifiedCore(data: Blob): Promise<Blob> {
  if (data.size !== 207_155) throw new Error("Cubism Core 与此版本的受支持校验值不符。");
  const bytes = await data.arrayBuffer();
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
  if (hash !== CORE_SHA256) throw new Error("Cubism Core 与此版本的受支持校验值不符。");
  return new Blob([bytes], { type: "application/javascript" });
}

export async function installCubismCore(file?: File): Promise<void> {
  const response = file ? null : await fetch(OFFICIAL_CORE_URL, { credentials: "omit", signal: AbortSignal.timeout(15_000) });
  if (response && !response.ok) throw new Error("无法下载官方 Cubism Core。");
  const core = await verifiedCore(file ?? await response!.blob());
  await saveModelRecord("cubismCore", core);
  await loadCubismCore();
  localStorage.setItem(PET_MODEL_CHANGE, crypto.randomUUID());
}

async function coreSource(): Promise<Blob> {
  const installed = await readModelRecord<Blob>("cubismCore");
  if (installed) return verifiedCore(installed);
  const response = await fetch(CORE_URL);
  if (!response.ok || !response.headers.get("content-type")?.includes("javascript")) {
    throw new Error("请先在桌宠设置中安装 Live2D Cubism Core。");
  }
  return verifiedCore(await response.blob());
}

export function loadCubismCore(): Promise<void> {
  runtime ??= coreSource().then(core => new Promise<void>((resolve, reject) => {
    const available = () => "Live2DCubismCore" in globalThis;
    if (available()) {
      resolve();
      return;
    }
    const script = document.createElement("script");
    const url = URL.createObjectURL(core);
    script.src = url;
    const release = () => { URL.revokeObjectURL(url); script.remove(); };
    script.onload = () => {
      release();
      if (available()) resolve();
      else reject(new Error("无法载入 Cubism Core。"));
    };
    script.onerror = () => { release(); reject(new Error("无法载入 Live2D Cubism Core。")); };
    document.head.appendChild(script);
  })).catch(error => {
    runtime = undefined;
    throw error;
  });
  return runtime;
}
