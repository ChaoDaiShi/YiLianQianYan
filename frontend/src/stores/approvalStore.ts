import { create } from "zustand";
import type { PendingApproval } from "../types/approval";

interface ApprovalStore {
  pending: PendingApproval[];
  /** approval_id → "resolving" while the decision request is in flight */
  resolving: Record<string, boolean>;

  add: (approval: PendingApproval) => void;
  remove: (approvalId: string) => void;
  setPending: (approvals: PendingApproval[]) => void;
  markResolving: (approvalId: string, value: boolean) => void;
}

export const useApprovalStore = create<ApprovalStore>((set) => ({
  pending: [],
  resolving: {},

  add: (approval) =>
    set((state) => {
      if (state.pending.some((a) => a.approval_id === approval.approval_id)) {
        return state;
      }
      return { pending: [...state.pending, approval] };
    }),

  remove: (approvalId) =>
    set((state) => {
      const resolving = { ...state.resolving };
      delete resolving[approvalId];
      return {
        pending: state.pending.filter((a) => a.approval_id !== approvalId),
        resolving,
      };
    }),

  setPending: (approvals) => set({ pending: approvals }),

  markResolving: (approvalId, value) =>
    set((state) => ({ resolving: { ...state.resolving, [approvalId]: value } })),
}));
