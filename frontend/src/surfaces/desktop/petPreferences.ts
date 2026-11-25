export interface DesktopPetPreferences {
  locked: boolean;
  reducedMotion: boolean;
  hideOnFullscreen: boolean;
  motionGroups?: Record<string, string>;
  expressions?: Record<string, string>;
  scale: number;
}

export const PET_PREFERENCES = "yilian:desktop-pet-preferences";
export const PET_MODEL_CHANGE = "yilian:pet-model-change";
export const PET_ACTIVITY_LABELS = {
  think: "思考中",
  speak: "回复中",
  puzzled: "等待确认",
  cheer: "任务完成",
  pout: "任务未完成",
} as const;
const defaults: DesktopPetPreferences = {
  locked: false,
  reducedMotion: false,
  hideOnFullscreen: false,
  scale: 1,
};

export function readPetPreferences(): DesktopPetPreferences {
  const data = localStorage.getItem(PET_PREFERENCES);
  if (!data) return defaults;
  return { ...defaults, ...JSON.parse(data) as Partial<DesktopPetPreferences> };
}

export function writePetPreferences(preferences: DesktopPetPreferences) {
  localStorage.setItem(PET_PREFERENCES, JSON.stringify(preferences));
}
