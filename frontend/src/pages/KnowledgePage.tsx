import { ArrowRight, BookOpen, Database, FileQuestion } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button, EmptyState, PageHeader } from "../components/ui";

export default function KnowledgePage() {
  const navigate = useNavigate();

  return (
    <div className="knowledge-page">
      <PageHeader
        title="知识库"
        description="管理可供小昔涟查阅的外部资料"
      />

      <div className="knowledge-page-body scrollbar-thin">
        <section className="knowledge-capability-panel" aria-labelledby="knowledge-capability-title">
          <EmptyState
            icon={<BookOpen className="h-7 w-7" />}
            title="当前还没有独立知识库数据源"
            description="当前版本只提供长期记忆记录；外部文档、文件预览和检索索引尚未接入。"
          />

          <div className="knowledge-capability-grid" aria-label="当前知识库能力">
            <div>
              <Database className="h-4 w-4" aria-hidden="true" />
              <strong>已有能力</strong>
              <span>长期记忆记录</span>
            </div>
            <div>
              <FileQuestion className="h-4 w-4" aria-hidden="true" />
              <strong>待接入</strong>
              <span>文档、文件与独立索引</span>
            </div>
          </div>

          <div className="knowledge-capability-action">
            <p>不在数据源尚未存在时展示虚构的文档、摘要或状态。</p>
            <Button variant="secondary" onClick={() => navigate("/memory")}>
              前往记忆中心
              <ArrowRight className="h-4 w-4" />
            </Button>
          </div>
        </section>
      </div>
    </div>
  );
}
