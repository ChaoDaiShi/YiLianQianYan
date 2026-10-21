import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SkillSourceList } from "./SkillDiscoverySources";
describe("read-only automatic skill sources", () => {
  it("renders actual source counts, missing directories and discovery errors without implying installation", () => {
    const html=renderToStaticMarkup(<SkillSourceList sources={[
      {id:"user-codex",label:"用户 Codex",path:"F:/tools/.codex/skills",kind:"user",exists:true,skill_count:3,error:null},
      {id:"user-claude",label:"用户 Claude Code",path:"F:/tools/.claude/skills",kind:"user",exists:false,skill_count:0,error:null},
      {id:"configured-1",label:"自定义目录",path:"F:/restricted/skills",kind:"configured",exists:true,skill_count:0,error:"access denied"},
    ]}/>);
    expect(html).toContain("用户 Codex"); expect(html).toContain("3 项技能"); expect(html).toContain("目录不存在");
    expect(html).toContain("access denied"); expect(html).toContain("F:/tools/.codex/skills");
    expect(html).not.toContain("已安装");
  });
});
