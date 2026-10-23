import { useSearchParams } from "react-router-dom";
import { Tabs } from "../../components/ui";
import ManagedImports from "./ManagedImports";
import StructuredImport from "./StructuredImport";
import LocalServiceDiscovery from "./LocalServiceDiscovery";

export default function CapabilityImportWorkspace() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("import_tab");
  const tab = requested === "structured" || requested === "discovery" ? requested : "packages";
  return <div className="capability-import-workspace"><Tabs aria-label="能力导入方式" activeId={tab} onChange={id=>setParams(current=>{const next=new URLSearchParams(current);next.set("import_tab",id);return next;})} tabs={[{id:"packages",label:"Skill 与插件",panelId:false},{id:"structured",label:"工作流与 MCP",panelId:false},{id:"discovery",label:"本地发现",panelId:false}]} />
    <div className="capability-import-body">{tab === "packages" ? <ManagedImports initialDirectory={params.get("import_directory") ?? undefined} initialName={params.get("import_name") ?? undefined} initialId={params.get("package") ?? undefined} /> : tab === "structured" ? <StructuredImport /> : <LocalServiceDiscovery />}</div>
  </div>;
}
