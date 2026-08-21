import { useCallback, useEffect, useMemo, useState } from "react";
import { Brain, Pencil, Plus, Trash2 } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import {
  createSkill,
  deleteSkill,
  listSkills,
  loadSkill,
  updateSkill,
  type SkillSummary,
} from "../api/client";
import { CapabilityDetailSection, CapabilityMetaRow } from "../components/capabilities";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Input,
  Modal,
  PageHeader,
  Skeleton,
  Textarea,
} from "../components/ui";
import { validateSkillDraft } from "../features/skills/skillForm";

type EditorMode = "create" | "edit" | null;

export default function SkillsPage() {
  const [searchParams] = useSearchParams();
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState("");
  const [detailError, setDetailError] = useState("");
  const [search, setSearch] = useState("");
  const [editorMode, setEditorMode] = useState<EditorMode>(null);
  const [formName, setFormName] = useState("");
  const [formContent, setFormContent] = useState("");
  const [formError, setFormError] = useState("");
  const [saving, setSaving] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    const result = await listSkills();
    if (result.ok) setSkills(result.data);
    else setError(result.error || "技能列表暂时无法加载。");
    setLoading(false);
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const openSkill = useCallback(async (skill: SkillSummary) => {
    setSelectedName(skill.name);
    setDetailLoading(true);
    setDetailError("");
    const result = await loadSkill(skill.name);
    if (result.ok) setSkillContent(result.data.content);
    else setDetailError(result.error || "无法加载技能内容。请检查技能目录与后端连接。");
    setDetailLoading(false);
  }, []);

  useEffect(() => {
    const requested = searchParams.get("skill");
    if (!requested || selectedName === requested) return;
    const skill = skills.find((item) => item.name === requested);
    if (skill) void openSkill(skill);
  }, [openSkill, searchParams, selectedName, skills]);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return skills;
    return skills.filter((skill) =>
      [skill.name, skill.description, skill.path].some((value) => value.toLowerCase().includes(query))
    );
  }, [skills, search]);

  const selectedSkill = skills.find((skill) => skill.name === selectedName) ?? null;

  const openCreate = () => {
    setEditorMode("create");
    setFormName("");
    setFormContent("# 新技能\n\n请描述这个技能的用途和执行步骤。\n");
    setFormError("");
  };

  const openEdit = () => {
    if (!selectedSkill?.editable) return;
    setEditorMode("edit");
    setFormName(selectedSkill.name);
    setFormContent(skillContent);
    setFormError("");
  };

  const closeEditor = () => {
    if (saving) return;
    setEditorMode(null);
    setFormError("");
  };

  const saveSkill = async () => {
    const validation = validateSkillDraft({ name: formName, content: formContent });
    if (!validation.ok) {
      setFormError(validation.error);
      return;
    }
    setSaving(true);
    setFormError("");
    const result = editorMode === "edit" && selectedSkill
      ? await updateSkill(selectedSkill.name, validation.value)
      : await createSkill(validation.value);
    setSaving(false);
    if (!result.ok) {
      setFormError(result.error);
      return;
    }
    setSelectedName(validation.value.name);
    setSkillContent(validation.value.content);
    setEditorMode(null);
    await reload();
  };

  const removeSkill = async () => {
    if (!selectedSkill?.editable) return;
    if (!window.confirm(`确定删除技能“${selectedSkill.name}”及其目录内容吗？`)) return;
    setDetailError("");
    const result = await deleteSkill(selectedSkill.name);
    if (!result.ok) {
      setDetailError(result.error);
      return;
    }
    setSelectedName(null);
    setSkillContent("");
    await reload();
  };

  return (
    <div className="capability-page skills-page page-canvas">
      <PageHeader
        title="技能"
        description="管理小昔涟可以使用的技能能力。"
        actions={<Button onClick={openCreate}><Plus className="h-4 w-4" />新增技能</Button>}
      />

      <div className="capability-page-body skills-page-body">
        <div className="capability-split-layout skills-split-layout">
          <aside className="capability-list-panel" aria-label="技能列表">
            <div className="capability-list-toolbar">
              <Input aria-label="搜索技能" placeholder="搜索技能…" value={search} onChange={(event) => setSearch(event.target.value)} />
            </div>
            <div className="capability-list-scroll scrollbar-thin">
              {loading ? (
                <div className="capability-skeleton-stack" aria-label="正在加载技能">
                  <Skeleton className="h-16 w-full" /><Skeleton className="h-16 w-full" /><Skeleton className="h-16 w-full" /><Skeleton className="h-16 w-full" />
                </div>
              ) : error ? (
                <ErrorState title="技能暂时无法加载" description={error} action={<Button variant="secondary" size="sm" onClick={() => void reload()}>重试</Button>} className="m-3" />
              ) : filtered.length === 0 ? (
                <EmptyState title="这里还没有技能" description="可以新建一个工作区技能，或在设置中添加只读技能目录。" action={<Button size="sm" onClick={openCreate}>新增技能</Button>} className="py-14" />
              ) : (
                <div className="capability-row-stack">
                  {filtered.map((skill) => (
                    <button key={skill.name} type="button" aria-selected={selectedName === skill.name} onClick={() => void openSkill(skill)} className={`capability-list-row ${selectedName === skill.name ? "capability-list-row-selected" : ""}`}>
                      <span className="capability-list-row-heading"><Brain className="h-4 w-4 shrink-0 text-[var(--accent-purple)]" /><strong>{skill.name}</strong>{!skill.editable && <Badge tone="default">只读</Badge>}</span>
                      <span className="capability-list-row-description">{skill.description || "暂无描述"}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
            <p className="capability-list-footnote">工作区托管技能可编辑；外部技能目录保持只读。</p>
          </aside>

          <section className="capability-detail-panel" aria-label="技能详情">
            {selectedSkill ? (
              <div className="capability-detail-content">
                <div className="capability-detail-heading">
                  <div>
                    <div className="capability-detail-title-line"><Brain className="h-5 w-5 text-[var(--accent-primary)]" /><h2>{selectedSkill.name}</h2><Badge tone="accent">SKILL.md</Badge></div>
                    <p className="capability-detail-description">{selectedSkill.description || "暂无描述"}</p>
                  </div>
                  {selectedSkill.editable && (
                    <div className="flex flex-wrap gap-2">
                      <Button size="sm" variant="secondary" onClick={openEdit} disabled={detailLoading}><Pencil className="h-3.5 w-3.5" />编辑</Button>
                      <Button size="sm" variant="danger" onClick={() => void removeSkill()}><Trash2 className="h-3.5 w-3.5" />删除</Button>
                    </div>
                  )}
                </div>

                <CapabilityDetailSection title="技能内容">
                  {detailLoading ? (
                    <div className="space-y-2"><Skeleton className="h-5 w-11/12" /><Skeleton className="h-5 w-full" /><Skeleton className="h-24 w-full" /></div>
                  ) : detailError ? (
                    <ErrorState title="技能内容暂时无法加载" description={detailError} action={<Button variant="secondary" size="sm" onClick={() => void openSkill(selectedSkill)}>重试</Button>} />
                  ) : (
                    <pre className="capability-content-preview">{skillContent}</pre>
                  )}
                </CapabilityDetailSection>

                <CapabilityDetailSection title="技术详情" technical>
                  <dl className="capability-meta-list"><CapabilityMetaRow label="名称" value={selectedSkill.name} mono /><CapabilityMetaRow label="路径" value={selectedSkill.path} mono /><CapabilityMetaRow label="管理状态" value={selectedSkill.editable ? "工作区托管" : "外部只读"} /></dl>
                </CapabilityDetailSection>
              </div>
            ) : (
              <EmptyState icon={<Brain className="h-7 w-7" />} title="选择一个技能" description="查看真实的 SKILL.md 内容，或新建工作区技能。" action={<Button size="sm" onClick={openCreate}>新增技能</Button>} className="h-full min-h-[360px]" />
            )}
          </section>
        </div>
      </div>

      <Modal
        open={editorMode !== null}
        onClose={closeEditor}
        title={editorMode === "edit" ? "编辑技能" : "新增技能"}
        className="max-w-3xl"
        footer={<><Button variant="secondary" onClick={closeEditor} disabled={saving}>取消</Button><Button onClick={() => void saveSkill()} disabled={saving}>{saving ? "保存中…" : "保存技能"}</Button></>}
      >
        <div className="space-y-4">
          <Input label="技能名称" value={formName} onChange={(event) => setFormName(event.target.value)} placeholder="例如：organize-downloads" disabled={saving} />
          <Textarea label="SKILL.md 内容" value={formContent} onChange={(event) => setFormContent(event.target.value)} className="min-h-[360px] font-mono text-xs leading-5" disabled={saving} />
          {formError && <p className="text-sm text-[var(--danger)]" role="alert">{formError}</p>}
        </div>
      </Modal>
    </div>
  );
}
