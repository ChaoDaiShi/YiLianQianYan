import type {Workspace} from "../../api/workspaces";
import type {SoftwareRegistration} from "../../api/software";
import {Button, Input, Modal} from "../../components/ui";
import type {SoftwareDraft} from "./softwareForm";
import {softwareExecutableDraft, validateSoftwareDraft} from "./softwareForm";
interface Props {
 registration?: SoftwareRegistration; draft: SoftwareDraft; workspaces: Workspace[];
 active: boolean; busy: boolean; error: string; onDraft: (draft: SoftwareDraft) => void;
 onSave: () => void; onClose: () => void;
}
export default function SoftwareRegistrationDialog({registration, draft, workspaces, active, busy, error, onDraft, onSave, onClose}: Props) {
 const change = (patch: Partial<SoftwareDraft>) => onDraft({...draft, ...patch});
 const validation = validateSoftwareDraft(draft).error;
 return <Modal open title={registration ? "软件登记与 AI 许可" : "登记软件"} size="lg" onClose={() => {if (!busy) onClose();}}
  footer={<><Button variant="secondary" disabled={busy} onClick={onClose}>取消</Button><Button loading={busy} disabled={busy || Boolean(validation)} onClick={onSave}>保存登记</Button></>}>
  <div className="software-form">
   <Input label="软件名称" placeholder="留空使用程序文件名" maxLength={80} value={draft.name} disabled={busy || active} onChange={event => change({name: event.target.value})} />
   <label className="software-field">适配类型<select value={draft.adapter} disabled={busy || active} onChange={event => change({adapter: event.target.value as SoftwareDraft["adapter"], allow_observe: false, allow_content: false, allow_window_ocr: false, allow_actions: false})}>
    <option value="generic">通用 Windows 软件</option>
    <option value="wechat">Windows 微信</option>
    {(import.meta.env.DEV || draft.adapter === "generic_fixture") && <option value="generic_fixture">受控测试窗口（开发验证）</option>}
   </select></label>
   <Input label="程序 EXE 绝对路径" value={draft.executable} disabled={busy || active} onChange={event => onDraft(softwareExecutableDraft(draft, event.target.value))} placeholder="C:\Apps\Example.exe" hint="仅登记现有程序，不安装软件、不执行命令行参数。更换路径或项目后须重新勾选 AI 许可。" />
   <label className="software-field">所属项目<select value={draft.workspace_id} disabled={busy || active} onChange={event => change({workspace_id: event.target.value, allow_observe: false, allow_content: false, allow_window_ocr: false, allow_actions: false})}>
    <option value="">个人软件（未关联项目）</option>
    {draft.workspace_id && !workspaces.some(item => item.id === draft.workspace_id && item.status === "active") && <option value={draft.workspace_id}>原项目已归档或暂不可用</option>}
    {workspaces.filter(item => item.status === "active").map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
   </select></label>
   <fieldset className="software-permissions"><legend>本软件的 AI 许可</legend>
    <label><input type="checkbox" checked={draft.allow_observe} disabled={busy || (active && !registration?.allow_observe)} onChange={event => change({allow_observe: event.target.checked, allow_content: event.target.checked && draft.allow_content, allow_window_ocr: event.target.checked && draft.allow_window_ocr, allow_actions: event.target.checked && draft.allow_actions})} /><span>允许 AI 观察已连接窗口的控件名称与结构</span></label>
    <p>观察可能包含当前界面的私密名称。开启控件观察后，忆涟千言就能识别按钮和选项。{draft.adapter === "wechat" && "连接微信后，还可以框选可见群聊，核对识别文字后带入对话。"}</p>
    <label><input type="checkbox" checked={draft.allow_content} disabled={busy || !draft.allow_observe || (active && !registration?.allow_content)} onChange={event => change({allow_content: event.target.checked})} /><span>允许 AI 读取可见控件文字和文档片段</span></label>
    <label><input type="checkbox" checked={draft.allow_window_ocr} disabled={busy || !draft.allow_observe || (active && !registration?.allow_window_ocr)} onChange={event => change({allow_window_ocr: event.target.checked})} /><span>允许在本机识别整个目标窗口的可见文字，并把文字提供给 AI</span></label>
    <label><input type="checkbox" checked={draft.allow_actions} disabled={busy || !draft.allow_observe || (active && !registration?.allow_actions)} onChange={event => change({allow_actions: event.target.checked})} /><span>允许 AI 请求控件操作</span></label>
    <p>这项许可不替代 Gateway 审批。操作绑定真实会话和当前观察；应用内容不能授予文件、消息或其他权限。</p>
   </fieldset>
   {active && <p className="software-notice">连接中只能收缩 AI 许可；修改软件、路径、项目或扩大许可前，请先断开会话。收缩许可会停止当前自动化。</p>}
   {validation && <p className="software-hint">{validation}</p>}
   {error && <p role="alert" className="software-error">{error}</p>}
  </div>
 </Modal>;
}
