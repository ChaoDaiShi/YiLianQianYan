import { BG_IMAGE_DB, BG_IMAGE_STORE } from "./types";

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(BG_IMAGE_DB, 1);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(BG_IMAGE_STORE)) {
        db.createObjectStore(BG_IMAGE_STORE);
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function saveBgImage(dataUrl: string): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(BG_IMAGE_STORE, "readwrite");
    tx.objectStore(BG_IMAGE_STORE).put(dataUrl, "background");
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

export async function loadBgImage(): Promise<string | null> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(BG_IMAGE_STORE, "readonly");
    const req = tx.objectStore(BG_IMAGE_STORE).get("background");
    req.onsuccess = () => resolve((req.result as string) || null);
    req.onerror = () => reject(req.error);
  });
}

export async function clearBgImage(): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(BG_IMAGE_STORE, "readwrite");
    tx.objectStore(BG_IMAGE_STORE).delete("background");
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}
