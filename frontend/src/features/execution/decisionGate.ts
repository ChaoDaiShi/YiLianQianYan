export type ApprovalDecision = "approve" | "reject";

export function createDecisionGate(
  run: (approvalId: string, decision: ApprovalDecision) => Promise<void>
) {
  const inFlight = new Map<string, Promise<void>>();

  return {
    submit(approvalId: string, decision: ApprovalDecision): Promise<void> {
      const existing = inFlight.get(approvalId);
      if (existing) return existing;

      const request = run(approvalId, decision).finally(() => {
        inFlight.delete(approvalId);
      });
      inFlight.set(approvalId, request);
      return request;
    },
  };
}
