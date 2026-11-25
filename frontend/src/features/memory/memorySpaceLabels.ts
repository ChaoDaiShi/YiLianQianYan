type ScreenRect = { left: number; top: number; right: number; bottom: number };
type Label = {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
  priority: number;
  depth: number;
  revealed?: boolean;
  centered?: boolean;
  above?: boolean;
  anchorPadding?: number;
};
type Placement = { x: number; y: number; bounds: ScreenRect; above: boolean };

const overlap = (a: ScreenRect, b: ScreenRect) =>
  a.left < b.right + 4 && a.right + 4 > b.left && a.top < b.bottom + 4 && a.bottom + 4 > b.top;

export function memorySpaceLineClearance(points: readonly { x: number; y: number }[]): ScreenRect[] {
  return points.slice(1).map((point, index) => {
    const previous = points[index];
    return {
      left: Math.min(previous.x, point.x) - 2, top: Math.min(previous.y, point.y) - 2,
      right: Math.max(previous.x, point.x) + 2, bottom: Math.max(previous.y, point.y) + 2,
    };
  });
}

function nearestUnblocked(value: number, min: number, max: number, intervals: [number, number][]) {
  const target = Math.max(min, Math.min(max, value));
  let start = -Infinity, end = -Infinity;
  for (const [lower, upper] of intervals.sort((a, b) => a[0] - b[0])) {
    if (lower >= end) {
      if (target > start && target < end) break;
      [start, end] = [lower, upper];
    } else end = Math.max(end, upper);
  }
  if (target <= start || target >= end) return target;
  if (start < min) return end <= max ? end : undefined;
  if (end > max) return start;
  return target - start <= end - target ? start : end;
}

/** Search the nearby free space, placing the box just beyond obstacles. */
function slidePlacement(label: Label, original: Placement, occupied: ScreenRect[], viewport: ScreenRect): Placement | undefined {
  const { left, top, right, bottom } = original.bounds;
  const width = right - left, height = bottom - top;
  const reach = 36;
  const box = { left: Math.max(viewport.left, left - reach), top: Math.max(viewport.top, top - reach),
    right: Math.min(viewport.right, right + reach), bottom: Math.min(viewport.bottom, bottom + reach) };
  const maxLeft = box.right - width, maxTop = box.bottom - height;
  if (maxLeft < box.left || maxTop < box.top) return undefined;
  const nearby = occupied.filter(item => overlap(box, item));
  const gap = 4.1;
  const xs = new Set([Math.max(box.left, Math.min(maxLeft, left)),
    ...nearby.flatMap(item => [item.left - width - gap, item.right + gap])]);
  let nearest: Placement | undefined;
  let distance = Infinity;
  for (const x of xs) {
    if (x < box.left || x > maxLeft || Math.abs(x - left) >= distance) continue;
    const blocking = nearby.filter(item => x < item.right + gap && x + width + gap > item.left);
    const y = nearestUnblocked(top, box.top, maxTop, blocking.map(item => [item.top - height - gap, item.bottom + gap]));
    if (y === undefined) continue;
    const candidate = { left:x, top:y, right:x+width, bottom:y+height };
    if (nearby.some(item => overlap(candidate, item))) continue;
    const move = Math.hypot(x - left, y - top);
    if (move < distance) {
      nearest = { x: original.x+(x-left)/label.scale, y: original.y+(y-top)/label.scale,
        bounds:candidate, above:original.above };
      distance = move;
    }
  }
  return nearest;
}

export function placeMemorySpaceEdgeLabel(
  x: number,
  y: number,
  width: number,
  occupied: ScreenRect[],
  size: { width: number; height: number },
  verticalInset: [number, number],
) {
  const bounds = { left: x - width / 2, right: x + width / 2, top: y - 14, bottom: y + 4 };
  if (bounds.left < 4 || bounds.right > size.width - 4 ||
    bounds.top < verticalInset[0] + 4 || bounds.bottom > size.height - verticalInset[1] - 4 ||
    occupied.some(existing => overlap(existing, bounds))) return false;
  occupied.push(bounds);
  return true;
}

export function placeMemorySpaceLabels(labels: Label[], size: { width: number; height: number }, reserved: ScreenRect[] = [],
  verticalInset: [number, number] = [0, 0], previous?: ReadonlyMap<string, Placement>) {
  const occupied = [...reserved];
  const placements = new Map<string, Placement>();
  const viewport = { left:4, top:4+verticalInset[0], right:size.width-4, bottom:size.height-4-verticalInset[1] };
  const candidates = [...labels].sort((a, b) => b.priority - a.priority || (previous ? 0 : b.depth - a.depth) || a.id.localeCompare(b.id));
  for (const label of candidates) {
    const row = label.height + 5;
    const offsets = label.priority > 1 ? [-7, -7 - row, -7 + row, -7 - 2 * row, -7 + 2 * row] : [-7];
    const centered = label.centered ?? false;
    const gap = label.anchorPadding ?? 7;
    const above = !centered && label.above ? [[-label.width / 2 + 6, -gap - label.height + 14]] : [];
    const positions = centered ? [[-label.width / 2, -label.height / 2]]
      : [...above, ...offsets.flatMap(y => [gap + 6, -label.width - gap + 6].map(x => [x, y]))];
    const position = (x: number, y: number, above: boolean) => {
      const left = label.x + (x - (centered ? 0 : 6)) * label.scale;
      const top = label.y + (y - (centered ? 0 : 14)) * label.scale;
      return { x, y, above, bounds: { left, top, right: left + label.width * label.scale, bottom: top + label.height * label.scale } };
    };
    const choices = positions.map(([x, y], index) => position(x, y, above.length > 0 && index === 0));
    const width = label.width * label.scale, height = label.height * label.scale;
    const reach = 40;
    const labelViewport = previous ? {
      left: Math.max(viewport.left, label.x - reach - width), right: Math.min(viewport.right, label.x + reach + width),
      top: Math.max(viewport.top, label.y - reach - height), bottom: Math.min(viewport.bottom, label.y + reach + height),
    } : viewport;
    const fits = ({ bounds }: Placement) => bounds.left >= labelViewport.left && bounds.right <= labelViewport.right
      && bounds.top >= labelViewport.top && bounds.bottom <= labelViewport.bottom;
    const coversEarlierAnchor = ({ left, top, right, bottom }: ScreenRect) => centered && candidates.some(other =>
      other.id !== label.id && (other.priority >= label.priority || placements.has(other.id)) &&
      other.x > left + 2 && other.x < right - 2 && other.y > top + 2 && other.y < bottom - 2);
    const clear = (placement: Placement) => fits(placement) && !coversEarlierAnchor(placement.bounds) &&
      !occupied.some(existing => overlap(placement.bounds, existing));
    const saved = previous?.get(label.id);
    const retained = saved && position(saved.x, saved.y, saved.above);
    const placed = (retained && (clear(retained) ? retained : slidePlacement(label, retained, occupied, labelViewport)))
      ?? choices.find(clear)
      ?? (previous && label.priority >= 2 ? slidePlacement(label, choices[0], occupied, labelViewport) : undefined)
      ?? (!previous && label.revealed ? choices.find(fits) : undefined);
    if (!placed) continue;
    placements.set(label.id, placed);
    occupied.push(placed.bounds);
  }
  return placements;
}
