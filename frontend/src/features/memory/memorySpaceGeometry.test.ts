import { describe, expect, it } from "vitest";
import { compileMemorySpace, defaultSpaceCamera, fitMemorySpace, flatMemoryCamera, frameMemorySpaceOrbit, frameMemorySpaceTransition, interpolateMemoryCamera, memorySpaceReveal, memoryVersionRadius, orbitMemorySpace, projectMemorySpace, transitionMemorySpace } from "./memorySpaceGeometry";
import { sampleMemorySpatialCurve } from "./memorySpatialCurve";

const revision = (id: string, layer: number) => ({ version_id: id, layer });
const viewport = { width: 950, height: 540 };
const scene = compileMemorySpace([
  { id: "a", x: 0, y: 0, currentVersionId: "a3", versions: [revision("a3", 2), revision("a1", 0), revision("a2", 1)] },
  { id: "b", x: 240, y: 50, currentVersionId: "b1", versions: [revision("b1", 0)] },
], [
  { id: "current", source: "a", target: "b" },
  { id: "past", source: "a", target: "b", sourceVersionId: "a2", targetVersionId: "b1" },
  { id: "missing-version", source: "a", target: "b", sourceVersionId: "purged" },
]);

describe("memory space", () => {
  it("keeps actual revisions growing upward even after a large vertical drag", () => {
    for (const delta of [-10000,0,10000]) {
      const camera = orbitMemorySpace(defaultSpaceCamera, 0, delta);
      const projected = projectMemorySpace(scene, camera, viewport).points;
      expect(projected.get("a1")!.y).toBeGreaterThan(projected.get("a2")!.y);
      expect(projected.get("a2")!.y).toBeGreaterThan(projected.get("a3")!.y);
    }
  });

  it("corrects a near-edge orbit with pan when its preferred scale can still fit", () => {
    const safe = { ...defaultSpaceCamera, zoom: .7 };
    const inset: [number, number] = [20, 30];
    expect(frameMemorySpaceOrbit(scene, safe, viewport, inset)).toEqual(safe);
    const offset = { ...safe, panY: 240 };
    const framed = frameMemorySpaceOrbit(scene, offset, viewport, inset);
    expect(framed.zoom).toBe(offset.zoom);
    expect(framed.panY).toBeLessThan(offset.panY);
  });
  it("uses the exact 2D scene pan and zoom before unfolding and after folding", () => {
    const transform = { x: 260, y: 190, zoom: 1.23 };
    const flat = flatMemoryCamera(scene, viewport, transform);
    for (const point of projectMemorySpace(scene, flat, viewport).points.values()) {
      expect(point.scale).toBeCloseTo(transform.zoom, 8);
    }
    const folded = projectMemorySpace(scene, flat, viewport).points;
    expect(folded.get("a1")?.x).toBeCloseTo(folded.get("a3")!.x, 8);
    expect(folded.get("a3")?.x).toBeCloseTo(transform.x, 8);
    expect(folded.get("a3")?.y).toBeCloseTo(transform.y, 8);
    expect(folded.get("b1")?.x).toBeCloseTo(240 * transform.zoom + transform.x, 8);
    expect(folded.get("b1")?.y).toBeCloseTo(50 * transform.zoom + transform.y, 8);
    const tilt = fitMemorySpace(scene, defaultSpaceCamera, viewport);
    const partial = projectMemorySpace(scene, interpolateMemoryCamera(flat, tilt, .5), viewport).points;
    expect(partial.get("a1")?.y).not.toBe(partial.get("a3")?.y);
    expect(interpolateMemoryCamera(tilt, flat, 1)).toEqual(flat);
  });

  it("keeps the visible network and growing versions in view across a narrow, rotated transition", () => {
    const narrow = { width: 535, height: 260 };
    const inset: [number, number] = [14, 30];
    const flat = flatMemoryCamera(scene, narrow, { x: 80, y: 235, zoom: 1.42 });
    const end = fitMemorySpace(scene, { ...defaultSpaceCamera, azimuth: 2.75 }, narrow, inset);
    expect(frameMemorySpaceTransition(scene, flat, narrow, inset, 0)).toEqual(flat);
    for (let frame = 1; frame <= 100; frame++) {
      const progress = frame / 100;
      const reveal = memorySpaceReveal(progress);
      if (reveal.base === 0) continue;
      const path = interpolateMemoryCamera(flat, end, progress);
      const camera = frameMemorySpaceTransition(scene, path, narrow, inset, progress);
      const { project } = projectMemorySpace(scene, camera, narrow, reveal.growth);
      const inside = (world: {x:number;y:number;z:number}, radius: number) => {
        const point = project(world);
        expect(point.x - radius * point.scale).toBeGreaterThan(0);
        expect(point.x + radius * point.scale).toBeLessThan(narrow.width);
        expect(point.y - radius * point.scale).toBeGreaterThan(inset[0]);
        expect(point.y + radius * point.scale).toBeLessThan(narrow.height - inset[1]);
      };
      for (const point of scene.points) inside(point, memoryVersionRadius(point));
      for (const { path } of scene.curves.values()) {
        for (let sample = 0; sample <= 16; sample++) inside(sampleMemorySpatialCurve(path, sample / 16), 4.5);
      }
    }
  });

  it("preserves entity anchors without inventing a version trajectory", () => {
    const entities = compileMemorySpace([
      { id: "m", x: 2, y: 4, currentVersionId: "m:1", versions: [revision("m:1", 0)] },
      { id: "source", x: 8, y: 9, versions: [], auxiliary: true },
    ], [{ id: "provenance", source: "source", target: "m" }]);
    expect(entities.points[1]).toMatchObject({ id: "source", x: 8, y: 9, current: true, auxiliary: true });
    expect(entities.risers).toHaveLength(0);
    expect(entities.links).toEqual([{ id: "provenance", source: "source", target: "m:1" }]);
  });
  it("uses independent per-memory heights and preserves base-plane coordinates", () => {
    expect(scene.points.map(({ id, x, y, z }) => [id, x, y, z])).toEqual([
      ["a1", 0, 0, 0], ["a2", 0, 0, 112], ["a3", 0, 0, 224], ["b1", 240, 50, 0],
    ]);
    expect(scene.risers.map(({ source, target }) => [source, target])).toEqual([["a1", "a2"], ["a2", "a3"]]);
  });

  it("shows the base network before lifting versions from their actual anchors", () => {
    expect(scene.points.filter(point => point.base).map(point => point.id)).toEqual(["a1", "b1"]);
    expect(scene.points.every(point => point.z >= 0)).toBe(true);
    expect(memorySpaceReveal(.32)).toEqual({ projection: 0, base: 1, growth: 0, upper: 0 });
    const mid = memorySpaceReveal(.65);
    expect(mid).toMatchObject({ base: 1, upper: 1 });
    expect(mid.growth).toBeGreaterThan(0);
    expect(mid.growth).toBeLessThan(1);
    const y = (id: string, growth: number) => projectMemorySpace(scene, defaultSpaceCamera, viewport, growth).points.get(id)!.y;
    expect(y("a3", 0)).toBe(y("a1", 0));
    expect(y("a3", 1)).toBeLessThan(y("a3", mid.growth));
    expect(y("a3", mid.growth)).toBeLessThan(y("a1", mid.growth));
    expect(y("a1", mid.growth)).toBe(y("a1", 0));
  });

  it("leaves unknown current heads and missing historical layers unresolved", () => {
    const incomplete = compileMemorySpace([
      { id: "a", x: 0, y: 0, versions: [revision("a3", 2), revision("a1", 0)] },
    ], []);
    expect(incomplete.points.every(point => !point.current)).toBe(true);
    expect(incomplete.risers).toHaveLength(0);
  });

  it("uses named relation versions, links current heads, and does not substitute an absent historical version", () => {
    expect(scene.links).toEqual([
      { id: "current", source: "a3", target: "b1" },
      { id: "past", source: "a2", target: "b1" },
    ]);
  });

  it("anchors current edges on the effective version even when newer history exists", () => {
    const actual = compileMemorySpace([
      { id: "a", x: 0, y: 0, currentVersionId: "a1", versions: [revision("a1", 0), revision("a2", 1)] },
      { id: "b", x: 80, y: 10, currentVersionId: null, versions: [revision("b1", 0)] },
      { id: "c", x: 180, y: 10, currentVersionId: "c1", versions: [revision("c1", 0)] },
    ], [
      { id: "current", source: "a", target: "c" },
      { id: "inactive", source: "a", target: "b" },
      { id: "reviewed-change", source: "b", target: "a", sourceVersionId: "b1", targetVersionId: "a2" },
    ]);
    expect(actual.points.filter(point => point.current).map(point => point.id)).toEqual(["a1", "c1"]);
    expect([...actual.cards.keys()]).toEqual(["a1", "c1"]);
    expect(actual.links).toEqual([
      { id: "current", source: "a1", target: "c1" },
      { id: "reviewed-change", source: "b1", target: "a2" },
    ]);
  });

  it("reverses at the current amount and takes only the remaining animation time", () => {
    const held = transitionMemorySpace(0, 1, 110);
    expect(held).toBeGreaterThan(0);
    expect(held).toBeLessThan(.5);
    expect(transitionMemorySpace(held, 0, 0)).toBe(held);
    expect(transitionMemorySpace(held, 0, 30)).toBeLessThan(held);
    expect(transitionMemorySpace(held, 0, 110)).toBe(0);
  });

  it("rotates and projects actual depth with a stable coordinate model", () => {
    const start = fitMemorySpace(scene, defaultSpaceCamera, viewport);
    const turned = orbitMemorySpace(start, 85, 47);
    const before = projectMemorySpace(scene, start, viewport);
    const after = projectMemorySpace(scene, turned, viewport);
    expect(before.points.get("a1")?.depth).not.toBe(before.points.get("a3")?.depth);
    expect(before.points.get("a1")?.x).not.toBe(after.points.get("a1")?.x);
    expect(after.points.get("a3")!.y).toBeLessThan(after.points.get("a1")!.y);
    expect(scene.points.find(point => point.id === "a3")?.z).toBe(224);
  });

  it("fits a larger scene and its layer extents inside a narrow viewport", () => {
    const large = compileMemorySpace(Array.from({ length: 48 }, (_, i) => ({
      id: `item-${i}`, x: (i % 8) * 245, y: Math.floor(i / 8) * 130,
      versions: Array.from({ length: 8 }, (_, layer) => revision(`${i}-${layer}`, layer)),
    })), []);
    const narrow = { width: 390, height: 480 };
    const projected = projectMemorySpace(large, fitMemorySpace(large, defaultSpaceCamera, narrow), narrow);
    expect(projected.points.size).toBe(384);
    for (const point of projected.points.values()) {
      expect(point.x).toBeGreaterThan(0);
      expect(point.y).toBeGreaterThan(0);
      expect(point.x).toBeLessThan(narrow.width);
      expect(point.y).toBeLessThan(narrow.height);
    }
  });

  it("frames version cores with room for their labels and the overlaid controls", () => {
    const narrow = { width: 780, height: 400 }, hud: [number, number] = [61, 28];
    const camera = fitMemorySpace(scene, defaultSpaceCamera, narrow, hud);
    const points = projectMemorySpace(scene, camera, narrow).points;
    for (const point of points.values()) {
      const core = 17 * point.scale;
      expect(point.x - core).toBeGreaterThan(0);
      expect(point.x + core).toBeLessThan(narrow.width);
      expect(point.y - Math.max(30, core)).toBeGreaterThan(hud[0]);
      expect(point.y + core).toBeLessThan(narrow.height - hud[1]);
    }
  });
});
