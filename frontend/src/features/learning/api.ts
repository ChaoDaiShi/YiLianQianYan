import { requestResult } from "../../core/api/http";
export interface LearningSource { document_id: string; resource_id: string; name: string; hash: string }
export interface Citation { document_id: string; quote: string }
export interface LearningSession { id: string; workspace_id: string; title: string; status: "generating" | "ready" | "failed" | "cancelled"; sources: LearningSource[]; revision: number; error: string | null; created_at: number; updated_at: number }
export interface LearningExercise { id: string; resource_id: string; question: string; options: Array<{ id: string; label: string }>; sources: Citation[] }
export interface LearningAnswer { id: string; exercise_id: string; option_id: string; correct: boolean; score: number; grading_method: "deterministic_option_match"; correct_option_id: string; explanation: string; created_at: number }
export interface LearningNote { revision: number; resource_id: string; parent_resource_id: string | null; created_at: number }
export interface LearningDetail { session: LearningSession; content: string | null; notes: LearningNote[]; exercises: LearningExercise[]; answers: LearningAnswer[] }
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, path, body);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
const base = (workspace: string) => `/api/workspaces/${encodeURIComponent(workspace)}/learning`;
export const listLearning = (workspace: string) => call<{ sessions: LearningSession[] }>("GET", base(workspace));
export const createLearning = (workspace: string, title: string, document_ids: string[]) => call<{ session: LearningSession }>("POST", base(workspace), { title, document_ids });
export const getLearning = (workspace: string, session: string) => call<LearningDetail>("GET", `${base(workspace)}/${encodeURIComponent(session)}`);
export const saveLearningNote = (workspace: string, session: string, content: string, expected_revision: number) => call<{ session: LearningSession }>("PUT", `${base(workspace)}/${encodeURIComponent(session)}/notes`, { content, expected_revision });
export const getLearningNote = (workspace: string, session: string, revision: number) => call<{ content: string; note: LearningNote }>("GET", `${base(workspace)}/${encodeURIComponent(session)}/notes/${revision}`);
export const answerLearning = (workspace: string, session: string, exercise: string, option_id: string, request_id: string) => call<{ answer: LearningAnswer }>("POST", `${base(workspace)}/${encodeURIComponent(session)}/exercises/${encodeURIComponent(exercise)}/answers`, { option_id, request_id });

export const cancelLearning = (workspace: string, session: string) => call<{ cancelled: boolean }>("POST", `${base(workspace)}/${encodeURIComponent(session)}/cancel`, {});
