# History Conversation Scroll Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the desktop workbench chrome visible while an arbitrarily long historical conversation opens at its final message inside the center message scroller.

**Architecture:** Extend the existing workspace layout module with explicit viewport class contracts and a container-scoped scroll helper. Wire those contracts through `AppShell`, `ChatPage`, `ChatView`, and `MessageList`, replacing the page-affecting `scrollIntoView()` anchor with direct scrolling of the message list element.

**Tech Stack:** React 18, TypeScript 5, Vite 5, Tailwind CSS 3, Vitest 2, Microsoft Edge headless for live visual verification.

## Global Constraints

- Frontend-only change; do not modify Rust, Axum, SQLite schemas, or conversation APIs.
- Opening history must select the final message while keeping the left conversation sidebar, right execution sidebar, header, and input inside the viewport.
- Do not truncate, delete, migrate, or rewrite historical message content.
- Preserve the existing `narrow`, `compact`, and `full` width breakpoints and drawer behavior.
- Preserve legacy `tool_calls` normalization and execution-history hydration.
- Add no new dependency.
- Do not modify the user's live SQLite database during verification; copy it into a temporary directory first.

---

## File Map

- Modify `frontend/src/components/layout/workspaceLayout.test.ts`: add red-green regression coverage for the scroll target, viewport class contracts, and component wiring.
- Modify `frontend/src/components/layout/workspaceLayout.ts`: own the reusable viewport class strings and `scrollMessageListToBottom()` helper.
- Modify `frontend/src/components/layout/AppShell.tsx`: apply the constrained app-content viewport class.
- Modify `frontend/src/pages/ChatPage.tsx`: apply the constrained single-row workbench class.
- Modify `frontend/src/components/chat/ChatView.tsx`: own the message scroll-container ref, invoke scoped scrolling, and constrain the chat column.
- Modify `frontend/src/components/chat/MessageList.tsx`: attach the supplied ref to the sole vertical message scroller and remove the obsolete end anchor.

### Task 1: Isolate long-history scrolling inside the message list

**Files:**
- Modify: `frontend/src/components/layout/workspaceLayout.test.ts`
- Modify: `frontend/src/components/layout/workspaceLayout.ts`
- Modify: `frontend/src/components/layout/AppShell.tsx:1-34`
- Modify: `frontend/src/pages/ChatPage.tsx:1-114`
- Modify: `frontend/src/components/chat/ChatView.tsx:1-397`
- Modify: `frontend/src/components/chat/MessageList.tsx:1-119`

**Interfaces:**
- Produces: `APP_CONTENT_VIEWPORT_CLASS_NAME: string`
- Produces: `WORKBENCH_VIEWPORT_CLASS_NAME: string`
- Produces: `CHAT_COLUMN_VIEWPORT_CLASS_NAME: string`
- Produces: `MESSAGE_LIST_VIEWPORT_CLASS_NAME: string`
- Produces: `MessageScrollTarget` with `scrollHeight: number` and `scrollTo(options: ScrollToOptions): void`
- Produces: `scrollMessageListToBottom(container: MessageScrollTarget | null, behavior?: ScrollBehavior): void`
- Consumes: the existing `WorkspaceMode`, `getWorkspaceMode()`, and `getDrawerState()` APIs without changing their behavior.

- [ ] **Step 1: Write failing scroll and viewport regression tests**

Replace the imports at the top of `frontend/src/components/layout/workspaceLayout.test.ts` with the following imports. Raw-module imports let the test verify that the real components consume the shared layout contracts without requiring a new DOM test dependency.

```ts
import { describe, expect, it } from "vitest";
import appShellSource from "./AppShell.tsx?raw";
import chatPageSource from "../../pages/ChatPage.tsx?raw";
import chatViewSource from "../chat/ChatView.tsx?raw";
import messageListSource from "../chat/MessageList.tsx?raw";
import * as workspaceLayout from "./workspaceLayout";

const { getDrawerState, getWorkspaceMode } = workspaceLayout;
```

Append these tests after the existing `getDrawerState` suite:

```ts
interface CandidateScrollTarget {
  scrollHeight: number;
  scrollTo(options: ScrollToOptions): void;
}

type CandidateWorkspaceLayout = typeof workspaceLayout & {
  APP_CONTENT_VIEWPORT_CLASS_NAME?: string;
  WORKBENCH_VIEWPORT_CLASS_NAME?: string;
  CHAT_COLUMN_VIEWPORT_CLASS_NAME?: string;
  MESSAGE_LIST_VIEWPORT_CLASS_NAME?: string;
  scrollMessageListToBottom?: (
    container: CandidateScrollTarget | null,
    behavior?: ScrollBehavior
  ) => void;
};

describe("history conversation viewport contract", () => {
  const candidate = workspaceLayout as CandidateWorkspaceLayout;

  it("scrolls the supplied message container instead of a page anchor", () => {
    let received: ScrollToOptions | undefined;
    const container: CandidateScrollTarget = {
      scrollHeight: 27_237,
      scrollTo(options) {
        received = options;
      },
    };

    expect(candidate.scrollMessageListToBottom).toBeTypeOf("function");
    candidate.scrollMessageListToBottom?.(container, "smooth");

    expect(received).toEqual({ top: 27_237, behavior: "smooth" });
    expect(() =>
      candidate.scrollMessageListToBottom?.(null, "smooth")
    ).not.toThrow();
  });

  it.each([
    ["APP_CONTENT_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-hidden"]],
    [
      "WORKBENCH_VIEWPORT_CLASS_NAME",
      ["min-h-0", "overflow-hidden", "grid-rows-[minmax(0,1fr)]"],
    ],
    ["CHAT_COLUMN_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-hidden"]],
    ["MESSAGE_LIST_VIEWPORT_CLASS_NAME", ["min-h-0", "overflow-y-auto"]],
  ] as const)("exports %s with required tokens", (name, requiredTokens) => {
    const className = candidate[name];
    expect(className).toBeTypeOf("string");
    const tokens = new Set(className?.split(/\s+/));
    requiredTokens.forEach((token) => expect(tokens.has(token)).toBe(true));
  });

  it("wires the shared viewport contracts into the rendered components", () => {
    expect(appShellSource).toContain("APP_CONTENT_VIEWPORT_CLASS_NAME");
    expect(chatPageSource).toContain("WORKBENCH_VIEWPORT_CLASS_NAME");
    expect(chatViewSource).toContain("CHAT_COLUMN_VIEWPORT_CLASS_NAME");
    expect(chatViewSource).toContain("scrollMessageListToBottom");
    expect(chatViewSource).not.toContain("scrollIntoView");
    expect(messageListSource).toContain("MESSAGE_LIST_VIEWPORT_CLASS_NAME");
    expect(messageListSource).toContain("ref={scrollContainerRef}");
  });
});
```

- [ ] **Step 2: Run the targeted test and verify RED**

Run:

```powershell
cd frontend
npm.cmd test -- src/components/layout/workspaceLayout.test.ts
```

Expected: the existing responsive-layout tests pass, while the new tests fail because the four class constants and `scrollMessageListToBottom` are currently undefined and the components still contain `scrollIntoView`.

- [ ] **Step 3: Implement the shared viewport and scroll contracts**

Add these exports near the top of `frontend/src/components/layout/workspaceLayout.ts`, before `WorkspaceMode`:

```ts
export const APP_CONTENT_VIEWPORT_CLASS_NAME =
  "relative z-0 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden animate-page-in";

export const WORKBENCH_VIEWPORT_CLASS_NAME =
  "workbench-grid min-h-0 overflow-hidden grid-rows-[minmax(0,1fr)]";

export const CHAT_COLUMN_VIEWPORT_CLASS_NAME =
  "flex h-full min-h-0 min-w-0 flex-col overflow-hidden";

export const MESSAGE_LIST_VIEWPORT_CLASS_NAME =
  "scrollbar-thin min-h-0 flex-1 overflow-y-auto px-4 py-6";

export interface MessageScrollTarget {
  scrollHeight: number;
  scrollTo(options: ScrollToOptions): void;
}

export function scrollMessageListToBottom(
  container: MessageScrollTarget | null,
  behavior: ScrollBehavior = "smooth"
): void {
  if (!container) return;
  container.scrollTo({ top: container.scrollHeight, behavior });
}
```

- [ ] **Step 4: Wire the viewport contracts through the component tree**

In `frontend/src/components/layout/AppShell.tsx`, import the app-content contract and replace the literal `<main>` class:

```tsx
import { APP_CONTENT_VIEWPORT_CLASS_NAME } from "./workspaceLayout";

// ...

<main className={APP_CONTENT_VIEWPORT_CLASS_NAME}>
  <Outlet />
</main>
```

In `frontend/src/pages/ChatPage.tsx`, include `WORKBENCH_VIEWPORT_CLASS_NAME` in the existing workspace-layout import and use it on the page root:

```tsx
import {
  getDrawerState,
  getWorkspaceMode,
  WORKBENCH_VIEWPORT_CLASS_NAME,
  type WorkspaceDrawer,
} from "../components/layout/workspaceLayout";

// ...

<div className={WORKBENCH_VIEWPORT_CLASS_NAME} data-mode={workspaceMode}>
```

In `frontend/src/components/chat/ChatView.tsx`, import the chat-column contract and scoped scroll helper:

```tsx
import {
  CHAT_COLUMN_VIEWPORT_CLASS_NAME,
  scrollMessageListToBottom,
} from "../layout/workspaceLayout";
```

Replace `messagesEndRef` with the scroll-container ref:

```tsx
const messagesScrollRef = useRef<HTMLDivElement>(null);
```

Replace the `scrollIntoView()` effect with:

```tsx
useEffect(() => {
  scrollMessageListToBottom(messagesScrollRef.current);
}, [messages, runState.content, runState.order.length]);
```

Use the shared class and pass the ref to `MessageList`:

```tsx
<div className={CHAT_COLUMN_VIEWPORT_CLASS_NAME}>
  {/* existing header */}
  <MessageList
    messages={messages}
    streaming={streaming}
    scrollContainerRef={messagesScrollRef}
    onHint={setSuggestedText}
    error={error}
  />
  {/* existing input */}
</div>
```

In `frontend/src/components/chat/MessageList.tsx`, import the message-list contract:

```tsx
import { MESSAGE_LIST_VIEWPORT_CLASS_NAME } from "../layout/workspaceLayout";
```

Rename the prop and attach it to the root scrolling element:

```tsx
interface MessageListProps {
  messages: Message[];
  streaming: StreamingState | null;
  scrollContainerRef: React.RefObject<HTMLDivElement>;
  onHint?: (text: string) => void;
  error?: string | null;
}

export default function MessageList({
  messages,
  streaming,
  scrollContainerRef,
  onHint,
  error,
}: MessageListProps) {
  return (
    <div
      ref={scrollContainerRef}
      className={MESSAGE_LIST_VIEWPORT_CLASS_NAME}
    >
      {/* existing message column and content */}
    </div>
  );
}
```

Remove the obsolete `<div ref={messagesEndRef} />` anchor from the end of the message column.

- [ ] **Step 5: Run the targeted test and verify GREEN**

Run:

```powershell
cd frontend
npm.cmd test -- src/components/layout/workspaceLayout.test.ts
```

Expected: all responsive, viewport-contract, component-wiring, and scoped-scroll tests in `workspaceLayout.test.ts` pass with zero failures.

- [ ] **Step 6: Run the complete frontend test and production build gates**

Run:

```powershell
cd frontend
npm.cmd test
npm.cmd run build
```

Expected:

- Vitest reports every test file and test as passed with zero failures.
- `tsc && vite build` exits with code `0` and emits `frontend/dist/index.html` plus bundled assets.

- [ ] **Step 7: Verify the real 27,237-character history path without modifying user data**

Use a single PowerShell session so the background jobs are always cleaned up. Copy the user's database to a temporary data directory, run the backend copy on port `19420` with a fixed test token, run Vite on `1420`, and capture the known long conversation in headless Edge:

```powershell
$repo = 'F:\项目开发\忆涟千言\YiLianQianYan'
$sourceDb = 'C:\Users\25113\AppData\Roaming\yilianqianyan\yilianqianyan.db'
$testData = Join-Path $env:TEMP ('yilian-history-scroll-' + [guid]::NewGuid())
$screenshot = Join-Path $testData 'history-conversation-scroll.png'
$token = 'a' * 64
New-Item -ItemType Directory -Path $testData | Out-Null
Copy-Item -LiteralPath $sourceDb -Destination (Join-Path $testData 'yilianqianyan.db')

try {
  $backendJob = Start-Job -ScriptBlock {
    param($repoPath, $dataPath, $sessionToken)
    $env:YILIAN_DATA_DIR = $dataPath
    $env:YILIAN_HOST = '127.0.0.1:19420'
    $env:YILIAN_CONTROL_SESSION_TOKEN = $sessionToken
    Set-Location $repoPath
    cargo run -p yilian-backend --bin yilian-server
  } -ArgumentList $repo, $testData, $token

  $backendReady = $false
  for ($i = 0; $i -lt 60 -and -not $backendReady; $i++) {
    try {
      $backendReady =
        (Invoke-WebRequest -UseBasicParsing 'http://127.0.0.1:19420/api/health').StatusCode -eq 200
    } catch {
      Start-Sleep -Milliseconds 500
    }
  }
  if (-not $backendReady) { throw 'Temporary backend did not become ready' }

  $frontendJob = Start-Job -ScriptBlock {
    param($frontendPath, $sessionToken)
    $env:VITE_API_BASE = 'http://127.0.0.1:19420'
    $env:VITE_CONTROL_SESSION_TOKEN = $sessionToken
    Set-Location $frontendPath
    npm.cmd run dev -- --host 127.0.0.1
  } -ArgumentList (Join-Path $repo 'frontend'), $token

  $frontendReady = $false
  for ($i = 0; $i -lt 40 -and -not $frontendReady; $i++) {
    try {
      $frontendReady =
        (Invoke-WebRequest -UseBasicParsing 'http://127.0.0.1:1420').StatusCode -eq 200
    } catch {
      Start-Sleep -Milliseconds 250
    }
  }
  if (-not $frontendReady) { throw 'Vite did not become ready' }

  & 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' `
    --headless --disable-gpu --hide-scrollbars `
    --window-size=1500,1000 --virtual-time-budget=5000 `
    "--screenshot=$screenshot" `
    'http://127.0.0.1:1420/chat/ed1fae98-2acf-4a08-9823-d71fc9d9d6a5'

  if (-not (Test-Path -LiteralPath $screenshot)) {
    throw 'Edge did not create the verification screenshot'
  }
  Get-Item -LiteralPath $screenshot | Select-Object FullName, Length
} finally {
  Get-Job | Stop-Job -ErrorAction SilentlyContinue
  Get-Job | Remove-Job -Force -ErrorAction SilentlyContinue
}
```

Inspect the reported temporary `history-conversation-scroll.png` path with the image viewer. Required visual evidence:

- the long numeric assistant response is positioned at its final lines in the center column;
- the left task list and right execution track are visible for the `1500x1000` full layout;
- the chat header and bottom input are both visible in the same frame;
- the page is not visually displaced into blank side columns;
- the original database under `%APPDATA%\yilianqianyan` remains untouched because only its temporary copy was opened by the test backend.

- [ ] **Step 8: Review the final diff and commit the verified fix**

Run:

```powershell
cd 'F:\项目开发\忆涟千言\YiLianQianYan'
git diff --check
git diff -- frontend/src/components/layout/workspaceLayout.ts `
  frontend/src/components/layout/workspaceLayout.test.ts `
  frontend/src/components/layout/AppShell.tsx `
  frontend/src/pages/ChatPage.tsx `
  frontend/src/components/chat/ChatView.tsx `
  frontend/src/components/chat/MessageList.tsx
git status --short
```

Expected: only the six planned frontend files are modified; the temporary screenshot is outside the repository; `git diff --check` reports no whitespace errors.

Commit:

```powershell
git add frontend/src/components/layout/workspaceLayout.ts `
  frontend/src/components/layout/workspaceLayout.test.ts `
  frontend/src/components/layout/AppShell.tsx `
  frontend/src/pages/ChatPage.tsx `
  frontend/src/components/chat/ChatView.tsx `
  frontend/src/components/chat/MessageList.tsx
git commit -m "fix(frontend): isolate history conversation scrolling"
```
