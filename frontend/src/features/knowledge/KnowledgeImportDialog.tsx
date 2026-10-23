import { useEffect, useMemo, useState } from "react";
import { Button, Modal } from "../../components/ui";
import { IMPORT_RESULT_PAGE_SIZE, importResultPage, summarizeImport, type ImportProgress } from "./knowledgeImportModel";
import "./knowledgeImport.css";

export default function KnowledgeImportDialog({ open, onClose, importing, progress, onStop }: {
  open: boolean; onClose: () => void; importing: boolean; progress: ImportProgress; onStop: () => void;
}) {
  const [page, setPage] = useState(0);
  const [onlyErrors, setOnlyErrors] = useState(false);
  useEffect(() => { if (!progress.rows.length) { setPage(0); setOnlyErrors(false); } }, [progress.rows.length]);
  const counts = useMemo(() => summarizeImport(progress.rows), [progress.rows]);
  const result = useMemo(() => importResultPage(progress.rows, page, onlyErrors), [progress.rows, page, onlyErrors]);
  const { page: currentPage, pages, rows } = result;
  const completed = progress.rows.length;
  return <Modal open={open} onClose={onClose} title="导入结果" size="lg" footer={<>
    {importing && <Button variant="secondary" onClick={onStop} disabled={progress.stopped}>{progress.stopped ? "正在停止…" : "停止后续导入"}</Button>}
    <Button onClick={onClose}>{importing ? "收起进度" : "关闭"}</Button>
  </>}>
    <div className="knowledge-import-dialog">
      <div className="knowledge-import-progress" role="status" aria-live="polite">
        <strong>{importing ? progress.stopped ? "当前文件处理完成后停止" : "正在导入文档" : progress.stopped ? "导入已停止" : "本次导入完成"}</strong>
        <span>{completed.toLocaleString()} / {progress.total.toLocaleString()} 个文件</span>
      </div>
      <progress aria-label="文档导入进度" value={completed} max={Math.max(progress.total, 1)} />
      {progress.current && <p className="knowledge-import-current" title={progress.current}>{progress.current}</p>}
      <div className="knowledge-import-summary" aria-label="导入汇总">
        <span>新导入 <b>{counts.imported}</b></span><span>已存在 <b>{counts.existing}</b></span><span>失败 <b>{counts.failed}</b></span><span>跳过 <b>{counts.skipped}</b></span>
        {!importing && progress.stopped && <span>未处理 <b>{progress.total - completed}</b></span>}
      </div>
      <label className="knowledge-import-errors"><input type="checkbox" checked={onlyErrors} onChange={(event) => { setOnlyErrors(event.target.checked); setPage(0); }} />只看失败项</label>
      <ul className="knowledge-import-result-list" aria-label="文档导入结果">
        {rows.map((row, index) => <li key={`${currentPage}:${index}`} data-status={row.status}>
          <strong title={row.path}>{row.path}</strong><span>{row.message}</span>
        </li>)}
        {!rows.length && <li>{importing ? "正在处理文件…" : onlyErrors ? "没有失败项" : "本次未处理文件"}</li>}
      </ul>
      <div className="knowledge-import-pagination"><Button size="sm" variant="ghost" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>上一页</Button><span>第 {currentPage + 1} / {pages} 页 · 每页 {IMPORT_RESULT_PAGE_SIZE} 条</span><Button size="sm" variant="ghost" disabled={currentPage + 1 >= pages} onClick={() => setPage(currentPage + 1)}>下一页</Button></div>
      <p className="knowledge-library-hint">同名文件夹会归入同一分类。相同内容只保存一份正文，同时保留各文件夹中的相对路径；收起弹窗不会停止导入。</p>
    </div>
  </Modal>;
}
