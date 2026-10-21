import {
  Bot,
  Boxes,
  BrainCircuit,
  Palette,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";

export type SettingsGroupKey =
  | "model"
  | "voice"
  | "agent"
  | "security"
  | "extensions"
  | "appearance";

export interface SettingsGroup {
  key: SettingsGroupKey;
  label: string;
  navDescription: string;
  title: string;
  description: string;
  icon: LucideIcon;
  accent: "violet" | "blue" | "rose" | "amber" | "teal" | "slate";
}

export const SETTINGS_GROUPS: SettingsGroup[] = [
  {
    key: "model",
    label: "基础配置",
    navDescription: "厂商、模型池与服务",
    title: "基础配置",
    description: "集中配置模型池、语音、搜索、图像和向量服务。",
    icon: BrainCircuit,
    accent: "violet",
  },
  {
    key: "agent",
    label: "智能体",
    navDescription: "身份、工作区与上下文",
    title: "智能体",
    description: "设置智能体身份、工作目录以及长对话的上下文管理策略。",
    icon: Bot,
    accent: "rose",
  },
  {
    key: "security",
    label: "安全控制",
    navDescription: "权限、授权与沙箱",
    title: "安全控制",
    description: "集中管理操作确认、持久化资源授权和本地执行边界。",
    icon: ShieldCheck,
    accent: "amber",
  },
  {
    key: "extensions",
    label: "能力扩展",
    navDescription: "技能与子智能体目录",
    title: "能力扩展",
    description: "指定本地技能和子智能体定义的发现位置。",
    icon: Boxes,
    accent: "teal",
  },
  {
    key: "appearance",
    label: "外观",
    navDescription: "主题、背景与可读性",
    title: "外观",
    description: "选择适合当前环境的界面明暗层级和背景。",
    icon: Palette,
    accent: "slate",
  },
];

const LEGACY_SECTION_GROUPS: Record<string, SettingsGroupKey> = {
  model: "model",
  voice: "model",
  agent: "agent",
  compaction: "agent",
  permissions: "security",
  sandbox: "security",
  skills: "extensions",
  extensions: "extensions",
  appearance: "appearance",
  security: "security",
};

export function resolveSettingsGroup(value: string | null | undefined): SettingsGroupKey {
  return value ? LEGACY_SECTION_GROUPS[value] ?? "model" : "model";
}

export function settingsGroup(key: SettingsGroupKey): SettingsGroup {
  return SETTINGS_GROUPS.find((group) => group.key === key) ?? SETTINGS_GROUPS[0];
}
