import { describe, expect, it } from "vitest";
import { normalizeBrowserAddress, browserOrigin, siteIdentityLabel } from "./browserPolicy";
describe("browser address and project identity",()=>{
 it.each(["file:///C:/Windows","javascript:alert(1)","https://user:pass@example.com","https://example.com/\nnext"])("rejects unsafe address %s",url=>expect(()=>normalizeBrowserAddress(url)).toThrow());
 it("retains the explicit development port but never infers a project from it",()=>{expect(browserOrigin("http://localhost:3000/path")).toBe("http://localhost:3000");expect(siteIdentityLabel("Project A")).toContain("未核实");expect(siteIdentityLabel("Project B")).toContain("未核实");});
});
