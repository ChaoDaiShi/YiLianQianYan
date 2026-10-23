import { useEffect, useRef, useState } from "react";
import { ensureKnowledgeCollection, importKnowledge, type KnowledgeCollection } from "../../api/knowledge";
import type { ImportNotice, ImportProgress } from "./knowledgeImportModel";

export const KNOWLEDGE_FILE_FORMATS = ".txt,.md,.markdown,.csv,.tsv,.json,.yaml,.yml,.pdf,.docx,.pptx,.xlsx,.rs,.ts,.tsx,.js,.py,.html,.css,.sql,.log,.toml,.xml";
export function useKnowledgeImport(onComplete: () => Promise<void>) {
  const [importing, setImporting] = useState(false);
  const [open, setOpen] = useState(false);
  const [progress, setProgress] = useState<ImportProgress>({ total: 0, current: "", rows: [], stopped: false });
  const active = useRef(true);
  const busy = useRef(false);
  const stop = useRef(false);
  const refresh = useRef(onComplete);
  refresh.current = onComplete;

  useEffect(() => {
    active.current = true;
    return () => { active.current = false; stop.current = true; };
  }, []);

  async function addFiles(files: File[]) {
    if (busy.current || !files.length) return;
    busy.current = true;
    stop.current = false;
    setImporting(true);
    setOpen(true);
    setProgress({ total: files.length, current: "", rows: [], stopped: false });
    const folders = new Map<string, Promise<{ collection: KnowledgeCollection }>>();
    const results: ImportNotice[] = [];
    for (const file of files) {
      if (stop.current || !active.current) break;
      const path = file.webkitRelativePath || file.name;
      setProgress((value) => ({ ...value, current: path }));
      let row: ImportNotice;
      if (!KNOWLEDGE_FILE_FORMATS.split(",").some((extension) => file.name.toLowerCase().endsWith(extension))) {
        row = { name: file.name, path, status: "skipped", message: "文件类型不支持，未上传" };
      } else {
        try {
          let location;
          if (file.webkitRelativePath && file.webkitRelativePath.includes("/")) {
            const folder = file.webkitRelativePath.split("/")[0];
            let requested = folders.get(folder);
            if (!requested) {
              requested = ensureKnowledgeCollection(folder);
              folders.set(folder, requested);
            }
            const { collection } = await requested;
            location = { collection_id: collection.id, relative_path: file.webkitRelativePath };
          }
          const result = await importKnowledge(file, location);
          row = { name: file.name, path, status: result.created ? "imported" : "existing", message: result.created ? `已导入 · ${result.document.chunk_count} 个知识片段` : location ? "文档已存在，已保存当前文件夹归属" : "文档已在知识库中" };
        } catch (cause) {
          row = { name: file.name, path, status: "failed", message: cause instanceof Error ? cause.message : String(cause) };
        }
      }
      results.push(row);
      if (active.current) setProgress((value) => ({ ...value, rows: [...results] }));
    }
    busy.current = false;
    if (active.current) {
      setImporting(false);
      setProgress((value) => ({ ...value, current: "", stopped: stop.current && results.length < files.length }));
      await refresh.current();
    }
  }

  return { importing, open, setOpen, progress, addFiles, requestStop: () => { stop.current = true; setProgress((value) => ({ ...value, stopped: true })); } };
}
