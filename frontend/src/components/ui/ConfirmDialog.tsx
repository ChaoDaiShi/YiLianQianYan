import { useState, useEffect, useRef, type FormEvent } from "react";
import { AlertTriangle, AlertCircle } from "lucide-react";
import Modal from "./Modal";
import Button from "./Button";
import Input from "./Input";

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "default" | "danger";
  requireText?: string;
  loading?: boolean;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}

export default function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "确认",
  cancelLabel = "取消",
  tone = "default",
  requireText,
  loading = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const [typedText, setTypedText] = useState("");
  const cancelBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (open) {
      setTypedText("");
      // Focus cancel button for safety
      setTimeout(() => {
        cancelBtnRef.current?.focus();
      }, 50);
    }
  }, [open]);

  const isConfirmedAllowed = !requireText || typedText.trim() === requireText.trim();

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (isConfirmedAllowed && !loading) {
      onConfirm();
    }
  };

  const footer = (
    <>
      <Button
        ref={cancelBtnRef}
        variant="secondary"
        onClick={onCancel}
        disabled={loading}
      >
        {cancelLabel}
      </Button>
      <Button
        variant={tone === "danger" ? "danger" : "primary"}
        onClick={onConfirm}
        disabled={!isConfirmedAllowed}
        loading={loading}
        loadingLabel="处理中"
      >
        {confirmLabel}
      </Button>
    </>
  );

  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      size="sm"
      footer={footer}
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <div
            className={`shrink-0 rounded-[var(--radius-sm)] p-2 ${
              tone === "danger"
                ? "bg-[var(--danger-soft)] text-[var(--danger)]"
                : "bg-[var(--accent-soft)] text-[var(--accent-primary)]"
            }`}
          >
            {tone === "danger" ? (
              <AlertTriangle className="h-5 w-5" />
            ) : (
              <AlertCircle className="h-5 w-5" />
            )}
          </div>
          <p className="text-sm text-[var(--text-secondary)] leading-relaxed pt-1">
            {description}
          </p>
        </div>

        {requireText && (
          <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3">
            <span className="text-xs text-[var(--text-secondary)]">
              请输入 <strong className="font-mono text-[var(--text-primary)]">{requireText}</strong> 以确认操作：
            </span>
            <Input
              value={typedText}
              onChange={(e) => setTypedText(e.target.value)}
              placeholder={requireText}
              disabled={loading}
              autoFocus
            />
          </div>
        )}
      </form>
    </Modal>
  );
}
