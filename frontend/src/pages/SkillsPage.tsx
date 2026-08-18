import { useCallback, useEffect, useMemo, useState } from "react";
import { Brain } from "lucide-react";
import { listSkills, loadSkill } from "../api/client";
import { CapabilityDetailSection, CapabilityMetaRow } from "../components/capabilities";
import { Badge, Button, EmptyState, ErrorState, Input, PageHeader, Skeleton } from "../components/ui";

interface Skill {
  name: string;
  description: string;
  path: string;
}

export default function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState("");
  const [detailError, setDetailError] = useState("");
  const [search, setSearch] = useState("");

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await listSkills();
      if (Array.isArray(data)) setSkills(data as Skill[]);
      else setError("技能列表暂时无法加载。");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return skills;
    return skills.filter((skill) =>
      [skill.name, skill.description, skill.path].some((value) => value.toLowerCase().includes(query)),
    );
  }, [skills, search]);

  const selectedSkill = skills.find((skill) => skill.name === selectedName) ?? null;

  const openSkill = async (skill: Skill) => {
    setSelectedName(skill.name);
    setDetailLoading(true);
    setDetailError("");
    try {
      const data = await loadSkill(skill.name);
      if (data && typeof data.content === "string") setSkillContent(data.content);
      else setDetailError("无法加载技能内容。请检查技能目录与后端连接。");
    } catch (cause) {
      setDetailError(String(cause));
    } finally {
      setDetailLoading(false);
    }
  };

  return (
    <div className="capability-page">
      <PageHeader title="技能" description="管理小昔涟可以使用的技能能力。" />

      <div className="capability-page-body">
        <aside className="capability-list-panel" aria-label="技能列表">
          <div className="capability-list-toolbar">
            <Input aria-label="搜索技能" placeholder="搜索技能…" value={search} onChange={(event) => setSearch(event.target.value)} />
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
              <ErrorState title="技能暂时无法加载" description={error} action={<Button variant="secondary" size="sm" onClick={() => void reload()}>重试</Button>} className="m-3" />
            ) : filtered.length === 0 ? (
              <EmptyState title="这里还没有技能" description="可用的技能会显示在这里。" className="py-14" />
            ) : (
              <div className="capability-row-stack">
                {filtered.map((skill) => (
                  <button
                    key={skill.name}
                    type="button"
                    aria-selected={selectedName === skill.name}
                    onClick={() => void openSkill(skill)}
                    className={`capability-list-row ${selectedName === skill.name ? "capability-list-row-selected" : ""}`}
                  >
                    <span className="capability-list-row-heading">
                      <Brain className="h-4 w-4 shrink-0 text-[var(--accent-purple)]" />
                      <strong>{skill.name}</strong>
                    </span>
                    <span className="capability-list-row-description">{skill.description || "暂无描述"}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <p className="capability-list-footnote">技能内容来自项目的 skills 目录。</p>
        </aside>

        <section className="capability-detail-panel" aria-label="技能详情">
          {selectedSkill ? (
            <div className="capability-detail-content">
              <div className="capability-detail-heading">
                <div>
                  <div className="capability-detail-title-line">
                    <Brain className="h-5 w-5 text-[var(--accent-primary)]" />
                    <h2>{selectedSkill.name}</h2>
                    <Badge tone="accent">SKILL.md</Badge>
                  </div>
                  <p className="capability-detail-description">{selectedSkill.description || "暂无描述"}</p>
                </div>
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
                <dl className="capability-meta-list">
                  <CapabilityMetaRow label="名称" value={selectedSkill.name} mono />
                  <CapabilityMetaRow label="路径" value={selectedSkill.path} mono />
                </dl>
              </CapabilityDetailSection>
            </div>
          ) : (
            <EmptyState icon={<Brain className="h-7 w-7" />} title="选择一个技能" description="查看真实的 SKILL.md 内容。" className="h-full min-h-[360px]" />
          )}
        </section>
      </div>
    </div>
  );
}
