import { invoke } from "@tauri-apps/api/core";

export interface DesktopApplicationStatus {
  running: boolean;
  commandAvailable: boolean;
  version: string | null;
}

interface DesktopIntegrationStatus {
  rainmeter: DesktopApplicationStatus;
  vtubeStudio: DesktopApplicationStatus;
}

export function getDesktopIntegrationStatus() {
  return invoke<DesktopIntegrationStatus>("desktop_integration_status");
}
