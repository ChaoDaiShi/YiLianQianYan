import type { MemoryRecord } from "../../api/memory";

export interface MemoryRelationship {
  kind: RelationshipKind;
  subject: string;
  label: string;
  object: string;
  directed: boolean;
  certainty: "confirmed" | "inferred";
  occurredAt: string;
}

export const relationshipKinds = { person: "人物关系", kinship: "亲属关系", event: "事件链", project: "项目关系", concept: "概念关系", preference: "偏好关系" } as const;
export type RelationshipKind = keyof typeof relationshipKinds;

export const emptyRelationship: MemoryRelationship = {
  kind: "person", subject: "", label: "", object: "", directed: false, certainty: "confirmed", occurredAt: "",
};

const relationshipText = ({ subject, label, object, occurredAt, certainty }: MemoryRelationship) =>
  `${subject} · ${label} · ${object}${occurredAt ? ` · ${occurredAt}` : ""}${certainty === "inferred" ? " · 待确认" : ""}`;

export function canSaveMemoryRelationship(relation: MemoryRelationship): boolean {
  return Boolean(relation.subject.trim() && relation.label.trim() && relation.object.trim());
}

export function readMemoryRelationship(memory: MemoryRecord): MemoryRelationship | null {
  if (!memory.metadata) return null;
  try {
    const relation = JSON.parse(memory.metadata)?.relationship;
    const label = relation?.label ?? relation?.action;
    if (typeof relation?.subject !== "string" || typeof relation?.object !== "string" || typeof label !== "string") return null;
    const kind = relation.kind ?? "person";
    if (!Object.prototype.hasOwnProperty.call(relationshipKinds, kind)) return null;
    const value: MemoryRelationship = {
      kind, subject: relation.subject.trim(), label: label.trim(), object: relation.object.trim(),
      directed: typeof relation.directed === "boolean" ? relation.directed : typeof relation.action === "string",
      certainty: relation.certainty === "inferred" || memory.source === "auto" ? "inferred" : "confirmed",
      occurredAt: typeof relation.occurredAt === "string" ? relation.occurredAt : "",
    };
    const encoded = relationshipText(typeof relation.action === "string" ? { ...value, certainty: "confirmed" } : value);
    return canSaveMemoryRelationship(value) && memory.content === encoded ? value : null;
  } catch { /* Older notes can carry non-JSON metadata. */ }
  return null;
}

export function relationshipMemory(relation: MemoryRelationship, metadata?: string) {
  const relationship = { ...relation, subject: relation.subject.trim(), label: relation.label.trim(), object: relation.object.trim() };
  const original = metadata ? JSON.parse(metadata) : {};
  return { content: relationshipText(relationship), metadata: JSON.stringify({ ...original, relationship }) };
}

export function isReviewedAiRelationship(memory: MemoryRecord): boolean {
  if (!memory.metadata) return false;
  try {
    const { candidate_id, source_kind } = JSON.parse(memory.metadata);
    return typeof candidate_id === "string" && source_kind === "user_message";
  } catch { return false; }
}
