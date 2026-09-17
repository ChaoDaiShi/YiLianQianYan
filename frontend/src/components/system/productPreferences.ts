export const NAV_IDS = ["tasks", "chat", "workflows", "workspaces", "skills", "plugins", "agents", "capabilities", "memory", "knowledge", "system", "logs", "settings"] as const;
export const MANDATORY_NAV = ["tasks", "chat", "settings"];
export const MONITOR_CARDS = ["health", "processes", "resources", "approvals", "diagnostics"] as const;
export interface MonitorCardLayout { id: string; x: number; y: number; width: number; height: number }
export interface ProductPreferences {
  schema_version: number; revision: number; setup_completed: boolean;
  navigation: Array<{ id: string; visible: boolean }>;
  enabled_modules: string[];
  monitor: { mode: "grid" | "free"; cards: MonitorCardLayout[] };
}
export const TRUSTED_MODULES = [
  { id: "gateway", name: "安全与权限", navigation: "settings", permissions: ["security.read"], mandatory: true },
  { id: "approvals", name: "审批", navigation: "chat", permissions: ["approvals.read"], mandatory: true },
  { id: "recovery", name: "任务恢复", navigation: "tasks", permissions: ["tasks.read"], mandatory: true },
  { id: "capabilities", name: "能力管理", navigation: "capabilities", permissions: ["capability.manage"], mandatory: false },
  { id: "monitoring", name: "监控工具箱", navigation: "system", permissions: ["system.read"], mandatory: false },
  { id: "workflows", name: "工作流", navigation: "workflows", permissions: ["workflow.read"], mandatory: false },
].map((module) => ({ ...module, version: "1.0.0", source: "builtin" as const, default_enabled: true, setup_selection: !module.mandatory }));

export function defaultPreferences(): ProductPreferences {
  return { schema_version: 1, revision: 0, setup_completed: false,
    navigation: NAV_IDS.map((id) => ({ id, visible: true })), enabled_modules: TRUSTED_MODULES.map((module) => module.id),
    monitor: { mode: "grid", cards: MONITOR_CARDS.map((id, index) => ({ id, x: index % 2 * 2, y: Math.floor(index / 2), width: 2, height: 1 })) },
  };
}

export function validPreferences(value: unknown): value is ProductPreferences {
  if (!value || typeof value !== "object") return false;
  const p = value as ProductPreferences;
  if (p.schema_version !== 1 || !Number.isSafeInteger(p.revision) || p.revision < 0 || typeof p.setup_completed !== "boolean") return false;
  if (!Array.isArray(p.navigation) || p.navigation.length !== NAV_IDS.length
    || new Set(p.navigation.map((item) => item?.id)).size !== NAV_IDS.length
    || p.navigation.some((item) => !item || !NAV_IDS.includes(item.id as typeof NAV_IDS[number]) || typeof item.visible !== "boolean" || (MANDATORY_NAV.includes(item.id) && !item.visible))) return false;
  if (!Array.isArray(p.enabled_modules) || p.enabled_modules.length > TRUSTED_MODULES.length
    || new Set(p.enabled_modules).size !== p.enabled_modules.length
    || p.enabled_modules.some((id) => !TRUSTED_MODULES.some((module) => module.id === id))
    || TRUSTED_MODULES.some((module) => module.mandatory && !p.enabled_modules.includes(module.id))) return false;
  if (!p.monitor || !["grid", "free"].includes(p.monitor.mode) || !Array.isArray(p.monitor.cards)
    || p.monitor.cards.length > MONITOR_CARDS.length || new Set(p.monitor.cards.map((card) => card?.id)).size !== p.monitor.cards.length) return false;
  return p.monitor.cards.every((card) => card && MONITOR_CARDS.includes(card.id as typeof MONITOR_CARDS[number])
    && [card.x, card.y, card.width, card.height].every(Number.isInteger)
    && card.x >= 0 && card.x <= 3 && card.y >= 0 && card.y <= 30
    && card.width >= 1 && card.width <= 4 && card.height >= 1 && card.height <= 3 && card.x + card.width <= 4);
}
export function safePreferences(value: unknown): ProductPreferences { return validPreferences(value) ? value : defaultPreferences(); }
export function acceptPreferences(current: ProductPreferences, incoming: unknown): ProductPreferences { return validPreferences(incoming) && incoming.revision >= current.revision ? incoming : current; }
