import WorkspaceIcon from "../components/ui/WorkspaceIcon";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, Copy, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { refreshSkillDiscovery } from "../api/skillDiscovery";
import { useNavigate, useSearchParams } from "react-router-dom";
import "../features/skills/skillsWorkspace.css";
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
  Skeleton,
  Textarea,
} from "../components/ui";
import ManagementFrame from "../features/capabilities/ManagementFrame";
import { validateSkillDraft } from "../features/skills/skillForm";

type CatalogSkill = SkillSummary & { is_bundled?: boolean; root_dir: string };

type EditorMode = "create" | "edit" | null;

export default function SkillsPage({ embedded = false, initialSkill }: { embedded?: boolean; initialSkill?: string } = {}) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const [skills, setSkills] = useState<CatalogSkill[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [fullTextOpen, setFullTextOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState("");
  const [detailError, setDetailError] = useState("");
  const [search, setSearch] = useState("");
  const [sourceFilter, setSourceFilter] = useState("bundled");
  const [editorMode, setEditorMode] = useState<EditorMode>(null);
  const [formName, setFormName] = useState("");
  const [formContent, setFormContent] = useState("");
  const [formError, setFormError] = useState("");
  const [saving, setSaving] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const requestIdRef = useRef(0);
  const requestedSkillRef = useRef<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    const result = await listSkills();
    if (result.ok) setSkills(result.data as CatalogSkill[]);
    else setError(result.error || "技能列表暂时无法加载。");
    setLoading(false);
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const refreshDiscovery = async () => {
    setRefreshing(true);
    const result = await refreshSkillDiscovery();
    if (result.ok) await reload(); else setError(result.error);
    setRefreshing(false);
  };

  const openSkill = useCallback(async (skill: SkillSummary) => {
    setSelectedName(skill.name);
    requestedSkillRef.current = skill.name;
    setSearchParams(current=>{const next=new URLSearchParams(current);next.set("skill",skill.name);return next;},{replace:true});
    setFullTextOpen(false);
    setDetailLoading(true);
    setDetailError("");
    const reqId = ++requestIdRef.current;
    const result = await loadSkill(skill.name);
    if (requestIdRef.current !== reqId) return;
    if (result.ok) setSkillContent(result.data.content);
    else setDetailError(result.error || "无法加载技能内容。请检查技能目录与后端连接。");
    setDetailLoading(false);
  }, [setSearchParams]);

  useEffect(() => {
    const requested = initialSkill ?? searchParams.get("skill");
    if (!requested) { requestedSkillRef.current = null; return; }
    if (requestedSkillRef.current === requested) return;
    const skill = skills.find((item) => item.name === requested);
    if (skill) {
      requestedSkillRef.current = requested;
      setSearch(""); setSourceFilter("all");
      void openSkill(skill);
    }
  }, [initialSkill, openSkill, searchParams, skills]);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    const visible = skills.filter(skill => sourceFilter === "all" || (sourceFilter === "bundled" ? skill.is_bundled === true : sourceFilter === "other" ? !skill.is_bundled : (skill.source ?? "未标注来源") === sourceFilter));
    if (!query) return visible;
    return visible.filter((skill) =>
      [skill.name, skill.description, skill.path, skill.source ?? "", skill.display_name ?? ""].some((value) =>
        value.toLowerCase().includes(query)
      )
    );
  }, [skills, search, sourceFilter]);

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
    setSourceFilter("all");
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
    <ManagementFrame embedded={embedded} variant="workspace"
      title="技能"
      description="管理小涟可以使用的技能能力。"
      className="capability-page skills-page page-canvas ui-opt-capabilities ui-opt-skills"
      contentClassName="p-0 flex flex-col flex-1 min-h-0 overflow-hidden"
    >
      <div className="capability-page-body capability-page-body-split skills-page-body">
        <div className="capability-workbench-toolbar skills-workbench-toolbar"><Input aria-label="搜索技能" placeholder="搜索技能名称或说明…" value={search} onChange={event=>setSearch(event.target.value)}/><select aria-label="技能来源筛选" className="settings-select" value={sourceFilter} onChange={event=>setSourceFilter(event.target.value)}><option value="bundled">官方内置</option><option value="other">其他来源</option><option value="all">全部来源</option>{[...new Set(skills.filter(skill=>!skill.is_bundled).map(skill=>skill.source??"未标注来源"))].map(source=><option key={source} value={source}>{source}</option>)}</select><span className="capability-list-caption">{filtered.length} 个技能</span><div className="skills-workbench-actions"><Button size="sm" variant="secondary" onClick={() => void refreshDiscovery()} disabled={refreshing}><RefreshCw className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`} />{refreshing ? "正在发现" : "刷新发现"}</Button><Button size="sm" variant="secondary" onClick={() => navigate("/capabilities?category=import&import_tab=discovery")}>发现与导入</Button><Button size="sm" onClick={openCreate}><Plus className="h-3.5 w-3.5" />新增技能</Button></div></div>
        <div className="capability-split-layout skills-split-layout">
          <aside className="capability-list-panel" aria-label="技能列表">
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
                  title={search.trim() ? "没有匹配的技能" : "当前来源没有技能"}
                  description={search.trim() ? `未找到与“${search}”匹配的技能名称或路径。` : "切换来源查看其他技能，或使用发现与导入。"}
                  action={
                    search.trim() ? <Button size="sm" variant="secondary" onClick={() => setSearch("")}>
                      清除搜索
                    </Button> : undefined
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
                          <WorkspaceIcon name="skill" size={20} className="shrink-0 text-[var(--accent-purple)]" />
                          <strong>{skill.display_name ?? skill.name}</strong>
                          {skill.compatibility && <Badge tone={skill.compatibility.runtime_ready ? "success" : "warning"}>{skill.compatibility.status === "external_host" ? "需外部宿主" : skill.compatibility.status === "adapted" ? "已适配工具" : "可用流程"}</Badge>}
                          {skill.editable ? (
                            <Badge tone="accent">可编辑</Badge>
                          ) : (
                            <Badge tone="default">只读</Badge>
                          )}
                        </span>
                        <span className="capability-list-row-description">
                          {skill.description || "暂无描述"}
                        </span>
                        {skill.source && <span className="text-[11px] text-[var(--text-muted)] block">来源：{skill.source}</span>}

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
                      <WorkspaceIcon name="skill" size={20} className="text-[var(--accent-primary)]" />
                      <h2>{selectedSkill.display_name ?? selectedSkill.name}</h2>
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
                    {selectedSkill.source && <p className="text-xs text-[var(--text-muted)] mt-2">来源：{selectedSkill.source}</p>}
                    {selectedSkill.compatibility && <div className="mt-3 rounded-xl border border-[var(--border-soft)] p-3 text-xs leading-relaxed" role="status"><strong>{selectedSkill.compatibility.status === "external_host" ? "需要外部宿主能力" : selectedSkill.compatibility.status === "adapted" ? "按本项目工具适配执行" : "本项目可加载的技能流程"}</strong><p className="mt-1">{selectedSkill.compatibility.reason}</p>{selectedSkill.compatibility.missing_tools.length > 0 && <p className="mt-2 break-all">缺少工具：{selectedSkill.compatibility.missing_tools.join("、")}</p>}{selectedSkill.compatibility.tool_mappings.map(mapping => <p key={mapping.from} className="mt-2"><code>{mapping.from} → {mapping.to}</code>：{mapping.instructions}</p>)}</div>}
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    {!selectedSkill.is_bundled && !selectedSkill.editable && <Button size="sm" variant="secondary" onClick={() => navigate(`/capabilities?${new URLSearchParams({category:"import",import_tab:"packages",import_directory:selectedSkill.root_dir,import_name:selectedSkill.display_name ?? selectedSkill.name})}`)}>检查并导入</Button>}
                    {selectedSkill.compatibility?.status === "external_host" && <Button size="sm" variant="secondary" disabled={detailLoading || Boolean(detailError)} onClick={() => {setEditorMode("create");setFormName(`adapted-${selectedSkill.name.split(":").pop()}`);setFormContent(skillContent);setFormError("请将原文的宿主调用改为本项目可用工具，确认脚本与引用的实际路径；直接保存原文不会解除宿主依赖。");}}>适配为工作区技能</Button>}
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
                      <Button size="sm" variant="ghost" onClick={() => setFullTextOpen(value=>!value)}>{fullTextOpen?"收起全文":"展开完整使用说明"}</Button>
                      <pre className="skill-reading-content leading-[1.6]">
                        {(fullTextOpen?skillContent:skillContent.slice(0,1100)) || "（内容为空）"}
                      </pre>
                    </div>
                  )}
                </CapabilityDetailSection>

                <CapabilityDetailSection title="技术详情" technical>
                  <dl className="capability-meta-list">
                    {selectedSkill.display_name && <CapabilityMetaRow label="技能原名" value={selectedSkill.display_name} />}
                    {selectedSkill.source_root && <CapabilityMetaRow label="发现目录" value={selectedSkill.source_root} mono />}
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
                icon={<WorkspaceIcon name="skill" size={28} />}
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

    </ManagementFrame>
  );
}
