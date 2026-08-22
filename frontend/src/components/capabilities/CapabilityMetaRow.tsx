interface CapabilityMetaRowProps {
  label: string;
  value?: React.ReactNode;
  mono?: boolean;
}

export default function CapabilityMetaRow({ label, value, mono = false }: CapabilityMetaRowProps) {
  if (!value) return null;

  return (
    <div className="capability-meta-row">
      <dt>{label}</dt>
      <dd className={mono ? "font-mono" : undefined}>{value}</dd>
    </div>
  );
}
