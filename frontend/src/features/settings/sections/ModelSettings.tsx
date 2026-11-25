import { useState, type ReactNode } from "react";
import type { ProviderConnectionKind } from "../../../api/providerConnection";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import QuickModelSetup from "../../llm/QuickModelSetup";
import ModelConfiguration from "../../llm/ModelConfiguration";
import { ModelManagerPanel } from "../../llm";
import { Button } from "../../../components/ui";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";
import SettingsDialogLayout from "./SettingsDialogLayout";

interface ModelSettingsProps {
  onClose: () => void;
  action?: ReactNode;
  secondaryActions?: ReactNode;
  error: string;
  notice: string;
  dirty: boolean;
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
  clearSecret: (field: "api_key" | "embedding_api_key") => Promise<void>;
  secretRefreshToken: number;
  readiness: ProviderReadinessProjection | null;
  testing: ProviderConnectionKind | null;
  results: Partial<Record<ProviderConnectionKind, string>>;
  onVerify: (kind: ProviderConnectionKind) => void;
  onConfigurationChanged?: (config: AppConfig) => void;
}

export default function ModelSettings({
  onClose,
  action,
  secondaryActions,
  error,
  notice,
  dirty,
  config,
  updateField,
  clearSecret,
  secretRefreshToken,
  readiness,
  testing,
  results,
  onVerify,
  onConfigurationChanged,
}: ModelSettingsProps) {
  const [modelRevision, setModelRevision] = useState(0);
  const [showLegacy, setShowLegacy] = useState(false);
  if (!showLegacy) return <ModelConfiguration onClose={onClose} onLegacy={() => setShowLegacy(true)} onChanged={onConfigurationChanged} />;
  return (
    <QuickModelSetup onSaved={() => setModelRevision(value => value + 1)}>
      {(quick) => (
        <SettingsDialogLayout title="基础对话配置与用量" size="lg" onClose={onClose} action={action ?? quick.action} secondaryActions={<>
          <Button variant="ghost" disabled={dirty} onClick={() => setShowLegacy(false)}>返回模型配置</Button>{secondaryActions}
        </>}
          error={error || (!action ? quick.error : "")} notice={notice || (!action ? quick.notice : "")}>
          {quick.fields}
          <details className="u2-disclosure"><summary>管理已保存模型</summary><div className="pt-4"><ModelManagerPanel key={modelRevision}
            actions={<ProviderReadinessCard dirty={dirty} readiness={readiness} kind="model" testing={testing} results={results} onVerify={onVerify} />}
            legacyModel={{ config, updateField, clearSecret, secretSourceLabel, secretRefreshToken }}
          /></div></details>
        </SettingsDialogLayout>
      )}
    </QuickModelSetup>
  );
}
