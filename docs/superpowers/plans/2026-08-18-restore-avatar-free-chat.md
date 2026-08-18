# Restore Avatar-Free Chat Implementation Plan

> **For agentic workers:** Execute inline in the current isolated worktree. Do not dispatch subagents.

**Goal:** Restore the old avatar-free assistant conversation layout without changing chat behavior.

**Architecture:** Remove the optional avatar presentation branch from `MessageBubble` and remove avatar markup/indentation from `MessageList`. Keep message rendering, streaming state, copy behavior, tool calls, and execution logic unchanged.

**Tech Stack:** React, TypeScript, Vitest, Vite.

## Global Constraints

- Do not change backend APIs, SSE events, message types, or execution state.
- Keep user message alignment and current visual tokens.
- Do not add dependencies or dispatch subagents.

### Task 1: Add the failing render contract

**Files:**
- Modify: `frontend/src/components/chat/messageRendering.test.ts`

- [ ] **Step 1: Add a failing test**

Import raw sources for `MessageBubble.tsx` and `MessageList.tsx`, then assert both contain no `showAssistantAvatar`, no `/favicon.png`, and no `ml-11` assistant offset. Update the prop comparison test to cover only message changes.

- [ ] **Step 2: Run the focused test**

Run `npm.cmd test -- src/components/chat/messageRendering.test.ts` from `frontend`; it must fail against the current avatar-rendering implementation.

### Task 2: Restore the avatar-free layout

**Files:**
- Modify: `frontend/src/components/chat/MessageBubble.tsx`
- Modify: `frontend/src/components/chat/MessageList.tsx`
- Modify: `frontend/src/components/chat/messageRendering.test.ts`

- [ ] **Step 1: Remove assistant avatar branches and offsets**

Make `MessageBubbleProps` contain only `message`, remove the avatar wrapper and optional prop from the memo comparator, remove `ml-11` from tool results/tool calls, and remove the avatar image from streaming content/loading states while keeping their current bubble/status styling.

- [ ] **Step 2: Run the focused test**

Run `npm.cmd test -- src/components/chat/messageRendering.test.ts`; expect all tests to pass.

### Task 3: Verify and integrate

**Files:**
- No additional source files.

- [ ] **Step 1: Run frontend tests and build**

Run `npm.cmd test` and `npm.cmd run build` from `frontend`; expect zero failures and exit code 0.

- [ ] **Step 2: Run backend safety checks**

Run `cargo fmt --check` and `cargo test` from `backend`; expect zero failures. Existing unrelated warnings may remain.

- [ ] **Step 3: Commit**

Commit with `fix(chat): restore avatar-free assistant messages` and fast-forward merge the branch into `develop`.
