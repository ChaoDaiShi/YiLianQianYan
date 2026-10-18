import { ArrowRight, BookOpen, Database, FileText } from "lucide-react";
import { useNavigate } from "react-router-dom";
import PageShell from "../components/layout/PageShell";
import { Button } from "../components/ui";

export default function KnowledgePage() {
  const navigate = useNavigate();

  return (
    <PageShell
      title="知识库"
      description="说明当前版本能力边界与外部资料接入规划"
      className="knowledge-page page-canvas ui-opt-knowledge"
    >
      <div className="knowledge-page-body scrollbar-thin">
        <div className="knowledge-explanation-container">
          <div className="knowledge-notice-banner" role="status">
            <BookOpen className="h-5 w-5 shrink-0 text-[var(--accent-primary)]" />
            <div>
              <h2 className="text-sm font-semibold text-[var(--text-primary)]">
                当前还没有独立知识库数据源
              </h2>
              <p className="mt-1 text-xs text-[var(--text-secondary)] leading-relaxed">
                当前版本提供长期记忆记录；外部文档、文件预览和独立检索索引尚未接入。
              </p>
            </div>
          </div>

          <div className="knowledge-dual-grid" aria-label="知识库能力划分">
            {/* Partition 1: Currently Available */}
            <section className="knowledge-partition knowledge-partition-available" aria-labelledby="available-heading">
              <div className="knowledge-partition-header">
                <Database className="h-4 w-4 text-[var(--success)]" aria-hidden="true" />
                <h3 id="available-heading">当前可用：长期记忆记录</h3>
                <span className="knowledge-badge-available">已就绪</span>
              </div>
              <p className="knowledge-partition-desc">
                支持事实、偏好、笔记等核心长期记忆记录的读取、手动维护与关键词检索。
              </p>
              <ul className="knowledge-feature-list">
                <li>结构化记忆分类与来源追踪（AI 提取 / 手动录入）</li>
                <li>记忆内容编辑、删除与详情查阅</li>
                <li>对话中关键事实与偏好持久化</li>
              </ul>
              <div className="knowledge-partition-footer">
                <Button onClick={() => navigate("/memory")}>
                  前往记忆中心
                  <ArrowRight className="h-4 w-4" />
                </Button>
              </div>
            </section>

            {/* Partition 2: Not Yet Connected */}
            <section className="knowledge-partition knowledge-partition-pending" aria-labelledby="pending-heading">
              <div className="knowledge-partition-header">
                <FileText className="h-4 w-4 text-[var(--text-faint)]" aria-hidden="true" />
                <h3 id="pending-heading">尚未接入：独立文档知识库</h3>
                <span className="knowledge-badge-pending">尚未接入</span>
              </div>
              <p className="knowledge-partition-desc">
                外部文档导入、文件内容预览和独立检索索引属于未来规划，尚未接入本桌面系统。
              </p>
              <ul className="knowledge-feature-list knowledge-feature-list-pending">
                <li>外部本地文档导入与解析（尚未接入）</li>
                <li>文件预览与分块切片能力（尚未接入）</li>
                <li>独立外部知识库检索索引（尚未接入）</li>
              </ul>
              <div className="knowledge-partition-footer">
                <p className="text-xs text-[var(--text-faint)]">
                  系统不在数据源尚未存在时展示虚构的文件、数量或虚假索引状态。
                </p>
              </div>
            </section>
          </div>
        </div>
      </div>
    </PageShell>
  );
}
