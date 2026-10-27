const ICON_GRID = {
  "workspace-switch": 64,
  "task-calendar": 64,
  "run-history": 64,
  "memory-timeline": 64,
  "plugin-adapter": 64,
  "task-flow": 64,
  "tool-catalog": 64,
  home: 24,
  canvas: 24,
  capabilities: 24,
  agent: 24,
  "knowledge-base": 24,
  monitor: 24,
  account: 24,
  "message-channels": 24,
  learning: 24,
  creation: 24,
  skill: 24,
  mcp: 24,
  "import-discover": 24,
  more: 24,
  "memory-record": 24,
  conversation: 24,
  security: 24,
  approval: 24,
  "settings-hub": 24,
  "models-services": 24,
  extensions: 24,
} as const;

export type WorkspaceIconName = keyof typeof ICON_GRID;

interface WorkspaceIconProps {
  name: WorkspaceIconName;
  size?: number;
  variant?: "auto" | "color" | "mono";
  className?: string;
  label?: string;
}

/** Decorative by default; pass a label when the icon conveys information alone. */
export default function WorkspaceIcon({ name, size = 32, variant = "auto", className = "", label }: WorkspaceIconProps) {
  return (
    <span
      className={`workspace-icon workspace-icon--${variant} ${className}`.trim()}
      data-icon={name}
      style={{ width: size, height: size }}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {variant !== "mono" && <img className="workspace-icon-color" src={`/workspace-icons/color/${name}.svg`} width={size} height={size} alt="" aria-hidden="true" draggable={false} />}
      {variant !== "color" && <svg className="workspace-icon-mono" width={size} height={size} viewBox={`0 0 ${ICON_GRID[name]} ${ICON_GRID[name]}`} aria-hidden="true" focusable="false"><use href={`/workspace-icons/sprite-mono.svg#yl-mono-${name}`} /></svg>}
    </span>
  );
}
