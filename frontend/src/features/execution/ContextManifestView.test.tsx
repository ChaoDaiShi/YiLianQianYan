import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ContextManifestView } from "./ContextManifestView";
import CapabilityAvailabilityView from "../capabilities/CapabilityAvailabilityView";
import type { CapabilityDescriptor } from "../../api/capabilities";
describe("integration context presentation", () => {
  it("shows unsupported and excluded sources without fabricating resolved input", () => { const html = renderToStaticMarkup(<ContextManifestView manifest={{ schema_version: 1, total_chars: 0, entries: [{ kind: "memory", source_id: "memory-1", status: "excluded", chars: 0, reason: "delegation restriction" }, { kind: "constraints", source_id: "node-1", status: "unsupported", chars: 0 }] }} />); expect(html).toContain("已排除"); expect(html).toContain("当前执行器未支持"); expect(html).toContain("delegation restriction"); });
  it("does not infer authorization from a ready descriptor", () => { const capability = { availability: { personal: { discoverable: true, bindable: false, executable: false, requires_approval: false, authorization: "runtime_check_required", reasons: ["personal scope only"] }, chat: { discoverable: true, bindable: true, executable: true, requires_approval: true, authorization: "runtime_check_required", reasons: [] }, task_node: { discoverable: true, bindable: false, executable: false, requires_approval: true, authorization: "runtime_check_required", reasons: ["not supported"] } } } as Pick<CapabilityDescriptor, "availability">; const html = renderToStaticMarkup(<CapabilityAvailabilityView capability={capability} />); expect(html).toContain("当前不支持执行"); expect(html).toContain("personal scope only"); expect(html).toContain("这里不会新增权限"); });
});
