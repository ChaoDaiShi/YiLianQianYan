import { expect, it } from "vitest";
import { memorySpatialCurve, sampleMemorySpatialCurve, splitMemorySpatialCurve } from "./memorySpatialCurve";

it("keeps actual endpoints and bends beyond the chord in 2D and 3D", () => {
  const start = {x:0,y:0,z:0}, end = {x:200,y:0,z:112};
  const curve = memorySpatialCurve(start,end,"relation","edge-1");
  expect(sampleMemorySpatialCurve(curve,0)).toEqual(start);
  expect(sampleMemorySpatialCurve(curve,1)).toEqual(end);
  const middle = sampleMemorySpatialCurve(curve,.5);
  expect(Math.abs(middle.y)).toBeGreaterThan(5);
  expect(middle.z).toBeGreaterThan(0);
  expect(middle.z).toBeLessThan(112);
});

it("keeps version trajectories and reviewed changes distinguishable from normal relations", () => {
  const start = {x:0,y:0,z:0}, end = {x:0,y:0,z:112};
  expect(Math.abs(sampleMemorySpatialCurve(memorySpatialCurve(start,end,"history","one"),.5).x)).toBeGreaterThan(1);
  expect(memorySpatialCurve(start,end,"event","one")).not.toEqual(memorySpatialCurve(start,end,"relation","one"));
});

it("retains the same spatial curve after subdividing its control hull", () => {
  const curve = memorySpatialCurve({x:20,y:-65,z:0},{x:200,y:90,z:112},"relation","edge");
  const [start, end] = splitMemorySpatialCurve(curve);
  for (const [half, offset] of [[start, 0], [end, .5]] as const) {
    const local = sampleMemorySpatialCurve(half, .35);
    const global = sampleMemorySpatialCurve(curve, offset + .175);
    expect(local.x).toBeCloseTo(global.x, 10);
    expect(local.y).toBeCloseTo(global.y, 10);
    expect(local.z).toBeCloseTo(global.z, 10);
  }
});
