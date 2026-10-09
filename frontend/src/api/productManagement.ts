import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";
import type { ProductPreferences } from "../components/system/productPreferences";

export async function managementRequest<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, { method, signal, headers: { "Content-Type": "application/json", ...controlSessionHeaders() }, body: body === undefined ? undefined : JSON.stringify(body) });
  if (!response.ok) {
    const error = await response.json().catch(() => null) as { message?: string } | null;
    throw new Error(error?.message || `服务暂不可用（${response.status}），请刷新后重试。`);
  }
  return response.json() as Promise<T>;
}
export const getProductPreferences = () => managementRequest<ProductPreferences>("GET", "/api/system/preferences");
export const saveProductPreferences = (preferences: ProductPreferences) => managementRequest<ProductPreferences>("PUT", "/api/system/preferences", preferences);
