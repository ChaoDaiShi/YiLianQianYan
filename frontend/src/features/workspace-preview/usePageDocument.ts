import { useCallback, useEffect, useRef, useState } from "react";
import { loadPage, updatePageBlocks, type BlockOperation, type PageDocument } from "../../api/workspacePages";
import type { ApiResult } from "../../core/api/http";

export default function usePageDocument(id: string | null, workspace: string) {
  const [document, setDocument] = useState<PageDocument | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<"idle" | "loading" | "edited" | "saving" | "saved" | "conflict" | "error">("idle");
  const [error, setError] = useState("");
  const current = useRef<PageDocument | null>(null);
  const local = useRef<Record<string, string>>({});
  const flight = useRef<Promise<boolean> | null>(null);
  const delay = useRef<number | undefined>(undefined);

  const accept = useCallback((value: PageDocument | null) => {
    current.current = value;
    setDocument(value);
  }, []);

  const reload = useCallback(async (pageId: string, applies: () => boolean = () => true) => {
    const result = await loadPage(pageId, true);
    if (!applies()) return;
    if (!result.ok || result.data.workspace_id !== workspace) {
      setStatus("error");
      setError(result.ok ? "页面不属于当前工作空间。" : result.error);
      return;
    }
    local.current = {};
    setDrafts({});
    setError("");
    setStatus("idle");
    accept(result.data);
  }, [accept, workspace]);

  const save = useCallback(async (): Promise<boolean> => {
    window.clearTimeout(delay.current);
    if (flight.current && !(await flight.current)) return false;
    if (flight.current) return save();
    const page = current.current;
    if (!page) return true;
    const changes = Object.entries(local.current);
    if (!changes.length) return true;
    const operations: BlockOperation[] = changes.map(([blockId, text]) => ({ op: "replace", block: { id: blockId, type: "markdown", text } }));
    setStatus("saving"); setError("");
    const write = updatePageBlocks(page.id, page.revision, operations).then(result => {
      if (current.current?.id !== page.id) return result.ok;
      if (!result.ok) {
        setStatus(result.status === 409 ? "conflict" : "error");
        setError(result.error);
        return false;
      }
      const pending = { ...local.current };
      changes.forEach(([blockId, text]) => { if (pending[blockId] === text) delete pending[blockId]; });
      local.current = pending;
      setDrafts(pending);
      accept(result.data);
      setStatus(Object.keys(pending).length ? "edited" : "saved");
      return true;
    }).finally(() => { flight.current = null; });
    flight.current = write;
    const succeeded = await write;
    return succeeded && (Object.keys(local.current).length === 0 || save());
  }, [accept]);

  useEffect(() => {
    let active = true;
    accept(null);
    local.current = {};
    setDrafts({});
    setStatus(id ? "loading" : "idle");
    setError("");
    if (id) void reload(id, () => active);
    return () => {
      active = false;
      window.clearTimeout(delay.current);
      if (Object.keys(local.current).length) void save();
    };
  }, [accept, id, reload, save]);

  const edit = (blockId: string, value: string) => {
    local.current = { ...local.current, [blockId]: value };
    setDrafts(local.current);
    setStatus("edited"); setError("");
    window.clearTimeout(delay.current);
    delay.current = window.setTimeout(() => void save(), 550);
  };

  const mutate = async (operation: (page: PageDocument) => Promise<ApiResult<PageDocument>>): Promise<PageDocument | undefined> => {
    if (!(await save()) || !current.current) return;
    const id = current.current.id;
    setStatus("saving"); setError("");
    const result = await operation(current.current);
    if (current.current?.id !== id) return;
    if (result.ok) {
      accept(result.data);
      setStatus("saved");
      return result.data;
    }
    setStatus(result.status === 409 ? "conflict" : "error");
    setError(result.error);
  };

  return { document, current, drafts, status, error, edit, save, mutate, reload };
}
