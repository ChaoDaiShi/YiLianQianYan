import { useState, useEffect, useMemo } from "react";
import { Brain } from "lucide-react";
import { listSkills, loadSkill } from "../api/client";
import { Input, EmptyState, Spinner, Badge } from "../components/ui";

interface Skill {
  name: string;
  description: string;
  path: string;
}

export default function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [loadedSkill, setLoadedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState("");
  const [search, setSearch] = useState("");

  useEffect(() => {
    listSkills()
      .then((data) => {
        if (Array.isArray(data)) setSkills(data);
        else setError("技能列表为空或后端未响应");
      })
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return skills;
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        (s.description || "").toLowerCase().includes(q)
    );
  }, [skills, search]);

  const openSkill = async (name: string) => {
    setLoadedSkill(name);
    setDetailLoading(true);
    const data = await loadSkill(name);
    if (data?.content) setSkillContent(data.content);
    else setSkillContent("无法加载技能内容。请检查 skills/ 目录与后端连接。");
    setDetailLoading(false);
  };

  return (
    <div className="flex h-full">
      <div className="w-80 border-r border-[var(--border)] flex flex-col bg-[var(--panel)]/40">
        <div className="px-4 py-3 border-b border-[var(--border)] space-y-3">
          <div>
            <h2 className="font-semibold font-display">技能管理</h2>
            <p className="text-xs text-[var(--text-muted)] mt-0.5">AI 可用的专业技能模块</p>
          </div>
          <Input
            placeholder="搜索技能…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="flex-1 overflow-y-auto scrollbar-thin">
          {loading && (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          )}
          {!loading && filtered.length === 0 && (
            <EmptyState
              title={error || "暂无技能"}
              description="在 skills/ 目录下创建 SKILL.md"
              className="py-10"
            />
          )}
          {filtered.map((s) => (
            <button
              key={s.name}
              onClick={() => openSkill(s.name)}
              className={`w-full text-left px-4 py-3 border-b border-[var(--border)]/60 hover:bg-[var(--panel-hover)] transition-colors ${
                loadedSkill === s.name
                  ? "bg-[var(--accent)]/10 border-l-2 border-l-[var(--accent)]"
                  : ""
              }`}
            >
              <div className="font-medium text-sm font-mono">{s.name}</div>
              <div className="text-xs text-[var(--text-muted)] mt-0.5 line-clamp-2">
                {s.description}
              </div>
            </button>
          ))}
        </div>
        <div className="px-4 py-3 border-t border-[var(--border)] text-xs text-[var(--text-faint)]">
          添加：在 skills/ 下创建 SKILL.md
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6 scrollbar-thin">
        {loadedSkill ? (
          <div>
            <div className="flex items-center gap-2 mb-4">
              <Brain className="w-7 h-7 text-[var(--accent)]" />
              <h3 className="text-lg font-semibold font-mono">{loadedSkill}</h3>
              <Badge tone="accent">SKILL.md</Badge>
            </div>
            {detailLoading ? (
              <Spinner />
            ) : (
              <pre className="whitespace-pre-wrap text-sm bg-[var(--panel)] border border-[var(--border)] rounded-xl p-4 font-mono leading-relaxed text-[var(--text-muted)]">
                {skillContent}
              </pre>
            )}
          </div>
        ) : (
          <EmptyState
            icon={<Brain className="w-12 h-12" />}
            title="选择一个技能"
            description="查看 SKILL.md 内容；AI 会在需要时自动调用"
            className="h-full"
          />
        )}
      </div>
    </div>
  );
}
