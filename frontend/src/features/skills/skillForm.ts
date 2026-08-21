export interface SkillDraft {
  name: string;
  content: string;
}

export type SkillDraftResult =
  | { ok: true; value: SkillDraft }
  | { ok: false; error: string };

export function validateSkillDraft(draft: SkillDraft): SkillDraftResult {
  const name = draft.name.trim();
  if (!name) return { ok: false, error: "请输入技能名称" };
  if (name.length > 80) return { ok: false, error: "技能名称不能超过 80 个字符" };
  if (
    name === "." ||
    name === ".." ||
    /[\\/:\u0000-\u001f]/u.test(name)
  ) {
    return { ok: false, error: "技能名称不能包含路径分隔符或控制字符" };
  }
  if (!draft.content.trim()) return { ok: false, error: "SKILL.md 内容不能为空" };
  if (new TextEncoder().encode(draft.content).length > 512 * 1024) {
    return { ok: false, error: "SKILL.md 内容不能超过 512 KB" };
  }
  return { ok: true, value: { name, content: draft.content } };
}
