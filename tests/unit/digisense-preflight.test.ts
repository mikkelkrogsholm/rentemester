import { describe, expect, test } from "bun:test";
import { checkDigisenseInvoiceReadiness } from "../../src/core/efaktura/digisense-preflight";
import type { DigisenseClient } from "../../src/core/efaktura/digisense-client";
const OIOUBL = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.1::2.1";

function client(options: { valid?: boolean; reachable?: boolean } = {}) {
  const calls: unknown[] = [];
  const stub: Pick<DigisenseClient, "validateDocument" | "lookupParticipant"> = {
    async validateDocument(xml) {
      calls.push(xml);
      return { ok: true, status: 200, data: { statusCode: 200, success: options.valid !== false, errors: [] } };
    },
    async lookupParticipant(query) {
      calls.push(query);
      return { ok: true, status: 200, data: {
        participantAbleToReceive: options.reachable !== false,
        matchedServiceEndpointUrls: ["https://example.invalid/endpoint"],
        matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: OIOUBL }],
      } };
    },
  };
  return { stub, calls };
}

describe("DigiSense invoice preflight", () => {
  test("validates the exact XML and checks the EAN without delivering", async () => {
    const { stub, calls } = client();
    expect(await checkDigisenseInvoiceReadiness(stub, "<Invoice/>", "5790000000013")).toEqual({ ok: true });
    expect(calls).toEqual(["<Invoice/>", { networkType: "Nemhandel", documentType: OIOUBL, participantScheme: "GLN", participantValue: "5790000000013" }]);
  });

  test("fails closed on invalid documents, unreachable recipients and oversized XML", async () => {
    const invalid = client({ valid: false });
    expect((await checkDigisenseInvoiceReadiness(invalid.stub, "<Invoice/>", "5790000000013")).ok).toBe(false);
    expect(invalid.calls).toHaveLength(1);
    const unreachable = client({ reachable: false });
    expect((await checkDigisenseInvoiceReadiness(unreachable.stub, "<Invoice/>", "5790000000013")).ok).toBe(false);
    const oversized = client();
    expect((await checkDigisenseInvoiceReadiness(oversized.stub, "x".repeat(10_000_001), "5790000000013")).ok).toBe(false);
    expect(oversized.calls).toHaveLength(0);
    const wrongVersion = { ...client().stub, async lookupParticipant() { return { ok: true as const, status: 200, data: { participantAbleToReceive: true, matchedServiceEndpointUrls: ["https://example.invalid"], matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.02::2.0" }] } }; } };
    expect((await checkDigisenseInvoiceReadiness(wrongVersion, "<Invoice/>", "5790000000013")).ok).toBe(false);
  });

  test("only the license's own registered test GLN can use the sandbox fallback", async () => {
    const { stub } = client();
    const testGln = "5790000000013";
    const fallback = {
      ...stub,
      async lookupParticipant() { return { ok: false as const, error: { status: 404, message: "not found" } }; },
      async validateAuth() { return { ok: true as const, status: 200, data: { apiLicenseKey: "synthetic", label: "test", signatureSecret: "synthetic", testGlnNumber: testGln, companyKeyConstraint: null } }; },
      async listParticipantsForCompany() { return { ok: true as const, status: 200, data: { participants: [{ networkType: "nemhandel", direction: "inbound", participantType: "GLN", participantId: testGln, companyKey: "synthetic", documentProfiles: "default-nemhandel" }] } }; },
    };
    expect((await checkDigisenseInvoiceReadiness(fallback, "<Invoice/>", testGln, "test")).ok).toBe(true);
    const missing = await checkDigisenseInvoiceReadiness(fallback, "<Invoice/>", testGln, "production");
    expect(missing.ok).toBe(false);
    if (!missing.ok) expect(missing.error).toContain("HTTP 404");
    expect((await checkDigisenseInvoiceReadiness(fallback, "<Invoice/>", "5790000000014", "test")).ok).toBe(false);
    const unavailable = { ...fallback, async lookupParticipant() { return { ok: false as const, error: { status: 500, message: "unavailable" } }; } };
    expect((await checkDigisenseInvoiceReadiness(unavailable, "<Invoice/>", testGln, "test")).ok).toBe(false);
    const wrongProfile = { ...fallback, async listParticipantsForCompany() { return { ok: true as const, status: 200, data: { participants: [{ networkType: "nemhandel", direction: "inbound", participantType: "GLN", participantId: testGln, companyKey: "synthetic", documentProfiles: null }] } }; } };
    expect((await checkDigisenseInvoiceReadiness(wrongProfile, "<Invoice/>", testGln, "test")).ok).toBe(false);
  });
});
