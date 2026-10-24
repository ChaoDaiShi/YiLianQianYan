import { useEffect, useState, type ReactNode } from "react";
import { Button, ErrorState, Input, Skeleton, Textarea, showToast } from "../../components/ui";
import { emptyProfile, getAgentProfile, saveAgentProfile, type AgentProfile, type Persona } from "./persona";

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
      .then(value => { if (current) { const draft = window.sessionStorage.getItem(draftKey); setProfile(draft ? JSON.parse(draft) as AgentProfile : value); setSavedProfile(JSON.stringify(value)); setReady(true); } })
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

  function field(key: keyof Persona, value: string) {
    setProfile(current => ({ ...current, persona: { ...current.persona, [key]: value || (key === "avatar_resource_id" ? null : "") } }));
  }

  async function save() {
    setSaving(true);
    setError("");
    try {
      await saveAgentProfile(agentId, profile);
      window.sessionStorage.removeItem(draftKey);
      setSavedProfile(JSON.stringify(profile));
      window.dispatchEvent(new CustomEvent("agent-persona-changed", { detail: { agentId, profile } }));
      showToast("角色已保存，将用于下一轮回复", "success");
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
      <Input label="名称" aria-label="角色名称" placeholder={agentName} value={profile.persona.display_name} onChange={event => field("display_name", event.target.value)} />
      <Textarea label="表达风格" aria-label="语气与表达风格" placeholder="例如：自然、简洁，有条理" value={profile.persona.tone} onChange={event => field("tone", event.target.value)} rows={2} />
      <Textarea label="用户提示词" aria-label="角色用户提示词" value={profile.user_prompt} onChange={event => setProfile({ ...profile, user_prompt: event.target.value })} rows={5} />
      <label className="persona-enabled-control"><input type="checkbox" checked={profile.persona_enabled} onChange={event => setProfile({ ...profile, persona_enabled: event.target.checked })} /><span>启用角色设定</span></label>
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
      {onCancel && <Button type="button" variant="secondary" disabled={saving} onClick={onCancel}>取消</Button>}
      <Button type="submit" disabled={!ready || loading || saving} loading={saving} loadingLabel="正在保存…">保存角色</Button>
    </div>
  </form>;
}
