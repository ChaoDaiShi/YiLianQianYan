import ResourceBindingPanel from "../../resources/ResourceBindingPanel";

interface ResourceSectionProps {
  graphId: string;
  nodeId: string;
}

/** Explicit, persisted bindings between this node and stored resources. */
export default function ResourceSection({ graphId, nodeId }: ResourceSectionProps) {
  return (
    <section className="mt-5 border-t border-[var(--border-soft)] pt-4">
      <ResourceBindingPanel target={{ kind: "node", graph_id: graphId, node_id: nodeId }} />
    </section>
  );
}
