import type { AppConfig } from "../../../types";

/** The six visible setting groups. Legacy query keys map through settingsNavigation. */
export type { SettingsGroupKey as SectionKey } from "./settingsNavigation";

/**
 * Field-level edit of one top-level config group.
 *
 * `value` is intentionally loose: each section writes its own field shape and
 * the config is re-validated by the backend on save.
 */
export type UpdateField = (section: keyof AppConfig, key: string, value: any) => void;

export type UpdateVoiceField = (
  side: "stt" | "tts",
  key: string,
  value: string | number,
) => void;
