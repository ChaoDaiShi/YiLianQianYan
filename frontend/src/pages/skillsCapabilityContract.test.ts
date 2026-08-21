import { describe, expect, it } from "vitest";
import skillsPageSource from "./SkillsPage.tsx?raw";

describe("Skills capability surface", () => {
  it("keeps real skill APIs while using the shared page states", () => {
    expect(skillsPageSource).toContain('className="capability-page skills-page page-canvas"');
    expect(skillsPageSource).toContain("skills-split-layout");
    expect(skillsPageSource).toContain("listSkills");
    expect(skillsPageSource).toContain("loadSkill");
    expect(skillsPageSource).toContain("PageHeader");
    expect(skillsPageSource).toContain("ErrorState");
    expect(skillsPageSource).toContain("Skeleton");
    expect(skillsPageSource).toContain('aria-label="搜索技能"');
  });

  it("does not invent unsupported skill metadata or actions", () => {
    expect(skillsPageSource).not.toContain("已启用");
    expect(skillsPageSource).not.toContain("版本");
    expect(skillsPageSource).not.toContain("安装");
    expect(skillsPageSource).not.toContain("测试技能");
  });
});
