import { describe, expect, test, vi } from "bun:test";
import { documentsApi } from "./documents";
import { request } from "./_shared";
import { mockFetch } from "../../test/fixtures";
import { stubGlobal } from "../../test/globals";

const path = "/api/companies/acme-aps/expense/original/vat-preflight/1";
const snapshot = (ok = false) => ({
  ok, documentId: 1, classification: "EU" as const, cached: ok, evidenceExpiresAt: ok ? "2026-10-25T00:00:00Z" : null,
  wouldCallProvider: !ok, errors: ok ? [] : ["fresh EU VAT validation evidence is required"], derivedRegion: "EU" as const,
  requiredValidation: "EU VAT provider validation", cache: { reused: ok, freshUntil: ok ? "2026-10-25T00:00:00Z" : null },
  applyWouldCallProvider: !ok, evidence: [], exception: null,
});
function setup(value: unknown, status = 200, raw = false) {
  mockFetch({ [`GET ${path}`]: snapshot(), [`POST ${path}`]: snapshot() });
  const original = globalThis.fetch;
  const spy = vi.fn((input: RequestInfo | URL, init?: RequestInit) => String(input) === path
    ? Promise.resolve(new Response(raw ? String(value) : JSON.stringify(value), { status })) : original(input, init));
  stubGlobal("fetch", spy);
  return spy;
}

describe("native purchase VAT preflight", () => {
  for (const ok of [true, false]) test(`flat ${ok} GET is an inert validated snapshot`, async () => {
    const value = snapshot(ok), spy = setup(value);
    expect(await documentsApi.nativeDocumentVatPreflight("acme-aps", 1)).toEqual(value);
    const calls = spy.mock.calls.filter(c => String(c[0]) === path);
    expect(calls).toHaveLength(1);
    expect(calls[0]![1]?.method).toBeUndefined();
    expect(calls[0]![1]?.body).toBeUndefined();
    expect(calls[0]![1]).not.toHaveProperty("acceptDomainFalse");
  });
  test("explicit confirmation maps to one exact POST; source false with exception completes", async () => {
    const value = { ...snapshot(), exception: { id: 2, status: "open", severity: "high", message: "Provider unavailable", requiredAction: "Review supplier evidence and retry explicitly", createdAt: "2026-10-24" } };
    const spy = setup(value);
    expect(await documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, { confirm: true })).toEqual(value);
    const calls = spy.mock.calls.filter(c => String(c[0]) === path);
    expect(calls).toHaveLength(1);
    expect(calls[0]![1]?.method).toBe("POST");
    expect(calls[0]![1]?.body).toBe('{"confirm":true}');
  });
  test("missing, false or extra confirmation fields never dispatch", async () => {
    const spy = setup(snapshot());
    for (const confirmation of [undefined, { confirm: false }, { confirm: true, expectedActor: "forged" }]) {
      await expect(documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, confirmation as { confirm: true })).rejects.toThrow();
    }
    expect(spy).not.toHaveBeenCalled();
  });
  const invalid = [
    { ...snapshot(true), documentId: 2 }, { ...snapshot(true), documentId: Number.MAX_SAFE_INTEGER + 1 },
    { ...snapshot(true), classification: "DK" }, { ...snapshot(true), ok: false }, { ...snapshot(), cache: { reused: true, freshUntil: null } },
    { ...snapshot(), wouldCallProvider: false }, { ...snapshot(), requiredValidation: 1 },
    { ...snapshot(), evidence: Array(21).fill({}) }, { ...snapshot(), evidence: [{ id: 1, event_type: "provider_result", classification: "EU", provider_status: "valid", evidence_expires_at: null, created_at: "now", actor: null, rawResponse: "private" }] },
    { ...snapshot(), exception: { id: -1, status: "open", severity: "high", message: "x", requiredAction: null, createdAt: "now" } },
    { ...snapshot(), privateBody: {} }, { ok: true, preflight: snapshot(true) },
  ];
  invalid.forEach((value, index) => test(`malformed source ${index + 1} remains unknown without fallback`, async () => {
    const spy = setup(value);
    await expect(documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, { confirm: true })).rejects.toMatchObject({ code: "internal" });
    expect(spy.mock.calls.filter(c => String(c[0]) === path)).toHaveLength(1);
    expect(spy.mock.calls.some(c => String(c[0]).includes("/documents/"))).toBe(false);
  }));
  for (const status of [401, 403, 422, 500]) test(`HTTP ${status} cannot be accepted as a source false`, async () => {
    setup(snapshot(), status);
    await expect(documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, { confirm: true })).rejects.toMatchObject({ status });
  });
  test("truncated JSON remains unknown", async () => {
    setup('{"ok":true', 200, true);
    await expect(documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, { confirm: true })).rejects.toMatchObject({ code: "internal" });
  });
  test("network loss remains unknown", async () => {
    const original = setup(snapshot());
    const fetch = globalThis.fetch;
    stubGlobal("fetch", vi.fn((input: RequestInfo | URL, init?: RequestInit) => String(input) === path ? Promise.reject(new TypeError("lost")) : fetch(input, init)));
    await expect(documentsApi.applyNativeDocumentVatPreflight("acme-aps", 1, { confirm: true })).rejects.toMatchObject({ code: "network" });
    expect(original.mock.calls.some(c => String(c[0]).includes("/documents/"))).toBe(false);
  });
  test("default transport still rejects domain false", async () => {
    setup(snapshot());
    await expect(request(path)).rejects.toMatchObject({ code: "internal" });
  });
  test("legacy envelope/routes are retained", async () => {
    const legacy = "/api/companies/acme-aps/documents/1/vat-preflight";
    mockFetch({ [`GET ${legacy}`]: { preflight: snapshot() }, [`POST ${legacy}/apply`]: { preflight: snapshot(true) } });
    expect(await documentsApi.documentVatPreflight("acme-aps", 1)).toEqual(snapshot());
    expect(await documentsApi.applyDocumentVatPreflight("acme-aps", 1)).toEqual(snapshot(true));
  });
});
