import { useState } from "react";
import type { ProviderConnectionKind } from "../../../api/providerConnection";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import QuickModelSetup from "../../llm/QuickModelSetup";
import { ModelManagerPanel } from "../../llm";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";

interface ModelSettingsProps {
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
  const [modelRevision, setModelRevision] = useState(0);
  return (
    <div className="space-y-4">
      <QuickModelSetup onSaved={() => setModelRevision(value => value + 1)} />
      <details className="u2-disclosure"><summary>管理已有模型与旧配置</summary><div className="pt-4"><ModelManagerPanel key={modelRevision}
        actions={<ProviderReadinessCard dirty={dirty} readiness={readiness} kinds={["model"]} testing={testing} results={results} onVerify={onVerify} />}
        legacyModel={{ config, updateField, clearSecret, secretSourceLabel, secretRefreshToken }}
      /></div></details>
    </div>
  );
}
