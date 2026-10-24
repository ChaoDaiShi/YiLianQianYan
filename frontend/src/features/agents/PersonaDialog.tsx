import { Modal } from "../../components/ui";
import PersonaEditor from "./PersonaEditor";
export default function PersonaDialog({ open, agentId, onClose, onSaved }: { open: boolean; agentId: string; onClose: () => void; onSaved?: () => void }) {
  return <Modal open={open} onClose={onClose} title="角色与用户提示词" size="lg">
    {open && <PersonaEditor key={agentId} agentId={agentId} onCancel={onClose} onSaved={() => { onSaved?.(); onClose(); }} />}
  </Modal>;
}
