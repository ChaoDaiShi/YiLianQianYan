import { Check, type LucideIcon } from "lucide-react";

export function ComposerOption({ icon: Icon, label, description, selected, important, disabled, onClick }: {
  icon: LucideIcon;
  label: string;
  description: string;
  selected: boolean;
  important?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return <button className="composer-option" type="button" aria-pressed={selected} data-important={important || undefined} disabled={disabled} onClick={onClick}>
    <span className="composer-option-icon"><Icon size={17} aria-hidden="true" /></span>
    <span className="composer-option-copy"><strong>{label}</strong><small>{description}</small></span>
    {selected && <Check className="composer-option-check" size={15} aria-hidden="true" />}
  </button>;
}
