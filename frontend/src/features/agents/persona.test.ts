import { describe, expect, it, vi } from "vitest";
import { emptyProfile, getAgentProfile, saveAgentProfile } from "./persona";
import { managementRequest } from "../../api/productManagement";
vi.mock("../../api/productManagement", () => ({ managementRequest: vi.fn() }));
describe("user persona API", () => {
  it("saves only user-owned fields through the selected agent profile", async () => {
    const profile = emptyProfile(); profile.persona_enabled = true; profile.persona.display_name = "昔涟"; profile.user_prompt = "认真作答";
    vi.mocked(managementRequest).mockResolvedValue(profile);
    await saveAgentProfile("agent / special", profile);
    expect(managementRequest).toHaveBeenCalledWith("PUT", "/api/agents/agent%20%2F%20special/profile", profile);
    expect(profile).not.toHaveProperty("instructions");
    await getAgentProfile("default"); expect(managementRequest).toHaveBeenLastCalledWith("GET", "/api/agents/default/profile");
  });
});
