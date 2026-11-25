import { afterEach, describe, expect, it, vi } from "vitest";
import { openExternalUrl, openInSystemBrowser } from "./externalLink";
const mocks=vi.hoisted(()=>({open:vi.fn()}));
vi.mock("@tauri-apps/plugin-shell",()=>({open:mocks.open}));
afterEach(()=>{vi.unstubAllGlobals();vi.clearAllMocks();});
describe("default website routing",()=>{
 it("surfaces desktop launch denial without attempting another opening path",async()=>{const fallback=vi.fn();vi.stubGlobal("window",{__TAURI_INTERNALS__:{},open:fallback});mocks.open.mockRejectedValue(new Error("permission denied"));await expect(openInSystemBrowser("https://example.com/")).rejects.toThrow("permission denied");expect(mocks.open).toHaveBeenCalledOnce();expect(fallback).not.toHaveBeenCalled();});
 it("uses a web tab only for an explicit non-desktop external choice",async()=>{const open=vi.fn();vi.stubGlobal("window",{open});await openInSystemBrowser("https://example.com/");expect(open).toHaveBeenCalledWith("https://example.com/","_blank","noopener,noreferrer");expect(mocks.open).not.toHaveBeenCalled();});
 it("requests the internal browser without opening a system window",async()=>{const dispatchEvent=vi.fn(),open=vi.fn();vi.stubGlobal("window",{dispatchEvent,open});vi.stubGlobal("CustomEvent",class {constructor(public type:string,public options:unknown){}});await openExternalUrl("https://example.com/");expect(dispatchEvent).toHaveBeenCalledOnce();expect(open).not.toHaveBeenCalled();});
 it("ignores privileged schemes",async()=>{const dispatchEvent=vi.fn();vi.stubGlobal("window",{dispatchEvent});await openExternalUrl("tauri://localhost");expect(dispatchEvent).not.toHaveBeenCalled();});
});
