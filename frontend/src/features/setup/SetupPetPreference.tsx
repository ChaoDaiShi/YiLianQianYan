import { useEffect, useState } from "react";
import { Button } from "../../components/ui";
import {
  closeFloatingSurface,
  desktopFloatingAvailable,
  isDesktopPetOpen,
  listenDesktopPetOpen,
  openDesktopPet,
} from "../voice/floatingSurface";

export default function SetupPetPreference() {
  const [open, setOpen] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!desktopFloatingAvailable()) return;
    let active = true;
    let release: (() => void) | undefined;
    const refresh = () =>
      isDesktopPetOpen().then((value) => {
        if (active) setOpen(value);
      });
    void listenDesktopPetOpen((value) => {
      if (active) setOpen(value);
    })
      .then((fn) => {
        if (active) {
          release = fn;
          return refresh();
        } else fn();
      })
      .catch(() => {
        if (active) setError("无法读取桌宠窗口状态。");
      });
    return () => {
      active = false;
      release?.();
    };
  }, []);
  async function change() {
    setBusy(true);
    try {
      if (open) await closeFloatingSurface("pet");
      else await openDesktopPet();
      setOpen(await isDesktopPetOpen());
      setError("");
    } catch {
      setError("桌宠窗口暂时无法更新。");
    } finally {
      setBusy(false);
    }
  }
  if (!desktopFloatingAvailable()) return null;
  return (
    <div>
      <Button variant="secondary" disabled={busy || open === null} onClick={() => void change()}>
        {open ? "关闭桌宠" : "开启桌宠"}
      </Button>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
