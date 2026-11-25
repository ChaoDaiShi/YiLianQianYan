import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Modal } from "../../components/ui";

const captureErrors: Record<string, string> = {
  screen_permission_denied: "请先在个人资料的权限设置中开启屏幕感知。",
  screen_permission_revoked: "屏幕感知权限已变更，请重新选择。",
  screen_permission_unavailable: "无法读取屏幕感知权限，请稍后重试。",
  screen_capture_unsupported: "当前设备不支持 Windows 图形捕获。",
  screen_capture_busy: "已有一个屏幕选取正在进行。",
  screen_frame_timeout: "选取的窗口没有提供画面。请确保它没有最小化后重试。",
};

export function screenCaptureAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window &&
    navigator.userAgent.includes("Windows");
}

interface Snapshot {
  file: File;
  preview: string;
}

export default function ScreenCaptureDialog({ onClose, onAttach }: {
  onClose: () => void;
  onAttach: (file: File) => void;
}) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const active = useRef(false);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      void invoke("desktop_cancel_capture").catch(() => undefined);
    };
  }, []);
  useEffect(() => () => {
    if (snapshot) URL.revokeObjectURL(snapshot.preview);
  }, [snapshot]);

  async function choose() {
    setBusy(true);
    setSnapshot(null);
    setStatus("");
    setError("");
    try {
      const bytes = await invoke<ArrayBuffer>("desktop_capture_screen");
      if (!active.current) return;
      if (!bytes.byteLength) {
        setStatus("已取消选取。");
        return;
      }
      const file = new File([bytes], "屏幕截取.png", { type: "image/png" });
      setSnapshot({ file, preview: URL.createObjectURL(file) });
    } catch (reason) {
      if (active.current) setError(captureErrors[String(reason)] || "无法取得所选画面，请重试。");
    } finally {
      if (active.current) setBusy(false);
    }
  }

  return (
    <Modal open title="屏幕选取" size="md" onClose={onClose}>
      <div className="space-y-3">
        <p className="text-sm text-[var(--text-secondary)]">
          在 Windows 选择窗口或显示器，仅截取一张图片。预览关闭后即清除，选择添加后图片会按聊天附件保存，发送时由当前模型处理。
        </p>
        {snapshot && <img src={snapshot.preview} alt="所选窗口或显示器的画面" className="block max-h-[45vh] w-full rounded-lg object-contain" />}
        {busy && <p role="status" className="text-sm">正在等待 Windows 选取画面…</p>}
        {status && <p role="status" className="text-sm">{status}</p>}
        {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>{busy ? "停止选取" : "取消"}</Button>
          <Button variant={snapshot ? "secondary" : "primary"} disabled={busy} onClick={() => void choose()}>
            {snapshot ? "重新选取" : "选择窗口或显示器"}
          </Button>
          {snapshot && <Button onClick={() => { onAttach(snapshot.file); onClose(); }}>添加到聊天</Button>}
        </div>
      </div>
    </Modal>
  );
}
