import { beforeEach, describe, expect, it, vi } from "vitest";
import { getSkillDiscovery, refreshSkillDiscovery } from "./skillDiscovery";
const request = vi.hoisted(() => vi.fn());
vi.mock("../core/api/http", () => ({ requestResult: request }));
describe("local skill discovery client", () => {
  beforeEach(() => request.mockReset());
  it("reads existing source metadata and refreshes discovery explicitly", async () => {
    const report = { skill_count: 1, sources: [{ id:"user-codex", label:"用户 Codex", path:"local/.codex/skills", kind:"user", exists:true, skill_count:1,error:null }] };
    request.mockResolvedValue({ ok:true,data:report });
    expect(await getSkillDiscovery()).toEqual({ ok:true,data:report });
    expect(request).toHaveBeenLastCalledWith("GET","/api/skills/discovery");
    await refreshSkillDiscovery();
    expect(request).toHaveBeenLastCalledWith("POST","/api/skills/discovery");
  });
});
