import { useEffect, useState } from "react";
import { getProductPreferences, saveProductPreferences } from "../../api/productManagement";
import { defaultPreferences, safePreferences, validPreferences, type ProductPreferences } from "./productPreferences";

const EVENT = "yilian-product-preferences";
export function useProductPreferences() {
  const [preferences, setPreferences] = useState(defaultPreferences);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void getProductPreferences().then((value) => { if (active) { setPreferences(safePreferences(value)); setLoaded(true); } })
      .catch(() => { if (active) setError("配置服务不可用，已保留安全默认布局。"); });
    const receive = (event: Event) => setPreferences(safePreferences((event as CustomEvent).detail));
    window.addEventListener(EVENT, receive);
    return () => { active = false; window.removeEventListener(EVENT, receive); };
  }, []);
  const save = async (next: ProductPreferences) => {
    if (!validPreferences(next)) throw new Error("布局无效，安全与恢复入口必须保留。");
    try {
      const saved = await saveProductPreferences(next);
      if (!validPreferences(saved)) throw new Error("服务返回了无效布局。");
      setError(""); setLoaded(true); setPreferences(saved);
      window.dispatchEvent(new CustomEvent(EVENT, { detail: saved }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "布局未保存，请重试。");
      throw cause;
    }
  };
  return { preferences, loaded, error, save };
}
