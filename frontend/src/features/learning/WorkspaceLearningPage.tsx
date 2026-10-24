import { Link, useParams } from "react-router-dom";
import { ArrowLeft, BookOpen } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import { EmptyState } from "../../components/ui";
import WorkspaceLearning from "./WorkspaceLearning";

export default function WorkspaceLearningPage() {
  const { id } = useParams<{ id: string }>();
  return <PageShell title={<div className="lc-page-title"><BookOpen size={22} /><h1>学习笔记 / 资料学习</h1></div>} variant="workspace" actions={<Link className="lc-back-link" to={id ? `/workspaces/${encodeURIComponent(id)}` : "/workspaces"}><ArrowLeft size={15} />工作空间</Link>}>
    {id ? <WorkspaceLearning key={id} workspaceId={id} /> : <EmptyState title="请选择工作空间" description="学习记录需要归属于一个工作空间。" />}
  </PageShell>;
}
