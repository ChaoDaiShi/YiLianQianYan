import { useParams } from "react-router-dom";
import PageShell from "../../components/layout/PageShell";
import { EmptyState } from "../../components/ui";
import WorkspaceLearning from "./WorkspaceLearning";
import WorkspaceSectionShell from "../workspace-preview/WorkspaceSectionShell";

export default function WorkspaceLearningPage() {
  const { id } = useParams<{ id: string }>();
  return id ? <WorkspaceSectionShell key={id} workspaceId={id} active="learning">{workspace => <WorkspaceLearning workspaceId={id} readOnly={workspace.status === "archived"} />}</WorkspaceSectionShell>
    : <PageShell title="资料学习"><EmptyState title="请选择工作空间" description="学习记录需要归属于一个工作空间。" /></PageShell>;
}
