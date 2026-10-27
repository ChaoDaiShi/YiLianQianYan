const SETTINGS_ICONS: Record<string, string> = {
  "settings-global": "settings-hub",
  "group:models": "models-services",
  "group:voice": "voice",
  "group:agent": "agent-context",
  "group:security": "security",
  "group:interface": "interface",
  "group:extensions": "extensions",
  model: "chat-model",
  stt: "speech-recognition",
  tts: "speech-synthesis",
  search: "web-search",
  vision: "image-understand",
  image: "image-generate",
  embedding: "embedding",
  agent: "agent-behavior",
  compaction: "context",
  permissions: "approval",
  sandbox: "sandbox",
  extensions: "extensions",
  appearance: "appearance",
  navigation: "navigation",
};

export function settingsIconName(settingId: string): string | undefined {
  return settingId.startsWith("plugin:") ? "extensions" : SETTINGS_ICONS[settingId];
}

export default function SettingsIcon({ name, size = 32 }: { name: string; size?: number }) {
  return <img className="settings-icon" src={`/settings-icons/${name}.svg`} width={size} height={size} alt="" aria-hidden="true" draggable={false} />;
}
