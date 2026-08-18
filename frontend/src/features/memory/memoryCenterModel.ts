import type { MemoryRecord, MemoryStats } from "../../api/client";

export const MEMORY_CATEGORY_OPTIONS = [
  { value: "", label: "全部" },
  { value: "fact", label: "事实" },
  { value: "preference", label: "偏好" },
  { value: "knowledge", label: "知识" },
  { value: "note", label: "笔记" },
] as const;

type MemoryTone = "info" | "accent" | "purple" | "warning" | "default";

const CATEGORY_PRESENTATIONS: Record<string, { label: string; tone: MemoryTone }> = {
  fact: { label: "事实", tone: "info" },
  preference: { label: "偏好", tone: "accent" },
  knowledge: { label: "知识", tone: "purple" },
  note: { label: "笔记", tone: "warning" },
};

const SOURCE_LABELS: Record<string, string> = {
  auto: "AI 提取",
  manual: "手动",
  document: "文档",
};

export function getMemoryCategoryPresentation(category: string) {
  return CATEGORY_PRESENTATIONS[category] ?? { label: category, tone: "default" as const };
}

export function getMemorySourceLabel(source: string): string {
  return SOURCE_LABELS[source] ?? source;
}

export function memoryCategoryCounts(stats: MemoryStats | null): Record<string, number> {
  const counts: Record<string, number> = { fact: 0, preference: 0, knowledge: 0, note: 0 };
  for (const [category, count] of stats?.by_category ?? []) {
    if (category in counts) counts[category] = count;
  }
  return counts;
}

export function truncateMemoryContent(content: string, maxLength = 140): string {
  return content.length > maxLength ? `${content.slice(0, maxLength)}…` : content;
}

export interface MemoryDetailField {
  key: "category" | "content" | "created_at" | "updated_at" | "source";
  label: string;
  value: string | number;
}

export function getMemoryDetailFields(memory: MemoryRecord): MemoryDetailField[] {
  return [
    { key: "category", label: "分类", value: memory.category },
    { key: "content", label: "内容", value: memory.content },
    { key: "created_at", label: "创建时间", value: memory.created_at },
    { key: "updated_at", label: "更新时间", value: memory.updated_at },
    { key: "source", label: "来源", value: memory.source },
  ];
}
