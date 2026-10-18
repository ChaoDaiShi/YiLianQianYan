import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Brain, Check, Copy, Pencil, Plus, Trash2 } from "lucide-react";
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
  ConfirmDialog,
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
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const requestIdRef = useRef(0);

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
    const reqId = ++requestIdRef.current;
    const result = await loadSkill(skill.name);
    if (requestIdRef.current !== reqId) return;
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
      [skill.name, skill.description, skill.path].some((value) =>
        value.toLowerCase().includes(query)
      )
    );
  }, [skills, search]);

  const selectedSkill = skills.find((skill) => skill.name === selectedName) ?? null;

  const handleCopy = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      setTimeout(() => {
        setCopiedKey((curr) => (curr === key ? null : curr));
      }, 2000);
    } catch {
      // Clipboard copy fallback
    }
  };

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
    const result =
      editorMode === "edit" && selectedSkill
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

  const confirmRemoveSkill = async () => {
    if (!deleteConfirm) return;
    setDetailError("");
    const targetName = deleteConfirm;
    const result = await deleteSkill(targetName);
    setDeleteConfirm(null);
    if (!result.ok) {
      setDetailError(result.error);
      return;
    }
    if (selectedName === targetName) {
      setSelectedName(null);
      setSkillContent("");
    }
    await reload();
  };

  const removeSkill = () => {
    if (!selectedSkill?.editable) return;
    setDeleteConfirm(selectedSkill.name);
  };

  return (
    <div className="capability-page skills-page page-canvas ui-opt-capabilities ui-opt-skills">
      <PageHeader
        title="技能"
        description="管理小昔涟可以使用的技能能力。"
        actions={
          <Button onClick={openCreate}>
            <Plus className="h-4 w-4" />
            新增技能
          </Button>
        }
      />

      <div className="capability-page-body skills-page-body">
        <div className="capability-split-layout skills-split-layout">
          <aside className="capability-list-panel" aria-label="技能列表">
            <div className="capability-list-toolbar">
              <Input
                aria-label="搜索技能"
                placeholder="搜索技能…"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </div>

            <div className="capability-list-scroll scrollbar-thin">
              {loading ? (
                <div className="capability-skeleton-stack" aria-label="正在加载技能">
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                </div>
              ) : error ? (
                <ErrorState
                  title="技能暂时无法加载"
                  description={error}
                  action={
                    <Button variant="secondary" size="sm" onClick={() => void reload()}>
                      重试
                    </Button>
                  }
                  className="m-3"
                />
              ) : skills.length === 0 ? (
                <EmptyState
                  title="这里还没有技能"
                  description="可以新建一个工作区技能，或在设置中添加只读技能目录。"
                  action={
                    <Button size="sm" onClick={openCreate}>
                      新增技能
                    </Button>
                  }
                  className="py-14"
                />
              ) : filtered.length === 0 ? (
                <EmptyState
                  title="没有匹配的技能"
                  description={`未找到与“${search}”匹配的技能名称或路径。`}
                  action={
                    <Button size="sm" variant="secondary" onClick={() => setSearch("")}>
                      清除搜索
                    </Button>
                  }
                  className="py-12"
                />
              ) : (
                <div className="capability-row-stack" role="listbox" aria-label="技能列表">
                  {filtered.map((skill) => {
                    const isSelected = selectedName === skill.name;
                    return (
                      <button
                        key={skill.name}
                        type="button"
                        role="option"
                        aria-selected={isSelected}
                        onClick={() => void openSkill(skill)}
                        className={`capability-list-row ${
                          isSelected ? "capability-list-row-selected" : ""
                        }`}
                      >
                        <span className="capability-list-row-heading">
                          <Brain className="h-4 w-4 shrink-0 text-[var(--accent-purple)]" />
                          <strong>{skill.name}</strong>
                          {skill.editable ? (
                            <Badge tone="accent">可编辑</Badge>
                          ) : (
                            <Badge tone="default">只读</Badge>
                          )}
                        </span>
                        <span className="capability-list-row-description">
                          {skill.description || "暂无描述"}
                        </span>
                        <span className="text-[11px] text-[var(--text-faint)] font-mono truncate block mt-0.5">
                          {skill.path}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
            <p className="capability-list-footnote">
              工作区托管技能可编辑；外部技能目录保持只读。
            </p>
          </aside>

          <section className="capability-detail-panel" aria-label="技能详情">
            {selectedSkill ? (
              <div className="capability-detail-content">
                <div className="capability-detail-heading">
                  <div>
                    <div className="capability-detail-title-line flex items-center gap-2 flex-wrap">
                      <Brain className="h-5 w-5 text-[var(--accent-primary)]" />
                      <h2>{selectedSkill.name}</h2>
                      <button
                        type="button"
                        className="skill-copy-btn"
                        onClick={() => void handleCopy(`name-${selectedSkill.name}`, selectedSkill.name)}
                        title="复制技能名称"
                        aria-label={`复制技能名 ${selectedSkill.name}`}
                      >
                        {copiedKey === `name-${selectedSkill.name}` ? (
                          <Check className="h-3.5 w-3.5 text-[var(--success)]" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                        <span>{copiedKey === `name-${selectedSkill.name}` ? "已复制名称" : "复制名称"}</span>
                      </button>
                      <Badge tone={selectedSkill.editable ? "accent" : "default"}>
                        {selectedSkill.editable ? "可编辑" : "只读"}
                      </Badge>
                    </div>
                    <p className="capability-detail-description mt-1">
                      {selectedSkill.description || "暂无描述"}
                    </p>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    {selectedSkill.editable ? (
                      <>
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={openEdit}
                          disabled={detailLoading}
                          aria-label={`编辑 ${selectedSkill.name}`}
                        >
                          <Pencil className="h-3.5 w-3.5" />
                          编辑
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          onClick={() => void removeSkill()}
                          aria-label={`删除 ${selectedSkill.name}`}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          删除
                        </Button>
                      </>
                    ) : (
                      <span className="text-xs text-[var(--text-faint)] italic px-2 py-1">
                        外部目录技能保持只读，不可在线修改
                      </span>
                    )}
                  </div>
                </div>

                <CapabilityDetailSection title="技能内容">
                  {detailLoading ? (
                    <div className="space-y-2">
                      <Skeleton className="h-5 w-11/12" />
                      <Skeleton className="h-5 w-full" />
                      <Skeleton className="h-28 w-full" />
                    </div>
                  ) : detailError ? (
                    <ErrorState
                      title="技能内容暂时无法加载"
                      description={detailError}
                      action={
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => void openSkill(selectedSkill)}
                        >
                          重试
                        </Button>
                      }
                    />
                  ) : (
                    <div className="space-y-2">
                      <div className="flex justify-end">
                        <button
                          type="button"
                          className="skill-copy-btn text-xs text-[var(--text-muted)] hover:text-[var(--text)]"
                          onClick={() => void handleCopy(`content-${selectedSkill.name}`, skillContent)}
                          aria-label="复制技能全文"
                        >
                          {copiedKey === `content-${selectedSkill.name}` ? (
                            <Check className="h-3.5 w-3.5 text-[var(--success)]" />
                          ) : (
                            <Copy className="h-3.5 w-3.5" />
                          )}
                          <span>{copiedKey === `content-${selectedSkill.name}` ? "已复制全文" : "复制全文"}</span>
                        </button>
                      </div>
                      <pre className="skill-reading-content leading-[1.6]">
                        {skillContent || "（内容为空）"}
                      </pre>
                    </div>
                  )}
                </CapabilityDetailSection>

                <CapabilityDetailSection title="技术详情" technical>
                  <dl className="capability-meta-list">
                    <CapabilityMetaRow
                      label="名称"
                      value={
                        <span className="inline-flex items-center gap-2">
                          <span>{selectedSkill.name}</span>
                          <button
                            type="button"
                            className="skill-copy-btn"
                            onClick={() => void handleCopy(`meta-name-${selectedSkill.name}`, selectedSkill.name)}
                            aria-label={`复制名称 ${selectedSkill.name}`}
                          >
                            <Copy className="h-3 w-3" />
                          </button>
                        </span>
                      }
                      mono
                    />
                    <CapabilityMetaRow
                      label="路径"
                      value={
                        <span className="inline-flex items-center gap-2">
                          <span>{selectedSkill.path}</span>
                          <button
                            type="button"
                            className="skill-copy-btn"
                            onClick={() => void handleCopy(`meta-path-${selectedSkill.name}`, selectedSkill.path)}
                            aria-label={`复制路径 ${selectedSkill.path}`}
                          >
                            <Copy className="h-3 w-3" />
                          </button>
                        </span>
                      }
                      mono
                    />
                    <CapabilityMetaRow
                      label="管理状态"
                      value={selectedSkill.editable ? "工作区托管（可编辑）" : "外部目录（只读）"}
                    />
                  </dl>
                </CapabilityDetailSection>
              </div>
            ) : (
              <EmptyState
                icon={<Brain className="h-7 w-7" />}
                title="选择一个技能"
                description="查看真实的 SKILL.md 内容，或新建工作区技能。"
                action={
                  <Button size="sm" onClick={openCreate}>
                    新增技能
                  </Button>
                }
                className="h-full min-h-[360px]"
              />
            )}
          </section>
        </div>
      </div>

      <Modal
        open={editorMode !== null}
        onClose={closeEditor}
        title={editorMode === "edit" ? "编辑技能" : "新增技能"}
        className="max-w-3xl"
        footer={
          <>
            <Button variant="secondary" onClick={closeEditor} disabled={saving}>
              取消
            </Button>
            <Button
              onClick={() => void saveSkill()}
              disabled={saving}
              loading={saving}
              loadingLabel="保存中…"
            >
              保存技能
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Input
            label="技能名称"
            value={formName}
            onChange={(event) => setFormName(event.target.value)}
            placeholder="例如：organize-downloads"
            disabled={saving}
          />
          <Textarea
            label="SKILL.md 内容"
            value={formContent}
            onChange={(event) => setFormContent(event.target.value)}
            className="min-h-[360px] font-mono text-xs leading-5"
            disabled={saving}
          />
          {formError && (
            <p className="text-sm text-[var(--danger)]" role="alert">
              {formError}
            </p>
          )}
        </div>
      </Modal>

      <ConfirmDialog
        open={Boolean(deleteConfirm)}
        title="删除技能"
        description={`确定删除技能“${deleteConfirm}”及其目录内容吗？此操作不可逆。`}
        confirmLabel="删除"
        tone="danger"
        onConfirm={() => void confirmRemoveSkill()}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  );
}
