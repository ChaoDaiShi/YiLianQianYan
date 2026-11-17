import { expect, it } from "vitest";
import { memoryVersionDiff } from "./MemoryGraphInspector";

it("shows changed text without splitting a Unicode character or repeating the identical context", () => {
  expect(memoryVersionDiff("安排𠮷县周一验收", "安排𠮷县周五验收")).toEqual({removed:"一",inserted:"五"});
  expect(memoryVersionDiff("已确认", "已确认")).toEqual({removed:"",inserted:""});
  expect(memoryVersionDiff("任务", "新任务")).toEqual({removed:"",inserted:"新"});
});
