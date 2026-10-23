import { afterEach, describe, expect, it, vi } from "bun:test";
import { protocolResponse } from "../../test/protocol";
import { restoreGlobals, stubGlobal } from "../../test/globals";
import { request, requestBlob } from "./_shared";

afterEach(restoreGlobals);

describe("passive protocol admission", () => {
  it("fetches credential-free metadata then dispatches exactly one POST with bound headers", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => String(input) === "/api/protocol"
      ? protocolResponse() : Response.json({ ok: true, amount: "9007199254740993" }));
    stubGlobal("fetch", fetchMock);
    expect(await request("/api/write", { method: "POST", body: "{}", headers: { authorization: "secret", "x-rentemester-protocol": "99" } })).toEqual({ ok: true, amount: "9007199254740993" });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const preflight = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(preflight).toEqual(["/api/protocol", { method: "GET", credentials: "omit", redirect: "error", signal: undefined }]);
    const business = fetchMock.mock.calls[1] as unknown as [string, RequestInit];
    expect(business[1].method).toBe("POST");
    expect(business[1].credentials).toBe("same-origin");
    const headers = new Headers(business[1].headers);
    expect([...headers].filter(([k]) => k.startsWith("x-rentemester-"))).toEqual([
      ["x-rentemester-manifest", "a".repeat(64)], ["x-rentemester-protocol", "1"], ["x-rentemester-wire-profile", "legacy-react-v1"],
    ]);
  });

  for (const [label, response] of [
    ["unknown major", () => protocolResponse({ protocolMajor: 2 })],
    ["unknown profile", () => protocolResponse({ wireProfiles: ["rust-http-v1"] })],
    ["missing profile", () => protocolResponse({ wireProfiles: undefined })],
    ["bad identity", () => protocolResponse({ manifestId: "A".repeat(64) })],
    ["unsupported route", () => protocolResponse({ operations: [] })],
    ["duplicate", () => protocolResponse({ operations: Array(2).fill({ method: "POST", path: "/api/write", capability: "POST /api/write" }) })],
    ["wildcard", () => protocolResponse({ operations: [{ method: "POST", path: "/api/*", capability: "POST /api/*" }] })],
    ["query route", () => protocolResponse({ operations: [{ method: "POST", path: "/api/write?x=1", capability: "POST /api/write?x=1" }] })],
    ["malformed", () => new Response("not-json")],
    ["404", () => new Response("", { status: 404 })],
    ["503", () => new Response("", { status: 503 })],
    ["oversized", () => new Response(" ".repeat(1024 * 1024 + 1))],
  ] as const) {
    it(`refuses ${label} before any POST`, async () => {
      const fetchMock = vi.fn(async (_input: RequestInfo | URL) => response());
      stubGlobal("fetch", fetchMock);
      await expect(request("/api/write", { method: "POST" })).rejects.toMatchObject({ code: "protocol_incompatible", status: 0 });
      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(fetchMock.mock.calls[0]?.[0]).toBe("/api/protocol");
    });
  }

  it("matches parameters as exactly one nonempty segment, not prefixes or other methods", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => String(input) === "/api/protocol" ? protocolResponse() : Response.json({ ok: true }));
    stubGlobal("fetch", fetchMock);
    await request("/api/companies/synthetic/journal?year=2025");
    for (const path of ["/api/companies//journal", "/api/companies/synthetic/journal/extra", "/api/companies/a%2Fb/journal"]) {
      await expect(request(path)).rejects.toMatchObject({ code: "protocol_incompatible" });
    }
    await expect(request("/api/read", { method: "POST" })).rejects.toMatchObject({ code: "protocol_incompatible" });
    expect(fetchMock.mock.calls.filter(([url]) => url !== "/api/protocol")).toHaveLength(1);
  });

  it("accepts the explicit legacy API root route in a real Bun manifest", async () => {
    const fetchMock = vi.fn(async (url: RequestInfo | URL) => url === "/api/protocol"
      ? protocolResponse({ operations: [{ method: "GET", path: "/api", capability: "GET /api" }, { method: "GET", path: "/api/health", capability: "GET /api/health" }] })
      : Response.json({ ok: true }));
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/health")).resolves.toEqual({ ok: true });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("does not retry a server admission rejection", async () => {
    const fetchMock = vi.fn(async (url: RequestInfo | URL) => url === "/api/protocol" ? protocolResponse() : Response.json({ ok: false, code: "protocol_incompatible", errors: ["Changed manifest"] }, { status: 409 }));
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/write", { method: "POST" })).rejects.toMatchObject({ code: "protocol_incompatible", status: 409 });
    expect(fetchMock.mock.calls.filter(([url]) => url === "/api/write")).toHaveLength(1);
  });

  it("keeps independent origins and restarted manifests independent", async () => {
    const fetchMock = vi.fn(async (url: RequestInfo | URL) => String(url).endsWith("/api/protocol") ? protocolResponse() : Response.json({ ok: true }));
    stubGlobal("fetch", fetchMock);
    await request("https://one.invalid/api/read");
    await request("https://two.invalid/api/read");
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual(["https://one.invalid/api/protocol", "https://one.invalid/api/read", "https://two.invalid/api/protocol", "https://two.invalid/api/read"]);
    stubGlobal("fetch", vi.fn(async () => protocolResponse({ protocolMajor: 2 })));
    await expect(request("https://one.invalid/api/read")).rejects.toMatchObject({ code: "protocol_incompatible" });
  });

  it("does not dispatch or expire authentication when preflight is cancelled", async () => {
    const controller = new AbortController();
    const expired = vi.fn();
    const target = new EventTarget();
    target.addEventListener("rentemester:auth-expired", expired);
    stubGlobal("window", target);
    const fetchMock = vi.fn(async () => { controller.abort(); return new Response("", { status: 401 }); });
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/read", { signal: controller.signal })).rejects.toMatchObject({ name: "AbortError" });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(expired).not.toHaveBeenCalled();
  });

  it("keeps binary response headers and payload intact", async () => {
    stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => url === "/api/protocol" ? protocolResponse() : new Response(new Uint8Array([0, 255, 1]), { headers: { "content-disposition": "attachment; filename=x.pdf" } })));
    const { blob, headers } = await requestBlob("/api/blob");
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([0, 255, 1]);
    expect(headers.get("content-disposition")).toBe("attachment; filename=x.pdf");
  });
});
