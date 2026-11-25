import { useState } from "react";
import { Button, Modal } from "../../components/ui";
import DesktopPetSettings from "./DesktopPetSettings";
import DesktopThemeSettings from "./DesktopThemeSettings";
import { screenCaptureAvailable } from "./ScreenCaptureDialog";
import VTubeStudioSettings from "./VTubeStudioSettings";
import RainmeterSettings from "./RainmeterSettings";

export default function DesktopSettingsDialog({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<"pet" | "theme" | "studio" | "rainmeter">("pet");
  return <Modal open title="桌面与桌宠" size="md" onClose={onClose}>
    {screenCaptureAvailable() && <div className="mb-3 flex flex-wrap gap-2">
      <Button size="sm" variant={tab === "pet" ? "primary" : "secondary"} aria-pressed={tab === "pet"}
        onClick={() => setTab("pet")}>桌宠设置</Button>
      <Button size="sm" variant={tab === "theme" ? "primary" : "secondary"} aria-pressed={tab === "theme"}
        onClick={() => setTab("theme")}>桌面主题</Button>
      <Button size="sm" variant={tab === "studio" ? "primary" : "secondary"} aria-pressed={tab === "studio"}
        onClick={() => setTab("studio")}>VTube Studio</Button>
      <Button size="sm" variant={tab === "rainmeter" ? "primary" : "secondary"} aria-pressed={tab === "rainmeter"}
        onClick={() => setTab("rainmeter")}>Rainmeter</Button>
    </div>}
    {tab === "pet" && <DesktopPetSettings />}
    {tab === "theme" && <DesktopThemeSettings />}
    {tab === "studio" && <VTubeStudioSettings />}
    {tab === "rainmeter" && <RainmeterSettings />}
  </Modal>;
}
