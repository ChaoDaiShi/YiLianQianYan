import { Input, Select } from "../../components/ui";
import { relationshipKinds, type MemoryRelationship, type RelationshipKind } from "./relationship";

export default function RelationshipFields({ value, onChange, disabled }: { value: MemoryRelationship; onChange: (value: MemoryRelationship) => void; disabled?: boolean }) {
  const people = value.kind === "person" || value.kind === "kinship";
  const event = value.kind === "event";
  const concept = value.kind === "concept";
  const preference = value.kind === "preference";
  return <fieldset className="memory-relationship-fields min-w-0" disabled={disabled}>
    <Select label="关系类型" value={value.kind} options={Object.entries(relationshipKinds).map(([value, label]) => ({ value, label }))}
      onChange={event => onChange({ ...value, kind: event.target.value as RelationshipKind, directed: event.target.value === "event" })} />
    <Input label={people ? "起点人物" : event ? "前序事件" : concept ? "起点概念" : preference ? "偏好主体" : "起点项目"} maxLength={60} value={value.subject} onChange={event => onChange({ ...value, subject: event.target.value })} />
    <Input label="关系" placeholder={value.kind === "kinship" ? "例如：姐姐" : event ? "例如：随后" : undefined} maxLength={40} value={value.label} onChange={event => onChange({ ...value, label: event.target.value })} />
    <Input label={people ? "关联人物" : event ? "后续事件" : concept ? "关联概念" : preference ? "偏好对象" : "关联项目或事项"} maxLength={60} value={value.object} onChange={event => onChange({ ...value, object: event.target.value })} />
    <Input label="发生日期（可选）" type="date" value={value.occurredAt} onChange={event => onChange({ ...value, occurredAt: event.target.value })} />
    <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={value.directed} onChange={event => onChange({ ...value, directed: event.target.checked })} />按填写顺序显示方向</label>
    <Select label="关系依据" value={value.certainty} options={[{ value: "confirmed", label: "已确认" }, { value: "inferred", label: "推断 · 待确认" }]}
      onChange={event => onChange({ ...value, certainty: event.target.value as MemoryRelationship["certainty"] })} />
  </fieldset>;
}
