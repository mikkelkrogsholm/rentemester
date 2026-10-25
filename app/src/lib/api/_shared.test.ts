import { afterEach, beforeEach, describe, expect, it, vi } from "bun:test";
import { withProtocol } from "../../test/protocol";
import { restoreGlobals, stubGlobal } from "../../test/globals";

beforeEach(() => { if (typeof window === "undefined") stubGlobal("window", new EventTarget()); });
afterEach(restoreGlobals);
import { ApiError, request, requestBlob } from "./_shared";

describe("read request cancellation", () => {
  it("passes the caller's signal and preserves an aborted fetch", async () => {
    const controller = new AbortController();
    const aborted = new DOMException("Cancelled", "AbortError");
    const fetchMock = vi.fn(withProtocol(async () => { controller.abort(); throw aborted; }));
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/read", { signal: controller.signal })).rejects.toBe(aborted);
    expect(fetchMock).toHaveBeenCalledWith("/api/read", expect.objectContaining({ signal: controller.signal, credentials: "same-origin" }));
  });

  it("preserves cancellation while reading the response body", async () => {
    const controller = new AbortController();
    const aborted = new DOMException("Cancelled", "AbortError");
    stubGlobal("fetch", vi.fn(withProtocol(async () => ({
      ok: true,
      status: 200,
      json: async () => { controller.abort(); throw aborted; },
    }))));
    await expect(request("/api/read", { signal: controller.signal })).rejects.toBe(aborted);
  });

  it("still reports a real network failure as a structured API error", async () => {
    stubGlobal("fetch", vi.fn(withProtocol(async () => { throw new TypeError("offline"); })));
    try {
      await request("/api/read");
      throw new Error("expected rejection");
    } catch (cause) {
      expect(cause).toBeInstanceOf(ApiError);
      expect((cause as ApiError).code).toBe("network");
      expect((cause as ApiError).status).toBe(0);
    }
  });
  it("distinguishes unknown write results from read failures without retrying", async () => {
    const fetchMock = vi.fn(withProtocol(async (_input: RequestInfo | URL, _init?: RequestInit) => { throw new TypeError("offline"); }));
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/write", { method: "POST", body: "{}" })).rejects.toMatchObject({ code: "network", message: expect.stringContaining("kan være gennemført") });
    await expect(request("/api/plan", { method: "POST", body: "{}", readOnly: true })).rejects.toMatchObject({ code: "network", message: expect.stringContaining("prøv igen") });
    expect(fetchMock).toHaveBeenCalledTimes(4);
    expect(fetchMock.mock.calls[3]?.[1]).not.toHaveProperty("readOnly");
  });
});


describe("current company access refusal", () => {
  it("shows a useful permission message for native read and binary failures without expiring the account", async () => {
    const expired = vi.fn();
    window.addEventListener("rentemester:auth-expired", expired);
    const fetchMock = vi.fn(withProtocol(async () => new Response(JSON.stringify({ ok: false, code: "access_denied", errors: ["fenced company transaction refused; inspect stage and connection reuse status"] }), { status: 403 })));
    stubGlobal("fetch", fetchMock);
    try {
      for (const invoke of [() => request("/api/companies/synthetic/journal"), () => requestBlob("/api/companies/synthetic/journal/export?format=pdf")]) {
        await expect(invoke()).rejects.toMatchObject({ code: "access_denied", status: 403, message: "Du har ikke adgang til denne handling. Bed workspace-ejeren kontrollere dine rettigheder." });
      }
      expect(fetchMock).toHaveBeenCalledTimes(4);
      expect(expired).not.toHaveBeenCalled();
    } finally { window.removeEventListener("rentemester:auth-expired", expired); }
  });
  it("preserves other policy guidance instead of treating every403 as company revocation", async () => {
    stubGlobal("fetch", vi.fn(withProtocol(async () => new Response(JSON.stringify({ ok: false, code: "mfa_required", errors: ["Bekræft din identitet igen."] }), { status: 403 }))));
    await expect(request("/api/account")).rejects.toMatchObject({ code: "mfa_required", status: 403, message: "Bekræft din identitet igen." });
  });
});


describe("oversized document upload refusal", () => {
  it("explains the native size refusal and does not retry the write", async () => {
    const fetchMock = vi.fn(withProtocol(
      async () => new Response(JSON.stringify({ ok: false, code: "upload_too_large" }), { status: 413 }),
      [["POST", "/api/companies/{company}/documents/ingest"]],
    ));
    stubGlobal("fetch", fetchMock);
    await expect(request("/api/companies/synthetic/documents/ingest", { method: "POST", body: "{}" })).rejects.toMatchObject({
      code: "upload_too_large", status: 413, message: "Filen er for stor. Vælg en mindre fil, og prøv igen.",
    });
    expect(fetchMock.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
  });

  it("retains unknown-outcome protection for an unrelated server failure", async () => {
    stubGlobal("fetch", vi.fn(withProtocol(
      async () => new Response(JSON.stringify({ ok: false, code: "internal" }), { status: 500 }),
      [["POST", "/api/companies/{company}/documents/ingest"]],
    )));
    await expect(request("/api/companies/synthetic/documents/ingest", { method: "POST", body: "{}" })).rejects.toMatchObject({ code: "internal", status: 500, message: "Ukendt serverfejl." });
  });
});
