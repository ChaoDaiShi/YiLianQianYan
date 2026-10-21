import {describe,expect,it,vi} from "vitest";
import {resolveCurrentTaskDestination} from "./currentTaskEntry";
describe("connector avatar entry",()=>{it("opens connectors without probing removed task pages",async()=>{const load=vi.fn();expect(await resolveCurrentTaskDestination(null,load)).toBe("/connectors");expect(await resolveCurrentTaskDestination("previous-graph",load)).toBe("/connectors");expect(load).not.toHaveBeenCalled();});});
