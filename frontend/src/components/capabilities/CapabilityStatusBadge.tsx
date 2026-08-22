import { Badge } from "../ui";
import type { CapabilityTone } from "./capabilityPresentation";

interface CapabilityStatusBadgeProps {
  label: string;
  tone: CapabilityTone;
  detail?: string;
}

export default function CapabilityStatusBadge({ label, tone, detail }: CapabilityStatusBadgeProps) {
  return (
    <span role="status" aria-label={detail ? `${label}：${detail}` : label}>
      <Badge tone={tone}>{label}</Badge>
    </span>
  );
}
