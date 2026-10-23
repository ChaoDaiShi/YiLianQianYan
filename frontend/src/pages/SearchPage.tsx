import { FormEvent, useEffect, useState } from "react";
import { BookOpenCheck, ExternalLink, ListTodo, MessageSquareText, Save, Search, Settings2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { getSearchConfig, runSearch, updateSearchConfig, type SearchConfig, type SearchResponse, type SearchResult } from "../api/cloud";
import { createMemory } from "../api/memory";
import { createTask } from "../api/tasks";
import { createWorkspace, listWorkspaces } from "../api/workspaces";
import { openExternalUrl } from "../components/chat/externalLink";
import { Badge, Button, EmptyState, Input } from "../components/ui";
import PageShell from "../components/layout/PageShell";
import { buildSearchChatPrompt, buildSearchKnowledgeRecord, buildSearchTaskDraft } from "../features/cloud/cloudPresentation";

export default function SearchPage() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [zone, setZone] = useState<"cn" | "intl">("cn");
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [config, setConfig] = useState<SearchConfig | null>(null);
  const [configDraft, setConfigDraft] = useState({ base_url: "https://api.anysearch.com", api_key: "" });
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  useEffect(() => { void getSearchConfig().then((value) => { if (value) { setConfig(value); setConfigDraft((current) => ({ ...current, base_url: value.base_url })); } }); }, []);

  const search = async (event: FormEvent) => {
    event.preventDefault();
    if (!query.trim()) return;
    setLoading(true); setError(""); setNotice("");
    const result = await runSearch({ query: query.trim(), max_results: 10, zone, language: "zh-CN" });
    if (result.ok) setResponse(result.data); else setError(result.error);
    setLoading(false);
  };

  const saveConfig = async () => {
    setSaving(true); setError("");
    const result = await updateSearchConfig({ base_url: configDraft.base_url, api_key: configDraft.api_key || undefined });
    if (result.ok) { setConfig(result.data); setConfigDraft((current) => ({ ...current, api_key: "" })); setNotice("搜索配置已保存到本机。"); }
    else setError(result.error);
    setSaving(false);
  };

  const sendToChat = (result: SearchResult) => navigate("/chat", { state: { suggestedText: buildSearchChatPrompt(result) } });
  const addTask = async (result: SearchResult) => {
    setNotice(""); setError("");
    const workspaces = await listWorkspaces();
    if (!workspaces.ok) { setError(workspaces.error); return; }
    let workspace = workspaces.data[0];
    if (!workspace) {
      const created = await createWorkspace({ name: "搜索研究", description: "由 AI Search 保存的研究任务" });
      if (!created.ok) { setError(created.error); return; }
      workspace = created.data;
    }
    const task = await createTask({ workspace_id: workspace.id, ...buildSearchTaskDraft(result) });
    if (!task.ok) { setError(task.error); return; }
    setNotice(`已加入任务：${task.data.title}`);
  };
  const saveKnowledge = async (result: SearchResult) => {
    try {
      const saved = await createMemory(buildSearchKnowledgeRecord(result));
      if (saved) setNotice(`已保存到知识记忆：${result.title}`); else setError("保存知识失败，请稍后重试。");
    } catch (e) {
      setError(e instanceof Error ? e.message : "保存知识失败，请稍后重试。");
    }
  };

  return (
    <PageShell
      variant="workspace"
      title="AI 搜索"
      description="统一检索入口。AnySearch 是首个 Provider，凭据仍保存在本机。"
      className="cloud-page search-page page-canvas flex h-full min-h-0 flex-col"
      contentClassName="p-0 flex flex-col flex-1 min-h-0 overflow-hidden"
      actions={<Badge tone={config?.configured ? "success" : "default"}>{config?.configured ? "本机 BYOK" : "匿名模式"}</Badge>}
    >
    <div className="search-page-body scrollbar-thin">
      <form className="search-command" onSubmit={search}>
        <Search aria-hidden="true" />
        <input aria-label="搜索问题" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索资料、文档、事实或当前信息…" />
        <select aria-label="搜索区域" value={zone} onChange={(event) => setZone(event.target.value as "cn" | "intl")}><option value="cn">中国区</option><option value="intl">国际区</option></select>
        <Button type="submit" disabled={loading || !query.trim()}>{loading ? "搜索中…" : "搜索"}</Button>
      </form>
      <details className="search-config cloud-card"><summary><Settings2 />搜索服务配置</summary><div className="search-config-grid"><Input label="AnySearch API 地址" value={configDraft.base_url} onChange={(event) => setConfigDraft({ ...configDraft, base_url: event.target.value })} /><Input label="API Key（可选）" type="password" value={configDraft.api_key} onChange={(event) => setConfigDraft({ ...configDraft, api_key: event.target.value })} hint={config?.configured ? "已配置本机密钥；留空表示保留。" : "留空使用匿名额度。"} /></div><Button size="sm" variant="secondary" disabled={saving} onClick={() => void saveConfig()}><Save />{saving ? "保存中…" : "保存配置"}</Button></details>
      {error ? <p className="cloud-error" role="alert">{error}</p> : null}
      {notice ? <p className="cloud-notice" role="status">{notice}</p> : null}
      {!response && !loading ? <EmptyState icon={<Search />} title="从一个问题开始" description="结果会显示标题、摘要和来源，并可以继续交给小涟、任务或知识流程。" /> : null}
      {response ? <section className="search-results" aria-label="搜索结果"><div className="search-results-meta"><span>{response.total_results} 条结果 · {response.search_time_ms} ms</span><code>{response.request_id}</code></div>{response.results.map((result, index) => <article className="search-result-card" key={`${result.url}-${index}`}><div className="search-result-index">{String(index + 1).padStart(2, "0")}</div><div className="search-result-content"><h2>{result.title || result.url}</h2><p>{result.snippet || result.content || "该来源没有提供摘要。"}</p><button type="button" className="search-source" onClick={() => void openExternalUrl(result.url)}>{result.url}<ExternalLink /></button><div className="search-result-actions"><Button size="sm" onClick={() => sendToChat(result)}><MessageSquareText />让小涟总结</Button><Button size="sm" variant="secondary" onClick={() => void addTask(result)}><ListTodo />加入任务</Button><Button size="sm" variant="secondary" onClick={() => void saveKnowledge(result)}><BookOpenCheck />保存为知识</Button></div></div></article>)}</section> : null}
      </div>
    </PageShell>
  );
}
