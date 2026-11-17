import { describe, expect, it } from "vitest";
import { compileMemorySpace } from "./memorySpaceGeometry";
import { buildMemorySolidGeometry, MEMORY_MESH_STRIDE } from "./memorySolidGeometry";

function positions(vertices: Float32Array) {
  const result: Array<[number, number, number]> = [];
  for (let i = 0; i < vertices.length; i += MEMORY_MESH_STRIDE) {
    result.push([vertices[i], vertices[i + 1], vertices[i + 2]]);
  }
  return result;
}

describe("volumetric memory geometry", () => {
  it("generates closed spheres for versions without making extra index nodes below the base", () => {
    const scene = compileMemorySpace([
      { id: "a", x: 0, y: 0, currentVersionId: "a:0", versions: [{ version_id: "a:0", layer: 0 }] },
      { id: "b", x: 150, y: 0, currentVersionId: "b:0", versions: [{ version_id: "b:0", layer: 0 }] },
    ], []);
    const geometry = buildMemorySolidGeometry(scene, [], null, false);
    expect(geometry.spheres).toBe(2);
    expect(geometry.tubes).toBe(0);
    const sphere = positions(geometry.projection).filter(([x]) => x < 25);
    for (const axis of [0, 1, 2] as const) {
      const range = sphere.map(point => point[axis]);
      expect(Math.min(...range)).toBeCloseTo(-17);
      expect(Math.max(...range)).toBeCloseTo(17);
    }
    expect(geometry.upper).toHaveLength(0);
    expect(positions(geometry.base).every(([, , z]) => z >= -17)).toBe(true);
  });

  it("sweeps a round tube on the cubic curvature above the world chord", () => {
    const items = [
      { id: "a", x: 0, y: 0, currentVersionId: "a:1", versions: [{ version_id: "a:1", layer: 1 }] },
      { id: "b", x: 205, y: 0, currentVersionId: "b:1", versions: [{ version_id: "b:1", layer: 1 }] },
    ];
    const edge = { id: "ab", source: "a", target: "b", directed: true };
    const empty = buildMemorySolidGeometry(compileMemorySpace(items, []), [], null, false);
    const connected = buildMemorySolidGeometry(compileMemorySpace(items, [edge]), [edge], null, false);
    const surface = positions(connected.upper.slice(empty.upper.length));
    expect(connected.tubes).toBe(1);
    const middle = surface.filter(([x]) => x > 95 && x < 110);
    expect(middle.length).toBeGreaterThan(20);
    expect(middle.every(([, y, z]) => Math.abs(y) > 20 && z > 117)).toBe(true);
    expect(Math.max(...middle.map(([, , z]) => z)) - Math.min(...middle.map(([, , z]) => z))).toBeCloseTo(2 * 1.45, 1);
  });

  it("keeps a proven historical relationship between base points in the base batch only", () => {
    const items = [
      { id: "a", x: 0, y: 0, currentVersionId: null, versions: [{ version_id: "a:0", layer: 0 }] },
      { id: "b", x: 205, y: 0, currentVersionId: null, versions: [{ version_id: "b:0", layer: 0 }] },
    ];
    const edge = { id: "ab", source: "a", target: "b", sourceVersionId: "a:0", targetVersionId: "b:0", historical: true };
    const model = buildMemorySolidGeometry(compileMemorySpace(items, [edge]), [edge], null, false);
    expect(model.tubes).toBe(1);
    expect(model.projection).toHaveLength(0);
    expect(model.upper).toHaveLength(0);
    const middle = positions(model.base).filter(([x, y]) => x > 95 && x < 110 && Math.abs(y) > 20);
    expect(middle.some(([, , z]) => z > 0)).toBe(true);
    expect(middle.every(([, , z]) => Math.abs(z) <= 2.51)).toBe(true);
  });
});
