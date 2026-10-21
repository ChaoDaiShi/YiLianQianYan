import PageShell from "../components/layout/PageShell";
import KnowledgeLibrary from "../features/knowledge/KnowledgeLibrary";
export default function KnowledgePage(){return <PageShell title="知识库" description="导入外部资料，查看全文并检索知识" variant="workspace" className="knowledge-page page-canvas ui-opt-knowledge" contentClassName="p-5 flex flex-1 min-h-0 flex-col overflow-hidden"><KnowledgeLibrary/></PageShell>;}
