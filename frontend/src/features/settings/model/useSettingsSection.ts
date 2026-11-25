import { useSearchParams } from "react-router-dom";
import presets from "../canvas/settingPresets.json";

export const settingLabels = new Map(presets.map(({ id, label }) => [id, label]));
export const workspaceServicesSection = "workspace-services";
settingLabels.set(workspaceServicesSection, "工作空间模型覆盖");
settingLabels.set("resource-policy","运行资源");

const sectionAliases = new Map([
  ["permission", "permissions"],
  ["security", "permissions"],
  ["skills", "extensions"],
  ["voice", "stt"],
]);

export function useSettingsSection() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("section");
  const key = requested && (sectionAliases.get(requested) ?? requested);
  const isCanvasView = params.get("view") === "canvas";
  const section = key && (settingLabels.has(key) || key.startsWith("plugin:")) &&
    (!isCanvasView || key !== workspaceServicesSection) ? key : null;

  function setSection(id: string | null) {
    const next = new URLSearchParams(params);
    if (id) next.set("section", id);
    else next.delete("section");
    setParams(next, { replace: id === null });
  }

  return { section, setSection, isCanvasView };
}
