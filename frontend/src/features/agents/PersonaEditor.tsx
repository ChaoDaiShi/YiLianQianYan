import { useEffect, useState, type ReactNode } from "react";
import { Button, ErrorState, Input, Select, Skeleton, Textarea, showToast } from "../../components/ui";
import { emptyProfile, getAgentProfile, saveAgentProfile, resetAgentProfile, type AgentProfile, type Persona, type ReplyVariation } from "./persona";
import PersonaGenerationFields from "./PersonaGenerationFields";

const variationOptions = {
  default: { label: "随模型", description: "由模型自行决定表达方式。" },
  consistent: { label: "稳定", description: "称呼、语气和组织方式保持一致。" },
  flexible: { label: "灵活", description: "保持角色特点，随话题调整用词和例子。" },
  playful: { label: "多变", description: "尝试不同的措辞、视角和叙述节奏。" },
} satisfies Record<ReplyVariation, { label: string; description: string }>;

export default function PersonaEditor({ agentId, agentName = "默认角色", onSaved, onCancel, children }: {
  agentId: string;
  agentName?: string;
  onSaved?: () => void;
  onCancel?: () => void;
  children?: ReactNode;
}) {
  const [profile, setProfile] = useState<AgentProfile>(emptyProfile);
  const [ready, setReady] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [savedProfile, setSavedProfile] = useState("");
  const draftKey = `yilian.persona-draft:${agentId}`;

  useEffect(() => {
    let current = true;
    setLoading(true);
    setReady(false);
    setError("");
    void getAgentProfile(agentId)
      .then(value => {
        if (!current) return;
        const draft = window.sessionStorage.getItem(draftKey);
        const saved = { ...value, persona: { ...emptyProfile().persona, ...value.persona } };
        if (draft) {
          const draftProfile = JSON.parse(draft) as AgentProfile;
          setProfile({ ...draftProfile, persona: { ...saved.persona, ...draftProfile.persona } });
        } else {
          setProfile(saved);
        }
        setSavedProfile(JSON.stringify(saved));
        setReady(true);
      })
      .catch(cause => { if (current) setError(String(cause)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [agentId]);

  useEffect(() => {
    if (!ready) return;
    const content = JSON.stringify(profile);
    if (content === savedProfile) window.sessionStorage.removeItem(draftKey);
    else window.sessionStorage.setItem(draftKey, content);
  }, [profile, savedProfile, ready, draftKey]);

  function field(key: Exclude<keyof Persona, "reply_variation" | "generation">, value: string) {
    setProfile(current => ({ ...current, persona: { ...current.persona, [key]: value || (key === "avatar_resource_id" ? null : "") } }));
  }

  async function save(reset = false) {
    setSaving(true);
    setError("");
    try {
      const saved = reset ? await resetAgentProfile(agentId) : await saveAgentProfile(agentId, profile);
      setProfile(saved);
      window.sessionStorage.removeItem(draftKey);
      setSavedProfile(JSON.stringify(saved));
      window.dispatchEvent(new CustomEvent("agent-persona-changed", { detail: { agentId, profile: saved } }));
      showToast(reset ? "角色已恢复默认设置" : "角色已保存，将用于下一轮回复", "success");
      onSaved?.();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  }

  return <form className="persona-editor" aria-label="角色编辑器" onSubmit={event => { event.preventDefault(); if (ready && !saving) void save(); }}>
    <div className="persona-editor-heading">
      <span className="persona-preview-avatar" aria-hidden="true" />
      <div><h2>{profile.persona.display_name || agentName}</h2><p>{profile.persona.description || "设置身份、表达风格与日常工作偏好"}</p></div>
    </div>
    {error && <ErrorState title="角色操作未完成" description={error} />}
    {loading ? <Skeleton className="h-56" /> : <fieldset disabled={!ready || saving} className="persona-editor-fields">
      <label className="persona-enabled-control"><input type="checkbox" checked={profile.persona_enabled} onChange={event => setProfile({ ...profile, persona_enabled: event.target.checked })} /><span>启用角色设定</span></label>
      <Input label="名称" aria-label="角色名称" placeholder={agentName} value={profile.persona.display_name} onChange={event => field("display_name", event.target.value)} />
      <Textarea label="表达风格" aria-label="语气与表达风格" placeholder="例如：自然、简洁，有条理" value={profile.persona.tone} onChange={event => field("tone", event.target.value)} rows={2} />
      <Select label="回复变化" value={profile.persona.reply_variation}
        options={Object.entries(variationOptions).map(([value, option]) => ({ value, label: option.label }))}
        hint={variationOptions[profile.persona.reply_variation].description}
        onChange={event => setProfile(current => ({ ...current, persona: { ...current.persona, reply_variation: event.target.value as ReplyVariation } }))} />
      <Textarea label="用户提示词" aria-label="角色用户提示词" value={profile.user_prompt} onChange={event => setProfile({ ...profile, user_prompt: event.target.value })} rows={5} />
      <PersonaGenerationFields agentId={agentId} enabled={profile.persona_enabled} value={profile.persona.generation ?? {}}
        onChange={generation => setProfile(current => ({ ...current, persona: { ...current.persona, generation } }))} />
      <details className="management-disclosure persona-extra-fields">
        <summary>扮演场景与回复示例</summary>
        <div className="management-disclosure-body">
          <Textarea label="扮演场景" placeholder="例如：你是博物馆讲解员，正在带第一次到访的观众参观。" value={profile.persona.scenario} onChange={event => field("scenario", event.target.value)} maxLength={4000} rows={3} />
          <Textarea label="回复示例" placeholder="写一段符合这个角色的典型回答。" value={profile.persona.reply_examples} onChange={event => field("reply_examples", event.target.value)} maxLength={6000} rows={4} />
        </div>
      </details>
      <details className="management-disclosure persona-extra-fields">
        <summary>角色简介、语言与头像</summary>
        <div className="management-disclosure-body">
          <Textarea label="角色简介" value={profile.persona.description} onChange={event => field("description", event.target.value)} rows={2} />
          <Input label="回复语言" value={profile.persona.language} onChange={event => field("language", event.target.value)} />
          <Input label="头像资源 ID" value={profile.persona.avatar_resource_id ?? ""} onChange={event => field("avatar_resource_id", event.target.value)} />
        </div>
      </details>
    </fieldset>}
    {children}
    {ready && JSON.stringify(profile) !== savedProfile && <p className="persona-editor-note" role="status">有未保存修改，草稿保留在当前窗口；点击保存角色后生效。</p>}
    <p className="persona-editor-note">关闭角色设定后，用户提示词仍然生效。执行权限由当前授权配置控制。</p>
    <div className="persona-editor-actions">
      <Button type="button" variant="secondary" disabled={!ready || saving} onClick={() => void save(true)}>恢复默认</Button>
      {onCancel && <Button type="button" variant="secondary" disabled={saving} onClick={onCancel}>取消</Button>}
      <Button type="submit" disabled={!ready || loading || saving} loading={saving} loadingLabel="正在保存…">保存角色</Button>
    </div>
  </form>;
}
