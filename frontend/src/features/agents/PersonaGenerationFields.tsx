import { useEffect, useState } from "react";
import { Button, Input } from "../../components/ui";
import { getAgentGenerationOptions, type GenerationOptions, type GenerationOverrides } from "./persona";

export default function PersonaGenerationFields({ agentId, enabled, value, onChange }: {
  agentId: string;
  enabled: boolean;
  value: GenerationOverrides;
  onChange: (value: GenerationOverrides) => void;
}) {
  const [options, setOptions] = useState<GenerationOptions>();
  const [error, setError] = useState("");
  const [opened, setOpened] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!opened) return;
    let current = true;
    setError("");
    setOptions(undefined);
    void getAgentGenerationOptions(agentId)
      .then(result => { if (current) setOptions(result); })
      .catch(cause => { if (current) setError(String(cause)); });
    return () => { current = false; };
  }, [agentId, attempt, opened]);

  const hasOverrides = Object.values(value).some(item => item != null);
  const hasControls = options && (options.temperature || options.top_p || options.max_output_tokens !== null);
  const savedUnavailable = options && ((value.temperature != null && !options.temperature)
    || (value.top_p != null && !options.top_p)
    || (value.max_output_tokens != null && options.max_output_tokens === null));
  const change = (key: keyof GenerationOverrides, input: string) => {
    onChange({ ...value, [key]: input === "" ? null : Number(input) });
  };

  return <details className="management-disclosure persona-extra-fields" onToggle={event => setOpened(event.currentTarget.open)}>
    <summary>模型生成参数{hasOverrides && " · 已覆盖"}</summary>
    <div className="management-disclosure-body">
      <p className="persona-editor-note">参数只用于这个角色。留空继承模型默认值。切换模型或开启思考后，以实际模型支持的参数为准。</p>
      {!enabled && <p className="persona-editor-note">启用角色设定后，保存的覆盖值才会生效。</p>}
      {opened && !options && !error && <p role="status" className="persona-editor-note">正在读取模型支持的参数…</p>}
      {error && <div><p role="alert" className="persona-editor-note">无法读取模型参数。{error}</p><Button type="button" variant="secondary" size="sm" onClick={() => setAttempt(value => value + 1)}>重试</Button></div>}
      {options && <p className="persona-editor-note">当前默认路由为 {options.model}。{!hasControls && "当前路由没有已声明的可覆盖参数。"}</p>}
      {savedUnavailable && <p className="persona-editor-note">当前路由不支持部分已保存的覆盖值。切换到支持的模型后可以继续使用，也可以恢复模型默认值。</p>}
      {options?.temperature && <Input label="温度" type="number" min={0} max={2} step="any" disabled={value.top_p != null}
        value={value.temperature ?? ""} onChange={event => change("temperature", event.target.value)}
        placeholder={options.default_temperature == null ? "模型默认" : String(options.default_temperature)}
        hint="调节生成时对不同词语的采样分布。温度和采样范围可覆盖其中一项。" />}
      {options?.top_p && <Input label="采样范围（top_p）" type="number" min={0.01} max={1} step="any" disabled={value.temperature != null}
        value={value.top_p ?? ""} onChange={event => change("top_p", event.target.value)} placeholder="1（模型默认）"
        hint="只从累计概率达到此值的候选中采样。" />}
      {options?.max_output_tokens != null && <Input label="单次输出上限（tokens）" type="number" min={1} max={1_000_000} step={1}
        value={value.max_output_tokens ?? ""} onChange={event => change("max_output_tokens", event.target.value)}
        placeholder={String(options.max_output_tokens)} hint={`包括模型计入输出的推理 tokens。当前路由上限为 ${options.max_output_tokens}，更严格的任务预算也会生效。`} />}
      {hasOverrides && <Button type="button" variant="secondary" size="sm" onClick={() => onChange({})}>恢复模型默认值</Button>}
    </div>
  </details>;
}
