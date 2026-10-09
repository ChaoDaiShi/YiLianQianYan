import { useCallback, useState } from "react";
import { getProviderReadiness } from "../../../api/providerConnection";
import {
  reviewTaskGraph,
  updateTaskNode,
  type ApiError,
  type TaskGraphReview,
  type TaskGraphReviewSuggestion,
} from "../../../api/taskWorld";
import { projectTaskGraph } from "../taskGraphProjection";

type Projection = NonNullable<ReturnType<typeof projectTaskGraph>>;
type Mutate = (
  operation: () => Promise<{ ok: true; data: unknown } | { ok: false; error: ApiError }>,
) => Promise<boolean>;

/**
 * AI graph review, expressed as user-accepted suggestions.
 *
 * Suggestions are never applied automatically: they are held against the
 * revision they were produced from, and applying one whose revision has moved
 * on is refused rather than silently merged.
 */
export function useTaskGraphReview(
  graphId: string,
  projection: Projection | null,
  reload: () => Promise<void>,
  mutate: Mutate,
  onModelUnavailableChange: (unavailable: boolean) => void,
) {
  const [review, setReview] = useState<TaskGraphReview | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState("");

  const runGraphReview = useCallback(async () => {
    if (!projection || reviewBusy) return;
    const readiness = await getProviderReadiness();
    if (!readiness?.model.available) {
      onModelUnavailableChange(true);
      setReviewError("还没有配置可用的模型服务。");
      return;
    }
    onModelUnavailableChange(false);
    setReviewBusy(true);
    setReviewError("");
    const result = await reviewTaskGraph(graphId, projection.revision);
    setReviewBusy(false);
    if (result.ok) setReview(result.data);
    else {
      setReview(null);
      setReviewError(result.error.message);
      if (result.error.code === "stale_revision") await reload();
    }
  }, [graphId, onModelUnavailableChange, projection, reload, reviewBusy]);

  const acceptReviewSuggestion = useCallback(async (suggestion: TaskGraphReviewSuggestion) => {
    if (!projection || !review) return;
    if (projection.revision !== review.reviewed_revision) {
      setReviewError("任务图已更新，旧审查建议未应用；请重新审查。");
      setReview(null);
      return;
    }
    const node = projection.nodes.find((candidate) => candidate.id === suggestion.node_id);
    if (!node) {
      setReviewError("建议引用的节点已不存在，请重新审查。");
      setReview(null);
      return;
    }
    const input: Record<string, unknown> = {
      instruction: suggestion.instruction,
      acceptance_criteria: suggestion.acceptance_criteria,
    };
    if (node.executor_ref) input.executor_ref = node.executor_ref;
    const accepted = await mutate(() => updateTaskNode(graphId, node.id, review.reviewed_revision, {
      kind: node.kind,
      title: suggestion.title,
      input,
      retry_policy: node.retry_policy || { max_attempts: 1 },
    }));
    if (accepted) {
      setReview(null);
      setReviewError("图已按该建议更新；如需继续，请基于新版本重新执行 AI 审查。");
    }
  }, [graphId, mutate, projection, review]);

  return { review, setReview, reviewBusy, reviewError, runGraphReview, acceptReviewSuggestion };
}
