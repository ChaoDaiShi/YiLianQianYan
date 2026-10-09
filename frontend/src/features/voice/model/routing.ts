import type { GlobalVoiceContextPatch, VoiceSurfaceHostMode } from "./types";

/**
 * Which surface the voice host should treat as focused, given a route.
 *
 * The desktop skeleton always resolves to the workspace: it hosts the shell
 * before any route exists. Unknown paths fall back to the workspace rather
 * than to a conversation, so an unrecognised route cannot silently redirect
 * a spoken turn into the wrong conversation.
 */
export function resolveVoiceContextForRoute(
  pathname: string,
  mode: VoiceSurfaceHostMode,
): GlobalVoiceContextPatch {
  if (mode === "desktop-skeleton") return { focused_surface: "workspace" };
  if (pathname === "/chat" || pathname.startsWith("/chat/")) {
    return {
      focused_surface: "conversation",
    };
  }
  if (
    pathname === "/tasks" ||
    pathname === "/task-world" ||
    pathname.startsWith("/task-world/")
  ) {
    return {
      focused_surface: "task_canvas",
    };
  }
  if (pathname === "/memory" || pathname.startsWith("/memory/")) {
    return {
      focused_surface: "memory",
    };
  }
  if (pathname === "/capabilities" || pathname.startsWith("/capabilities/")) {
    return {
      focused_surface: "capability_center",
    };
  }
  if (pathname === "/system" || pathname === "/logs" || pathname === "/settings") {
    return {
      focused_surface: "system",
    };
  }
  return {
    focused_surface: "workspace",
  };
}
