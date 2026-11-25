import type { AttachmentReference } from "../types";
import { requestResult } from "../core/api/http";
import type { PageDocument } from "./workspacePages";

interface BaseBlock { id: string; schema_version: 1; available_actions: RichAction[] }
export type AppliedPage = Pick<PageDocument, "id" | "workspace_id" | "revision">;
export type RichAction = "sort" | "select" | "submit" | "acknowledge" | "apply_page_proposal";
export interface FormField { name: string; label: string; choices: string[]; required: boolean }
export type RichBlock = BaseBlock & (
  | { type: "text"; text: string }
  | { type: "code"; language: string; code: string }
  | { type: "math"; expression: string }
  | { type: "image" | "file" | "artifact"; resource: AttachmentReference }
  | { type: "table"; title: string; columns: string[]; rows: string[][] }
  | { type: "chart"; title: string; kind: "bar" | "line" | "scatter"; labels: string[]; values: number[] }
  | { type: "form"; title: string; fields: FormField[] }
  | { type: "interactive_card"; title: string; text: string; action_label: string }
  | { type: "task_status"; title: string; status: string; task_id: string; workspace_id: string }
  | { type: "page_proposal"; page_id: string; revision: number; applied_page?: AppliedPage; changes: ({ block_id?: string | null; markdown: string } | { reference_page_id: string; reference_title: string })[] }
  | { type: "page_create_proposal"; title: string; markdown: string; parent_page_id?: string | null; parent_title?: string; applied_page?: AppliedPage }
  | { type: "unavailable"; message: string }
);

export interface RichComponentState {
  state_version: number;
  state: Record<string, string | number | boolean>;
}

export interface RichMessage {
  parts: Array<{ type: "markdown"; text: string } | { type: "rich"; blocks: RichBlock[] }>;
  states: Record<string, RichComponentState>;
}

export type RichActionResult = RichComponentState | { page: PageDocument };

function messagePath(conversation: string, message: string) {
  return "/api/v1/conversations/" + encodeURIComponent(conversation) + "/messages/" + encodeURIComponent(message);
}

export function readRichMessage(conversation: string, message: string) {
  return requestResult<RichMessage>("GET", messagePath(conversation, message) + "/rich-content");
}

export function postRichAction(conversation: string, message: string, component: string, stateVersion: number, action: RichAction, values?: object) {
  return requestResult<RichActionResult>(
    "POST",
    messagePath(conversation, message) + "/rich-content/" + encodeURIComponent(component) + "/actions",
    { state_version: stateVersion, action, values },
  );
}

export function saveAnswerToPage(conversation: string, message: string, data: {
  workspace_id: string; title?: string; parent_page_id?: string; page_id?: string; revision?: number; image_ids?: string[];
}) {
  return requestResult<PageDocument>("POST", messagePath(conversation, message) + "/save-to-page", data);
}
