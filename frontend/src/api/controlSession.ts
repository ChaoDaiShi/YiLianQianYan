export const CONTROL_SESSION_HEADER = "X-Yilian-Control-Session";

type TokenProvider = () => Promise<string>;

export interface ControlSessionInitOptions {
  tauriAvailable?: boolean;
  environmentToken?: string;
  invokeToken?: TokenProvider;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export class ControlSessionClient {
  private token: string | null = null;

  async initialize(options: ControlSessionInitOptions = {}): Promise<void> {
    this.token = null;
    const tauriAvailable =
      options.tauriAvailable ??
      (typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined);

    let token: string | undefined;
    if (tauriAvailable) {
      const provider = options.invokeToken ?? defaultTauriTokenProvider;
      token = await provider();
    } else {
      token =
        options.environmentToken ??
        (import.meta.env.VITE_CONTROL_SESSION_TOKEN as string | undefined);
    }

    if (!token) {
      throw new Error("control session token is required");
    }
    if (token.trim().length < 32) {
      throw new Error("control session token must contain at least 32 characters");
    }
    this.token = token;
  }

  headers(): Record<string, string> {
    if (!this.token) {
      throw new Error("control session is not initialized");
    }
    return { [CONTROL_SESSION_HEADER]: this.token };
  }
}

async function defaultTauriTokenProvider(): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("get_control_session_token");
}

const sharedControlSession = new ControlSessionClient();

export async function initializeControlSession(): Promise<void> {
  await sharedControlSession.initialize();
}

export function controlSessionHeaders(): Record<string, string> {
  return sharedControlSession.headers();
}
