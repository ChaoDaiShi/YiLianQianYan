import PageShell, { type PageShellProps } from "../../components/layout/PageShell";
export default function ManagementFrame({ embedded = false, actions, children, ...props }: PageShellProps & { embedded?: boolean }) {
  if (!embedded) return <PageShell {...props} actions={actions}>{children}</PageShell>;
  return <section className="capability-embedded-manager flex min-h-0 flex-col h-full">{actions && <div className="flex shrink-0 justify-end gap-2 pb-3">{actions}</div>}<div className="min-h-0 flex-1 overflow-auto">{children}</div></section>;
}
