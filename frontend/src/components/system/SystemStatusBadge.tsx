import Badge from "../ui/Badge";
import { formatSystemStatus, type SystemTone } from "./systemPresentation";

export default function SystemStatusBadge({
  status,
  label,
}: {
  status: string | null | undefined;
  label?: string;
}) {
  const presentation = formatSystemStatus(status);
  const tone: SystemTone = presentation.tone;
  return (
    <span aria-label={`${label ?? "状态"}：${presentation.label}`}>
      <Badge tone={tone}>
        <span aria-hidden="true">●</span>
        {presentation.label}
      </Badge>
    </span>
  );
}
