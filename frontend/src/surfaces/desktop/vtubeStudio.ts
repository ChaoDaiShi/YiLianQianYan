const API_NAME = "VTubeStudioPublicAPI";
const API_VERSION = "1.0";
const PLUGIN = { pluginName: "YiLian QianYan", pluginDeveloper: "YiLian QianYan" };
const SUPPORTED_HOTKEYS = new Set(["TriggerAnimation", "ToggleExpression", "RemoveAllExpressions"]);

interface Envelope {
  apiName: string;
  apiVersion: string;
  requestID: string;
  messageType: string;
  data: Record<string, unknown>;
}

interface Hotkey {
  hotkeyID: string;
  name: string;
  type: string;
}

export interface StudioModel {
  modelLoaded: boolean;
  modelName: string;
  modelID: string;
  availableHotkeys: Hotkey[];
}

type Pending = { resolve: (response: Envelope) => void; reject: (error: Error) => void };

export class VTubeStudioProvider {
  private readonly socket: WebSocket;
  private readonly pending = new Map<string, Pending>();
  private readonly ready: Promise<void>;

  constructor(port: number, onModelChanged: () => void, onClosed: () => void) {
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("API 端口不正确。");
    const socket = new WebSocket(`ws://127.0.0.1:${port}`);
    this.socket = socket;
    this.ready = new Promise((resolve, reject) => {
      const timer = window.setTimeout(() => {
        reject(new Error("连接 VTube Studio 超时。"));
        this.disconnect();
      }, 4_000);
      socket.addEventListener("open", () => { window.clearTimeout(timer); resolve(); }, { once: true });
      socket.addEventListener("close", () => {
        window.clearTimeout(timer);
        const error = new Error("无法连接 VTube Studio。请确认应用已运行并启用插件 API。");
        reject(error);
        this.pending.forEach(request => request.reject(error));
        this.pending.clear();
        onClosed();
      }, { once: true });
    });
    socket.addEventListener("message", event => {
      let response: Envelope;
      try { response = JSON.parse(event.data) as Envelope; } catch { return; }
      if (response?.apiName !== API_NAME || response.apiVersion !== API_VERSION) return;
      if (response.messageType === "ModelLoadedEvent") { onModelChanged(); return; }
      this.pending.get(response.requestID)?.resolve(response);
    });
  }

  async connect() {
    await this.ready;
    const state = await this.request("APIStateRequest");
    if (state.active !== true) throw new Error("VTube Studio 插件 API 尚未启用。");
    const { authenticationToken } = await this.request("AuthenticationTokenRequest", PLUGIN, 90_000);
    if (typeof authenticationToken !== "string") throw new Error("没有收到 VTube Studio 授权。");
    const authentication = await this.request("AuthenticationRequest", { ...PLUGIN, authenticationToken });
    if (authentication.authenticated !== true) throw new Error("VTube Studio 未授权当前连接。");
    await this.request("EventSubscriptionRequest", { eventName: "ModelLoadedEvent", subscribe: true, config: {} });
    return typeof state.vTubeStudioVersion === "string" ? state.vTubeStudioVersion : null;
  }

  async readModel(): Promise<StudioModel> {
    const result = await this.request("HotkeysInCurrentModelRequest") as unknown as StudioModel;
    return { ...result, availableHotkeys: result.availableHotkeys.filter(key => SUPPORTED_HOTKEYS.has(key.type)) };
  }

  async trigger(model: StudioModel, id: string) {
    if (!model.availableHotkeys.some(key => key.hotkeyID === id && SUPPORTED_HOTKEYS.has(key.type))) {
      throw new Error("所选模型没有此动画或表情热键。");
    }
    await this.request("HotkeyTriggerRequest", { hotkeyID: id });
  }

  disconnect() {
    this.socket.close();
  }

  private request(type: string, data: Record<string, unknown> = {}, timeout = 5_000): Promise<Record<string, unknown>> {
    return new Promise((resolve, reject) => {
      if (this.socket.readyState !== WebSocket.OPEN) { reject(new Error("VTube Studio 连接已断开。")); return; }
      const requestID = crypto.randomUUID();
      const remove = () => { this.pending.delete(requestID); window.clearTimeout(timer); };
      const timer = window.setTimeout(() => { remove(); reject(new Error("VTube Studio 未在时限内回应。")); }, timeout);
      this.pending.set(requestID, {
        resolve: response => {
          remove();
          if (response.messageType === "APIError") {
            reject(new Error(String(response.data.message ?? "VTube Studio 拒绝了操作。")));
          } else if (response.messageType !== type.replace(/Request$/, "Response")) {
            reject(new Error("VTube Studio 返回了意外的响应类型。"));
          } else {
            resolve(response.data);
          }
        },
        reject: error => { remove(); reject(error); },
      });
      this.socket.send(JSON.stringify({ apiName: API_NAME, apiVersion: API_VERSION, requestID, messageType: type, data }));
    });
  }
}
