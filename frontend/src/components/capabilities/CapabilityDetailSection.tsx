interface CapabilityDetailSectionProps {
  title: string;
  children: React.ReactNode;
  technical?: boolean;
}

export default function CapabilityDetailSection({ title, children, technical = false }: CapabilityDetailSectionProps) {
  if (technical) {
    return (
      <details className="capability-detail-section capability-detail-technical">
        <summary>{title}</summary>
        <div className="capability-detail-section-body">{children}</div>
      </details>
    );
  }

  const headingId = `capability-detail-${title.replace(/[^\p{L}\p{N}]+/gu, "-").toLowerCase()}`;
  return (
    <section className="capability-detail-section" aria-labelledby={headingId}>
      <h3 id={headingId}>{title}</h3>
      <div className="capability-detail-section-body">{children}</div>
    </section>
  );
}
