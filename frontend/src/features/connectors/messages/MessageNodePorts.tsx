import { Handle, Position } from "@xyflow/react";

const EDGES = [
  ["top", Position.Top], ["right", Position.Right],
  ["bottom", Position.Bottom], ["left", Position.Left],
] as const;

export default function MessageNodePorts() {
  return <>{EDGES.flatMap(([edge, position]) => [
    <Handle key={`in-${edge}`} id={`in-${edge}`} type="target" position={position} isConnectable={false} />,
    <Handle key={`out-${edge}`} id={`out-${edge}`} type="source" position={position} isConnectable={false} />,
  ])}</>;
}
