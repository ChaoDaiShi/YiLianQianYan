// ============================================================
// Task title display — pure frontend presentation logic.
//
// The backend only exposes a single `title` field on a conversation
// summary, which is auto-derived from the first user message. When that
// first message is a raw shell command (e.g. `Write-Output "..."`), the
// low-level command leaks into the task list as a first-class title.
//
// We demote command-looking titles to a neutral label and surface the
// original command as secondary detail (tooltip) instead of changing the
// database schema or migrating history.
// ============================================================

// Powershell verb-noun commandlets and common shell/binary invocations.
const COMMAND_PREFIX =
  /^(write-output|write-host|echo|printf|get-|set-|new-|invoke-|remove-|start-|stop-|copy-|move-|test-|select-|where-|foreach-|export-|import-|convertto-|convertfrom-|out-file|format-|cmd(\s|$)|powershell(\s|$)|pwsh(\s|$)|bash(\s|$)|sh(\s|$)|npm(\s|$)|cargo(\s|$)|git(\s|$)|python\d*(\s|$)|pip(\s|$)|node(\s|$)|docker(\s|$))/i;

// Shell operators / variables / command substitutions that only appear in
// raw command text, never in natural-language goals.
const COMMAND_MARKER =
  /(\|\s*\w+)|(&&\s*\w+)|(;\s*\w+)|(\$\w+)|([A-Za-z0-9_-]+\s*>\s*[^'"\s])|(`[^`]+`)/;

export function looksLikeCommand(title: string): boolean {
  const trimmed = title.trim();
  if (!trimmed) return false;
  return COMMAND_PREFIX.test(trimmed) || COMMAND_MARKER.test(trimmed);
}

/**
 * Resolve a conversation summary title for display.
 *
 * Priority (per v0.9 display spec):
 *   1. user-defined title
 *   2. user's original goal summary
 *   3. existing semantic task name
 *   4. low-level command (demoted to secondary detail)
 *
 * TODO: The backend currently collapses all of these into a single
 * `title` field. When a dedicated `goal` / `summary` field is exposed on
 * the conversation summary, prefer it here over the command heuristic.
 */
export function deriveTaskDisplayTitle(title: string | null | undefined): string {
  const raw = (title ?? "").trim();
  if (!raw) return "新任务";
  if (looksLikeCommand(raw)) return "新任务";
  return raw;
}
