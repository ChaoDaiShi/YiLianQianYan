import UnifiedOutputs from "../outputs/UnifiedOutputs";
import WorkspaceStudyOutputs from "./WorkspaceStudyOutputs";

/** The shared list retains each source's original storage and access checks. */
export default function WorkspaceArtifacts({ workspaceId, limit = 50, embeddedPreview = false, includeStudy = false }: { workspaceId?: string; limit?: number; embeddedPreview?: boolean; includeStudy?: boolean }) {
  return <>
    {workspaceId && includeStudy && <WorkspaceStudyOutputs workspaceId={workspaceId} />}
    <UnifiedOutputs workspaceId={workspaceId} limit={limit} embeddedPreview={embeddedPreview} title={includeStudy ? "任务与对话成果" : undefined} />
  </>;
}
