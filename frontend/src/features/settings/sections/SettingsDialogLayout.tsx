import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { Button, Modal, type ModalSize } from "../../../components/ui";

interface SettingsDialogLayoutProps {
  title: string;
  onClose?: () => void;
  size?: ModalSize;
  closeDisabled?: boolean;
  returnFocusRef?: RefObject<HTMLElement>;
  unsaved?: boolean;
  error?: string;
  notice?: string;
  secondaryActions?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}

export default function SettingsDialogLayout({ title, onClose, size = "md", closeDisabled, returnFocusRef, unsaved, error, notice, secondaryActions, action, children }: SettingsDialogLayoutProps) {
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const continueEditing = useRef<HTMLButtonElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (confirmDiscard) continueEditing.current?.focus();
    else if (previousFocus.current) {
      previousFocus.current.focus();
      previousFocus.current = null;
    }
  }, [confirmDiscard]);

  function requestClose() {
    if (closeDisabled) return;
    if (confirmDiscard) setConfirmDiscard(false);
    else if (unsaved) {
      previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setConfirmDiscard(true);
    }
    else onClose?.();
  }
  const footer = (
    <div className="settings-dialog-footer">
      {error ? <p className="settings-save-error settings-dialog-error" role="alert">{error}</p>
        : notice && <p className="settings-help settings-dialog-notice" role="status">{notice}</p>}
      <div className="settings-header-actions settings-dialog-actions">
        {secondaryActions}
        {onClose && <Button variant="secondary" disabled={closeDisabled} onClick={requestClose}>关闭</Button>}
        {action}
      </div>
    </div>
  );
  if (!onClose) return <section className="space-y-3" aria-label={title}>{children}{footer}</section>;
  return (
    <Modal open onClose={requestClose} closeDisabled={closeDisabled} returnFocusRef={returnFocusRef} title={title} size={size} className="settings-node-editor settings-node-editor-compact"
      footer={confirmDiscard ? <>
        <Button ref={continueEditing} variant="secondary" onClick={() => setConfirmDiscard(false)}>继续编辑</Button>
        <Button variant="danger" onClick={onClose}>放弃并关闭</Button>
      </> : footer}>
      {confirmDiscard && <p role="status" className="text-sm">这次修改尚未保存。继续编辑，或放弃修改并关闭设置。</p>}
      <div className="settings-form space-y-3" hidden={confirmDiscard}>{children}</div>
    </Modal>
  );
}
