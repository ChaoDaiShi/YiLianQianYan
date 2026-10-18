import { useEffect, useRef, useState } from "react";
import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { PendingApproval } from "../../types/approval";
import {
  attestVoiceApprovalDisplayed,
  revokeVoiceApprovalDisplay,
  type VoiceApprovalAttestation,
} from "../../api/voice";
import { useGlobalVoiceContext } from "../../features/voice/GlobalVoiceHost";
import { formatToolDisplayName } from "../chat/toolDisplay";
import { Button } from "../ui";

interface ApprovalCardProps {
  approval: PendingApproval;
  resolving?: boolean;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

const SENSITIVE_KEYS = [
  "api_key",
  "apikey",
  "token",
  "password",
  "secret",
  "authorization",
  "cookie",
];

function maskArgs(args: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(args).map(([key, value]) => [
      key,
      SENSITIVE_KEYS.some((sensitive) => key.toLowerCase().includes(sensitive))
        ? "••••••••"
        : value,
    ]),
  );
}

function riskTone(risk: string): string {
  switch (risk) {
    case "critical":
      return "border-[#e05a80] bg-[#e05a80]/15 text-[#e05a80]";
    case "high":
      return "border-[#df7995] bg-[#df7995]/15 text-[#df7995]";
    case "medium":
      return "border-[#d9a441] bg-[#d9a441]/15 text-[#d9a441]";
    case "low":
      return "border-[#5ca8d6] bg-[#5ca8d6]/15 text-[#5ca8d6]";
    default:
      return "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning-fg)]";
  }
}

function cardRiskBorder(risk: string): string {
  switch (risk) {
    case "critical":
      return "border-[#e05a80] shadow-[0_0_16px_-4px_rgba(224,90,128,0.35)]";
    case "high":
      return "border-[#df7995] shadow-[0_0_12px_-4px_rgba(223,121,149,0.3)]";
    case "medium":
      return "border-[#d9a441] shadow-[0_0_12px_-4px_rgba(217,164,65,0.25)]";
    case "low":
      return "border-[#5ca8d6] shadow-[0_0_12px_-4px_rgba(92,168,214,0.25)]";
    default:
      return "border-[var(--warning-border)]";
  }
}

function riskLabel(risk: string): string {
  const labels: Record<string, string> = {
    critical: "严重风险",
    high: "高风险",
    medium: "中风险",
    low: "低风险",
  };
  return labels[risk] || "风险待确认";
}

function targetFromArgs(args: Record<string, unknown>): string | null {
  for (const key of ["target", "path", "file", "url", "command"]) {
    const value = args[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return null;
}

export default function ApprovalCard({
  approval,
  resolving = false,
  onApprove,
  onReject,
}: ApprovalCardProps) {
  const { session } = useGlobalVoiceContext();
  const cardRef = useRef<HTMLElement>(null);
  const [voiceProofUnavailable, setVoiceProofUnavailable] = useState(false);
  const isCritical = approval.risk_level === "critical";
  const target = targetFromArgs(approval.arguments || {});
  const maskedArgs = JSON.stringify(maskArgs(approval.arguments || {}), null, 2);

  useEffect(() => {
    if (
      approval.task_graph_id
      || !session
      || session.state === "ended"
      || session.conversational_anchor?.conversation_id !== approval.conversation_id
    ) return;
    const element = cardRef.current;
    if (!element || typeof IntersectionObserver === "undefined") return;
    const controller = new AbortController();
    let active = true;
    let visible = false;
    let issuing = false;
    let requestStarted = false;
    let displayId = crypto.randomUUID();
    let issued: VoiceApprovalAttestation | null = null;
    const revoke = () => {
      const current = issued;
      if (!current && !requestStarted) return;
      const revokedDisplayId = current?.display_id ?? displayId;
      issued = null;
      requestStarted = false;
      displayId = crypto.randomUUID();
      void revokeVoiceApprovalDisplay(
        approval.approval_id,
        revokedDisplayId,
      ).catch(() => undefined);
    };
    const attest = async () => {
      if (!active || issuing || issued || !visible || document.visibilityState !== "visible") return;
      issuing = true;
      requestStarted = true;
      const requestedDisplayId = displayId;
      try {
        const result = await attestVoiceApprovalDisplayed(
          approval.approval_id,
          session.voice_session_id,
          session.generation,
          requestedDisplayId,
          controller.signal,
        );
        if (!active || !visible || document.visibilityState !== "visible") {
          if (result) {
            void revokeVoiceApprovalDisplay(
              approval.approval_id,
              requestedDisplayId,
            ).catch(() => undefined);
          }
          return;
        }
        issued = result;
        requestStarted = false;
        setVoiceProofUnavailable(!result);
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setVoiceProofUnavailable(true);
        }
      } finally {
        issuing = false;
      }
    };
    const observer = new IntersectionObserver(([entry]) => {
      visible = Boolean(entry?.isIntersecting);
      if (visible) void attest();
      else revoke();
    }, { threshold: 0.5 });
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") void attest();
      else revoke();
    };
    observer.observe(element);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      active = false;
      controller.abort();
      observer.disconnect();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      revoke();
    };
  }, [
    approval.approval_id,
    approval.conversation_id,
    session?.generation,
    session?.voice_session_id,
    session?.conversational_anchor?.conversation_id,
  ]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing || resolving) return;
      const activeEl = document.activeElement;
      if (
        activeEl &&
        (activeEl.tagName === "INPUT" ||
          activeEl.tagName === "TEXTAREA" ||
          activeEl.tagName === "SELECT" ||
          (activeEl instanceof HTMLElement && activeEl.isContentEditable))
      ) {
        return;
      }
      if (event.key === "y" || event.key === "Y") {
        event.preventDefault();
        onApprove(approval);
      } else if (event.key === "n" || event.key === "N") {
        event.preventDefault();
        onReject(approval);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [approval, onApprove, onReject, resolving]);

  return (
    <section
      ref={cardRef}
      className={`conversation-approval-card overflow-hidden rounded-[var(--radius-lg)] border bg-[var(--surface-solid)] transition-shadow duration-[var(--motion-fast)] ${cardRiskBorder(approval.risk_level)}`}
      aria-label="等待用户确认"
    >
      <div
        className={
          "approval-card-header flex items-center gap-2 border-b px-4 py-3 " +
          (isCritical
            ? "border-[var(--danger-border)] bg-[var(--danger-soft)]"
            : "border-[var(--warning-border)] bg-[var(--warning-soft)]")
        }
      >
        {isCritical ? (
          <ShieldAlert className="h-4 w-4 text-[var(--danger)]" />
        ) : (
          <ShieldCheck className="h-4 w-4 text-[var(--accent-gold)]" />
        )}
        <div>
          <p className="approval-card-title text-sm font-semibold text-[var(--text-primary)]">
            <span className="approval-card-star" aria-hidden="true">✦</span>
            需要你的许可
          </p>
          <p className="mt-0.5 text-xs text-[var(--text-secondary)]">
            这一步在得到确认前不会执行
          </p>
        </div>
        <span
          className={
            "ml-auto rounded-full border px-2 py-0.5 text-[10px] font-medium " +
            riskTone(approval.risk_level)
          }
        >
          {riskLabel(approval.risk_level)}
        </span>
      </div>

      <div className="approval-card-details space-y-3 px-4 py-3 text-xs">
        <div>
          <span className="text-[var(--text-faint)]">操作</span>
          <p className="mt-0.5 font-medium text-[var(--text-primary)]">
            {formatToolDisplayName(approval.tool_name)}
          </p>
        </div>
        {target && (
          <div>
            <span className="text-[var(--text-faint)]">目标</span>
            <p className="mt-0.5 break-all font-mono text-[var(--text-secondary)]">
              {target}
            </p>
          </div>
        )}
        {approval.reason && (
          <div>
            <span className="text-[var(--text-faint)]">原因</span>
            <p className="mt-0.5 leading-5 text-[var(--text-secondary)]">
              {approval.reason}
            </p>
          </div>
        )}

        <details className="approval-card-technical rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2">
          <summary className="cursor-pointer select-none text-[var(--text-secondary)] hover:text-[var(--text-primary)]">
            查看技术详情
          </summary>
          <dl className="mt-2 space-y-2 font-mono text-[11px]">
            <div>
              <dt className="font-sans text-[var(--text-faint)]">Tool Name</dt>
              <dd className="mt-0.5 break-all text-[var(--text-secondary)]">
                {approval.tool_name}
              </dd>
            </div>
            {typeof approval.arguments?.command === "string" && (
              <div>
                <dt className="font-sans text-[var(--text-faint)]">Command</dt>
                <dd className="mt-1 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-[#232032] p-2 text-xs font-mono text-[#eeeaf8] shadow-inner">
                  {approval.arguments.command}
                </dd>
              </div>
            )}
            <div>
              <dt className="font-sans text-[var(--text-faint)]">Arguments</dt>
              <dd className="mt-1 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-[#232032] p-2 text-xs font-mono text-[#eeeaf8] shadow-inner">
                {maskedArgs}
              </dd>
            </div>
          </dl>
        </details>

        <div className="approval-card-actions flex flex-wrap items-center justify-end gap-2 pt-1">
          <Button
            size="sm"
            variant="secondary"
            loading={resolving}
            loadingLabel="提交中…"
            disabled={resolving}
            onClick={() => onReject(approval)}
            aria-label="拒绝此次工具操作"
          >
            拒绝 <kbd className="ml-1 text-[10px] opacity-75 font-mono">N</kbd>
          </Button>
          <Button
            size="sm"
            loading={resolving}
            loadingLabel="提交中…"
            disabled={resolving}
            onClick={() => onApprove(approval)}
            aria-label="允许此次工具操作"
          >
            允许本次 <kbd className="ml-1 text-[10px] opacity-80 font-mono">Y</kbd>
          </Button>
        </div>
        {voiceProofUnavailable ? (
          <p className="text-[11px] text-[var(--text-faint)]" role="status">
            语音确认暂不可用，请使用卡片按钮。
          </p>
        ) : null}
      </div>
    </section>
  );
}
