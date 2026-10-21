import {describe,expect,it} from "vitest";
import source from "../../surfaces/workspace/WorkspaceSurface.tsx?raw";
import {NAV_GROUPS} from "./navGroups";
describe("workbench routing",()=>{it("replaces removed pages with connectors and more while preserving chat",()=>{expect(source).toContain('path="connectors"');expect(source).toContain('path="more"');expect(source).toContain('path="chat/:id"');expect(source).not.toContain('path="tasks"');expect(source).not.toContain('path="search"');expect(NAV_GROUPS.flatMap(g=>g.items).map(i=>i.to)).toContain("/connectors");});});
