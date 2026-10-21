import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import { listPluginFeatures, type PluginFeatureEntry } from "../../api/plugins";
import { Button, ErrorState, EmptyState, Modal, Skeleton } from "../../components/ui";
import { startWorkflowRun } from "../../api/workflows";
import WorkflowRunInspector from "../../components/workflow/WorkflowRunInspector";
import PageShell from "../../components/layout/PageShell";
export function usePluginFeatures() {
  const [features, setFeatures] = useState<PluginFeatureEntry[]>([]); const [loading, setLoading] = useState(true); const [error, setError] = useState("");
  const reload = useCallback(async () => { setLoading(true); setError(""); try { setFeatures(await listPluginFeatures()); } catch (cause) { setFeatures([]); setError(String(cause)); } finally { setLoading(false); } }, []);
  useEffect(() => { void reload(); const update = () => void reload(); window.addEventListener("plugin-features-changed", update); window.addEventListener("focus", update); return () => { window.removeEventListener("plugin-features-changed", update); window.removeEventListener("focus", update); }; }, [reload]);
  return { features, loading, error, reload };
}
export default function PluginFeaturePage({ pluginId, featureId }: { pluginId?: string; featureId?: string } = {}) {
  const params = useParams(); const navigate = useNavigate(); const { features, loading, error, reload } = usePluginFeatures(); const [full, setFull] = useState(false); const [runId, setRunId] = useState<string | null>(null); const [actionError, setActionError] = useState(""); const [running, setRunning] = useState(false);
  const feature = features.find(item => item.plugin_id === (pluginId ?? params.pluginId) && item.id === (featureId ?? params.featureId));
  async function run() { if (!feature?.resource_id) return; setRunning(true); setActionError(""); try { const result = await startWorkflowRun(feature.resource_id); if (!result.ok) throw new Error(result.error); setRunId(result.data.run_id); } catch (cause) { setActionError(String(cause)); } finally { setRunning(false); } }
  return <PageShell title={feature?.title ?? "插件功能"} description={feature?.plugin_name}>
    {loading ? <Skeleton className="h-40" /> : error ? <ErrorState title="插件功能无法加载" description={error} action={<Button onClick={() => void reload()}>重试</Button>} /> : !feature ? <EmptyState title="功能当前不可用" description="所属插件可能已停用、卸载或移除了此功能。" /> : <article className="mx-auto max-w-3xl space-y-5 p-6"><p>{feature.description}</p><div className="prose text-[var(--text)]"><ReactMarkdown>{feature.content.slice(0, 1600)}</ReactMarkdown></div><div className="flex gap-2"><Button variant="secondary" onClick={() => setFull(true)}>更多 · 查看完整内容</Button>{feature.target.kind === "workflow" ? <Button disabled={!feature.resource_id || running} onClick={() => void run()}>运行已安装工作流</Button> : feature.target.kind === "agent" ? <Button disabled={!feature.resource_id} onClick={() => navigate(`/agents?agent=${encodeURIComponent(feature.resource_id!)}`)}>配置已安装智能体</Button> : <Button onClick={() => navigate("/chat", { state: { suggestedText: `使用插件「${feature.plugin_name}」的「${feature.title}」完成以下任务。请遵循这份已安装的${feature.target.kind}贡献：\n\n${feature.content}\n\n我的任务：` } })}>用于对话</Button>}</div>{actionError && <ErrorState title="功能执行未完成" description={actionError} />}{feature.target.kind !== "skill" && !feature.resource_id && <p>贡献引用的资源当前未安装，请先在能力管理中完成安装。</p>}{runId && <WorkflowRunInspector runId={runId} onClose={() => setRunId(null)} />}<p className="text-xs text-[var(--text-muted)]">贡献：{feature.target.contribution} · {feature.target.kind}</p></article>}
    <Modal open={full} onClose={() => setFull(false)} title={feature?.title ?? "完整内容"} size="xl"><pre className="whitespace-pre-wrap break-words">{feature?.content}</pre></Modal>
  </PageShell>;
}
