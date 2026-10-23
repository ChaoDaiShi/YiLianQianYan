import { describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { renderToStaticMarkup } from "react-dom/server";
import ManagedImports, { filterManagedImports, type ManagedImport } from "./ManagedImports";
import discovery from "./LocalServiceDiscovery.tsx?raw";
import structured from "./StructuredImport.tsx?raw";

const record = (patch: Partial<ManagedImport>): ManagedImport => ({id:"one",name:"示例",kind:"skill",version:"1",source:"local",permissions:[],content_hash:"hash",revision:1,enabled:false,installed:true,history_count:0,runtime_ready:false,runtime_status:"disabled",...patch});
const all = {query:"",status:"all",source:"all"};
describe("capability import workbench", () => {
  it("filters by category before deciding whether a plugin list is empty", () => {
    expect(filterManagedImports([record({})],{...all,kind:"plugin"})).toEqual([]);
    const plugin=record({id:"plugin",kind:"plugin"});
    expect(filterManagedImports([record({}),plugin],{...all,kind:"plugin"})).toEqual([plugin]);
  });
  it("combines search, real source and installed/runtime state", () => {
    const ready=record({id:"ready",name:"文档助手",kind:"plugin",enabled:true,runtime_ready:true,source:"github"});
    const removed=record({id:"removed",name:"文档助手旧版",kind:"plugin",installed:false,enabled:true,runtime_ready:true,source:"github"});
    expect(filterManagedImports([ready,removed],{kind:"plugin",query:"文档",source:"github",status:"ready"})).toEqual([ready]);
    expect(filterManagedImports([ready,removed],{...all,status:"removed"})).toEqual([removed]);
  });
  it("shows plugin management rather than a Skill import form on entry", () => {
    const html=renderToStaticMarkup(<MemoryRouter><ManagedImports kind="plugin"/></MemoryRouter>);
    expect(html).toContain('aria-label="插件列表"');
    expect(html).toContain('aria-label="插件详情"');
    expect(html).toContain("导入插件");
    expect(html).not.toContain("Markdown Skill");
    expect(html).not.toContain("Skill 目录绝对路径");
    expect(html).not.toContain('role="dialog"');
  });
  it("links actual discovered and imported records to their next management step", () => {
    expect(discovery).toContain("import_directory:candidate.path!");
    expect(discovery).toContain("检查并导入此 Skill");
    expect(discovery).toContain("encodeURIComponent(saved.data.id)");
    expect(structured).toContain("workflow=${encodeURIComponent(saved.data.id)}");
    expect(structured).toContain("server=${encodeURIComponent(saved.data.id)}");
    expect(structured).toContain('workflow_tab=templates');
  });
});
