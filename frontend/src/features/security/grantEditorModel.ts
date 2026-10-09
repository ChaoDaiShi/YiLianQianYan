export type GrantEditorKind = "filesystem" | "network" | "process" | "shell" | "mcp";

export interface GrantEditorState {
  kind: GrantEditorKind;
  permission: string;
  effect: "allow" | "deny";
  root: string;
  recursive: boolean;
  scheme: "http" | "https";
  host: string;
  port: string;
  methods: string;
  zone: "public" | "loopback" | "private";
  processScope: "managed_children";
  pid: string;
  hostEscapeAcknowledged: boolean;
  serverId: string;
  toolName: string;
}

export const initialGrantEditorState: GrantEditorState = {
  kind: "filesystem",
  permission: "filesystem.read",
  effect: "allow",
  root: "",
  recursive: true,
  scheme: "https",
  host: "",
  port: "",
  methods: "GET",
  zone: "public",
  processScope: "managed_children",
  pid: "",
  hostEscapeAcknowledged: false,
  serverId: "",
  toolName: "",
};

export function grantPayload(state: GrantEditorState): {
  permission_id: string;
  effect: string;
  resource: Record<string, unknown>;
} {
  switch (state.kind) {
    case "mcp":
      return {
        permission_id: "mcp.invoke",
        effect: state.effect,
        resource: { type: "mcp", server_id: state.serverId, tool_name: state.toolName === "" ? null : state.toolName },
      };
    case "filesystem":
      return {
        permission_id: state.permission,
        effect: state.effect,
        resource: { type: "filesystem", root: state.root.trim(), recursive: state.recursive },
      };
    case "network":
      return {
        permission_id: "network.request",
        effect: state.effect,
        resource: {
          type: "network",
          scheme: state.scheme,
          host: state.host.trim().toLowerCase(),
          port: state.port ? Number(state.port) : null,
          methods: state.methods.split(",").map((method) => method.trim().toUpperCase()).filter(Boolean),
          zone: state.zone,
        },
      };
    case "process":
      return {
        permission_id: "process.control",
        effect: state.effect,
        resource: {
          type: "process",
          scope:
          { kind: state.processScope },
        },
      };
    case "shell":
      return {
        permission_id: "shell.execute",
        effect: state.effect,
        resource: { type: "shell", host_escape_acknowledged: state.hostEscapeAcknowledged },
      };
  }
}

export function grantWarning(state: GrantEditorState): string | null {
  if (state.kind === "mcp") {
    return "按 Server ID 与工具名精确匹配，区分大小写；工具名留空表示整个服务。拒绝优先于允许；允许授权仍需按高风险策略逐次审批。";
  }
  if (state.kind === "network" && state.zone === "loopback") {
    return "回环地址只允许连接本机服务；请确认这是预期目标。";
  }
  if (state.kind === "network" && state.zone === "private") {
    return "私有网络授权可能触达内网服务或云元数据地址；请确认范围。";
  }
  if (state.kind === "shell" && state.hostEscapeAcknowledged) {
    return "Shell 授权允许命令逃逸工作区边界；仅在明确需要时启用。";
  }
  return null;
}

export function mcpGrantError(serverId: string, toolName: string): string | null {
  const valid = (value: string) => value.trim().length > 0
    && new TextEncoder().encode(value).length <= 256
    && !/[\u0000-\u001f\u007f-\u009f*]/.test(value);
  if (!valid(serverId)) return "Server ID 必须为 1–256 字节，不能包含控制字符或 *";
  if (toolName !== "" && !valid(toolName)) return "工具名必须为 1–256 字节，不能包含控制字符或 *；留空表示整个服务";
  return null;
}

export function isolationRows(status: {
  privilege_reduction: boolean;
  restricting_sids: boolean;
  job_object: boolean;
  process_containment: boolean;
  filesystem_os_enforced: boolean;
  network_os_enforced: boolean;
}) {
  return [
    ["Privilege-reduced token", status.privilege_reduction],
    ["Restricting-SID ACL sandbox", status.restricting_sids],
    ["Windows Job Object", status.job_object],
    ["Process tree containment", status.process_containment],
    ["OS filesystem isolation", status.filesystem_os_enforced],
    ["OS network isolation", status.network_os_enforced],
  ] as const;
}
