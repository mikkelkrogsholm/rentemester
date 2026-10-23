import { afterEach, expect, it, vi } from "bun:test";
import { restoreGlobals, stubGlobal } from "../../test/globals";
import { withProtocol } from "../../test/protocol";
import { request, requestBlob } from "./_shared";

afterEach(restoreGlobals);

for (const binary of [false, true]) {
  const invoke = binary ? requestBlob : request;
  it(`${binary ? "binary" : "JSON"} 401 expires a current session but not a superseded read`, async () => {
    const expired = vi.fn();
    const target = new EventTarget();
    target.addEventListener("rentemester:auth-expired", expired);
    stubGlobal("window", target);
    stubGlobal("fetch", vi.fn(withProtocol(async () => Response.json({ ok: false, code: "unauthorized" }, { status: 401 }))));
    await expect(invoke("/api/read")).rejects.toMatchObject({ code: "unauthorized", status: 401 });
    expect(expired).toHaveBeenCalledTimes(1);
    const controller = new AbortController();
    stubGlobal("fetch", vi.fn(withProtocol(async () => ({
      ok: false, status: 401,
      json: async () => { controller.abort(); return { ok: false, code: "unauthorized" }; },
    }))));
    await expect(invoke("/api/read", { signal: controller.signal })).rejects.toMatchObject({ name: "AbortError" });
    expect(expired).toHaveBeenCalledTimes(1);
  });
}

it("malformed binary error bodies remain typed and do not retry a write", async () => {
  const fetchMock = vi.fn(withProtocol(async () => new Response("not JSON", { status: 409 })));
  stubGlobal("fetch", fetchMock);
  await expect(requestBlob("/api/write", { method: "POST" })).rejects.toMatchObject({ code: "internal", status: 409 });
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/write")).toHaveLength(1);
});

it("binary body read failure after dispatch preserves uncertain write guidance", async () => {
  const fetchMock = vi.fn(withProtocol(async () => ({ ok: true, status: 200, blob: async () => { throw new TypeError("broken body"); } })));
  stubGlobal("fetch", fetchMock);
  await expect(requestBlob("/api/write", { method: "POST" })).rejects.toMatchObject({ code: "internal", message: expect.stringContaining("Kontrollér status") });
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/write")).toHaveLength(1);
});
