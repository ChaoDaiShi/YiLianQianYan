import { useState } from "react";
import type { ConversationInput, UserInputAnswer } from "../../api/chat";
import { Button, Textarea } from "../ui";

export default function UserInputCard({ question, pending, onAnswer, onCancel }: { question: ConversationInput; pending: boolean; onAnswer: (answer: UserInputAnswer) => void; onCancel: () => void }) {
  const [option, setOption] = useState("");
  const [text, setText] = useState("");
  const valid = question.kind === "text" ? Boolean(text.trim()) : Boolean(option || (question.allow_text && text.trim()));
  return <section className="mx-4 my-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 shadow-sm" aria-label="需要补充信息">
    <p className="text-xs text-[var(--text-secondary)]">需要补充信息 · 回答后继续当前轮次</p>
    <h3 className="mt-1 text-sm font-medium">{question.title}</h3>
    {question.kind === "single_choice" && <fieldset className="mt-3 space-y-2" disabled={pending}>
      <legend className="sr-only">请选择一项</legend>
      {question.options.map((item) => <label key={item.id} className="flex items-start gap-2 rounded border border-[var(--border-soft)] px-3 py-2 text-sm">
        <input type="radio" className="mt-1" name={question.question_id} value={item.id} checked={option === item.id} onChange={() => setOption(item.id)} />
        <span>{item.label}{item.description && <span className="mt-1 block text-xs text-[var(--text-secondary)]">{item.description}</span>}</span>
      </label>)}
    </fieldset>}
    {(question.kind === "text" || question.allow_text) && <div className="mt-3"><Textarea label={question.kind === "text" ? "你的回答" : "补充说明"} value={text} onChange={(event) => setText(event.target.value)} disabled={pending} /></div>}
    <div className="mt-3 flex items-center gap-2">
      <Button size="sm" disabled={!valid || pending} onClick={() => onAnswer({ revision: question.revision, option_id: option || undefined, text: text.trim() || undefined })}>{pending ? "正在继续…" : "回答并继续"}</Button>
      <Button size="sm" variant="ghost" disabled={pending} onClick={onCancel}>停止本轮</Button>
    </div>
  </section>;
}
