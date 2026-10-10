import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "review-control-token" }),
}));

import { requestBlob } from "./http";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("requestBlob", () => {
  it("uses the configured API base and local control-session header", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(new Blob(["review-export"]), { status: 200 })
    );
    vi.stubGlobal("fetch", fetchMock);

    const result = await requestBlob("/api/cloud/export/job-1/download");

    expect(result.ok).toBe(true);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/cloud/export/job-1/download"),
      expect.objectContaining({
        method: "GET",
        headers: { "X-Yilian-Control-Session": "review-control-token" },
      })
    );
  });
});
