import type { ProviderConnectionKind } from "../../../api/providerConnection";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import ModelManagerPanel from "../../llm/ModelManagerPanel";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";

interface ModelSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
  clearSecret: (field: "api_key" | "embedding_api_key") => Promise<void>;
  secretRefreshToken: number;
  readiness: ProviderReadinessProjection | null;
  testing: ProviderConnectionKind | null;
  results: Partial<Record<ProviderConnectionKind, string>>;
  onVerify: (kind: ProviderConnectionKind) => void;
}

export default function ModelSettings({
  config,
  updateField,
  clearSecret,
  secretRefreshToken,
  readiness,
  testing,
  results,
  onVerify,
}: ModelSettingsProps) {
  return (
    <div className="space-y-4">
      <ProviderReadinessCard readiness={readiness} kinds={["model"]} testing={testing} results={results} onVerify={onVerify} />
      <ModelManagerPanel
        legacyModel={{ config, updateField, clearSecret, secretSourceLabel, secretRefreshToken }}
      />
    </div>
  );
}
