import type { ProviderConnectionKind } from "../../../api/providerConnection";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import ModelManagerPanel from "../../llm/ModelManagerPanel";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";

interface ModelSettingsProps {
  actions?: React.ReactNode;
  dirty: boolean;
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
  actions,
  dirty,
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
      <ModelManagerPanel
        actions={<><ProviderReadinessCard dirty={dirty} readiness={readiness} kinds={["model"]} testing={testing} results={results} onVerify={onVerify} />{actions}</>}
        legacyModel={{ config, updateField, clearSecret, secretSourceLabel, secretRefreshToken }}
      />
    </div>
  );
}
