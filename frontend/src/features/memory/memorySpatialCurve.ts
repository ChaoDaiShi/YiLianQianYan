export type MemoryPoint3 = {x: number; y: number; z: number};
export type MemorySpatialCurve = readonly [MemoryPoint3, MemoryPoint3, MemoryPoint3, MemoryPoint3];
export type MemoryCurveKind = "relation" | "event" | "history" | "semantic" | "base";

/** Curve endpoints are the actual memory positions in world coordinates. */
export function memorySpatialCurve(from: MemoryPoint3, to: MemoryPoint3, kind: MemoryCurveKind, id: string): MemorySpatialCurve {
  const dx = to.x - from.x, dy = to.y - from.y, dz = to.z - from.z;
  const planar = Math.hypot(dx, dy);
  const length = Math.hypot(planar, dz);
  const stem = kind === "history" || kind === "semantic";
  const sign = kind === "event" ? 1 : [...id].reduce((parity, char) => parity ^ char.charCodeAt(0), 0) % 2 ? -1 : 1;
  const bend = Math.min(stem ? 15 : 72, length * (stem ? .12 : kind === "event" ? .3 : .18)) * sign;
  const lateralX = planar > 0.001 ? -dy / planar : 1;
  const lateralY = planar > 0.001 ? dx / planar : 0;
  const lift = kind === "event" ? Math.min(48, length * .19) : stem || kind === "base" ? 0 : Math.min(18, length * .075);
  const control = (t: number): MemoryPoint3 => ({
    x: from.x + dx * t + lateralX * bend,
    y: from.y + dy * t + lateralY * bend,
    z: from.z + dz * t + lift,
  });
  return [from, control(1 / 3), control(2 / 3), to];
}

export function sampleMemorySpatialCurve([a,b,c,d]: MemorySpatialCurve, t: number): MemoryPoint3 {
  const s = 1 - t;
  const axis = (name: keyof MemoryPoint3) =>
    s ** 3 * a[name] + 3 * s * s * t * b[name] + 3 * s * t * t * c[name] + t ** 3 * d[name];
  return { x: axis("x"), y: axis("y"), z: axis("z") };
}

/** Each half retains a smaller control hull around the same curve. */
export function splitMemorySpatialCurve([a, b, c, d]: MemorySpatialCurve): [MemorySpatialCurve, MemorySpatialCurve] {
  const midpoint = (p: MemoryPoint3, q: MemoryPoint3) => ({
    x: (p.x + q.x) / 2, y: (p.y + q.y) / 2, z: (p.z + q.z) / 2,
  });
  const ab = midpoint(a, b), bc = midpoint(b, c), cd = midpoint(c, d);
  const left = midpoint(ab, bc), right = midpoint(bc, cd);
  const middle = midpoint(left, right);
  return [[a, ab, left, middle], [middle, right, cd, d]];
}
