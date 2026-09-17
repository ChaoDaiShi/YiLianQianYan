import { describe, expect, it, vi } from "vitest";
import { AttachmentQueue } from "./attachmentQueue";

const file = (name = "notes.txt", size = 5, type = "text/plain") => ({ name, size, type } as File);
const options = () => ({ upload: vi.fn(async () => ({ id: "res-real" })), preview: vi.fn(async () => ({ status: "ready" })) });

describe("bounded attachment queue", () => {
  it("tracks upload and parsing before allowing a resource binding", async () => {
    const states: string[] = [];
    const deps = options();
    const queue = new AttachmentQueue({ ...deps, changed: () => states.push(queue.snapshot()[0]?.state ?? "empty") });
    queue.add([file()]);
    expect(queue.snapshot()).toHaveLength(1);
    expect(queue.snapshot()[0].state).toBe("queued");
    expect(queue.readyIds()).toEqual([]);
    await queue.process();
    expect(states).toEqual(["queued", "uploading", "parsing", "ready"]);
    expect(queue.readyIds()).toEqual(["res-real"]);
  });
  it("rejects count, size and unsupported type before any upload", async () => {
    const deps = options(); const queue = new AttachmentQueue(deps);
    expect(() => queue.add(Array.from({ length: 9 }, () => file()))).toThrow();
    queue.add([file("too-large.txt", 26 * 1024 * 1024), file("program.exe", 5, "application/octet-stream")]);
    await queue.process();
    expect(queue.snapshot().every(item => item.state === "failed")).toBe(true);
    expect(deps.upload).not.toHaveBeenCalled();
  });
  it("removed uploads cannot revive or acquire bindings from late responses", async () => {
    let resolve!: (value: { id: string }) => void;
    const deps = options(); deps.upload.mockImplementation(() => new Promise(done => { resolve = done; }));
    const queue = new AttachmentQueue(deps); queue.add([file()]);
    expect(queue.snapshot()).toHaveLength(1);
    const processing = queue.process();
    const id = queue.snapshot()[0].id; queue.remove(id);
    resolve({ id: "late-resource" }); await processing;
    expect(queue.snapshot()[0].state).toBe("removed");
    expect(queue.readyIds()).toEqual([]);
    expect(deps.preview).not.toHaveBeenCalled();
  });
  it("keeps failed extraction explicit and never binds it", async () => {
    const deps = options(); deps.preview.mockResolvedValue({ status: "failed" });
    const queue = new AttachmentQueue(deps); queue.add([file()]); await queue.process();
    expect(queue.snapshot()).toHaveLength(1);
    expect(queue.snapshot()[0].state).toBe("failed");
    expect(queue.readyIds()).toEqual([]);
  });
});
