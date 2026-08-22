import { create } from "zustand";
import type { PendingApproval } from "../types/approval";

export interface ApprovalState {
  approvals: PendingApproval[];
  resolving: Record<string, boolean>;
  add: (approval: PendingApproval) => void;
  remove: (approvalId: string) => void;
  setApprovals: (approvals: PendingApproval[]) => void;
  markResolving: (approvalId: string, value: boolean) => void;
}

export const selectPendingApprovals = (state: ApprovalState) =>
  state.approvals.filter((approval) => approval.status === "pending");

export const useApprovalStore = create<ApprovalState>((set) => ({
  approvals: [],
  resolving: {},

  add: (approval) =>
    set((state) => {
      const existingIndex = state.approvals.findIndex(
        (item) => item.approval_id === approval.approval_id
      );
      if (existingIndex < 0) {
        return { approvals: [...state.approvals, approval] };
      }
      const approvals = [...state.approvals];
      approvals[existingIndex] = approval;
      return { approvals };
    }),

  remove: (approvalId) =>
    set((state) => {
      const resolving = { ...state.resolving };
      delete resolving[approvalId];
      return {
        approvals: state.approvals.filter(
          (approval) => approval.approval_id !== approvalId
        ),
        resolving,
      };
    }),

  setApprovals: (approvals) => set({ approvals }),

  markResolving: (approvalId, value) =>
    set((state) => ({
      resolving: { ...state.resolving, [approvalId]: value },
    })),
}));
