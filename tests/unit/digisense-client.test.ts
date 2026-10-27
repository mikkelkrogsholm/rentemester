// Tests: src/core/efaktura/digisense-client.ts
//
// INGEN rigtige netkald: klienten injiceres med en fake HttpFetch der både
// optager den udgående request (sti, header, query, body) og returnerer et
// forudbestemt svar. Samme injection-mønster som PeppolTransmitter i
// public-einvoice.test.ts.
import { describe, expect, test } from "bun:test";
import {
  createDigisenseClient,
  digisenseBaseUrl,
  type HttpFetch,
  type HttpRequest,
} from "../../src/core/efaktura/digisense-client";

// Bygger en fake transport: optager hver request og svarer med (status, body).
function recordingFetch(
  status: number,
  body: string,
): { fetch: HttpFetch; requests: HttpRequest[] } {
  const requests: HttpRequest[] = [];
  const fetch: HttpFetch = async (request) => {
    requests.push(request);
    return { status, text: async () => body };
  };
  return { fetch, requests };
}

const LICENSE_KEY = "lic-test-key-123";

function clientWith(status: number, body: string, environment: "production" | "test" = "test") {
  const { fetch, requests } = recordingFetch(status, body);
  const client = createDigisenseClient({ apiLicenseKey: LICENSE_KEY, environment }, { fetch });
  return { client, requests };
}

describe("digisense base-url switch", () => {
  test("prod vs test base-url", () => {
    expect(digisenseBaseUrl("production")).toBe("https://api.digisense.dk/ap/api/rest");
    expect(digisenseBaseUrl("test")).toBe("https://test-api.digisense.dk/ap/api/rest");
  });

  test("factory rejects an empty license-key", () => {
    expect(() => createDigisenseClient({ apiLicenseKey: "  ", environment: "test" })).toThrow();
  });
});

describe("digisense client — auth + headers", () => {
  test("Authorization header carries the license-key on every call", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({ apiLicenseKey: LICENSE_KEY, label: "L", signatureSecret: "uuid", testGlnNumber: "5790", companyKeyConstraint: null }));
    const result = await client.validateAuth();
    expect(result.ok).toBe(true);
    expect(requests[0]!.headers.Authorization).toBe(LICENSE_KEY);
    expect(requests[0]!.method).toBe("GET");
    expect(requests[0]!.url).toBe("https://test-api.digisense.dk/ap/api/rest/validate-auth");
    if (result.ok) {
      expect(result.data.signatureSecret).toBe("uuid");
      expect(result.data.companyKeyConstraint).toBeNull();
    }
  });

  test("production environment hits the prod host", async () => {
    const { client, requests } = clientWith(200, "{}", "production");
    await client.validateAuth();
    expect(requests[0]!.url).toBe("https://api.digisense.dk/ap/api/rest/validate-auth");
  });
});

describe("digisense client — register-company", () => {
  test("POSTs JSON body and parses companyKey", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({ companyKey: "ck-abc", message: "ok" }));
    const result = await client.registerCompany({
      companyType: { type: "DK:CVR", id: "DK12345678" },
      companyName: "Min Virksomhed ApS",
    });
    expect(requests[0]!.method).toBe("POST");
    expect(requests[0]!.url).toBe("https://test-api.digisense.dk/ap/api/rest/register-company");
    expect(requests[0]!.headers["Content-Type"]).toBe("application/json");
    expect(JSON.parse(requests[0]!.body!)).toEqual({
      companyType: { type: "DK:CVR", id: "DK12345678" },
      companyName: "Min Virksomhed ApS",
    });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.data.companyKey).toBe("ck-abc");
  });
});

describe("digisense client — register-participant", () => {
  test("targets the network in the path and sends webhookUrl=null", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({ registeredOnNetwork: true, webhookRegistered: false }));
    const result = await client.registerParticipant("nemhandel", {
      direction: "inbound",
      participantType: "DK:CVR",
      participantId: "DK12345678",
      companyKey: "ck-abc",
      webhookUrl: null,
      documentProfiles: "default-nemhandel",
    });
    expect(requests[0]!.url).toBe("https://test-api.digisense.dk/ap/api/rest/v2/register-participant/nemhandel");
    expect(JSON.parse(requests[0]!.body!).webhookUrl).toBeNull();
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.registeredOnNetwork).toBe(true);
      expect(result.data.webhookRegistered).toBe(false);
    }
  });
});

describe("digisense client — validate-document", () => {
  test("sends raw XML body with application/xml and parses schematron errors", async () => {
    const xml = '<?xml version="1.0"?><Invoice/>';
    const { client, requests } = clientWith(200, JSON.stringify({
      statusCode: 200,
      success: false,
      errors: [{ context: "c", pattern: "p", description: "d", xpath: "/x", type: "error" }],
    }));
    const result = await client.validateDocument(xml);
    expect(requests[0]!.method).toBe("POST");
    expect(requests[0]!.url).toBe("https://test-api.digisense.dk/ap/api/rest/validate-document");
    expect(requests[0]!.headers["Content-Type"]).toBe("application/xml");
    expect(requests[0]!.body).toBe(xml);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.success).toBe(false);
      expect(result.data.errors[0]!.type).toBe("error");
    }
  });
});

describe("digisense client — lookup-participant", () => {
  test("reads the authenticated license's test registrations without a company selector", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({ participants: [] }));
    const result = await client.listParticipantsForCompany!();
    expect(requests[0]!.method).toBe("GET");
    expect(requests[0]!.url).toBe("https://test-api.digisense.dk/ap/api/rest/list-participants-for-company");
    expect(requests[0]!.headers.Authorization).toBe(LICENSE_KEY);
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.data.participants).toEqual([]);
  });
  test("encodes the query string", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({
      participantAbleToReceive: true,
      matchedServiceEndpointUrls: ["https://ap.example/x"],
      matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "Invoice" }],
    }));
    const result = await client.lookupParticipant({
      networkType: "Nemhandel",
      participantScheme: "DK:CVR",
      participantValue: "DK12345678",
    });
    expect(requests[0]!.method).toBe("GET");
    expect(requests[0]!.url).toBe(
      "https://test-api.digisense.dk/ap/api/rest/lookup-participant?networkType=Nemhandel&participantScheme=DK%3ACVR&participantValue=DK12345678",
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.participantAbleToReceive).toBe(true);
      expect(result.data.matchedDocumentTypes).toEqual([{ scheme: "busdox-docid-qns", value: "Invoice" }]);
    }
  });
});

describe("digisense client — deliver-document", () => {
  test("uses the official KSeF values PROD, TEST and DEMO", async () => {
    for (const environment of ["PROD", "TEST", "DEMO"] as const) {
      const { client, requests } = clientWith(200, JSON.stringify({ statusCode: 200, documentStatus: "delivered", documentId: "doc", message: "ok", publicUrl: "" }));
      await client.deliverDocument("<Invoice/>", { companyKey: "ck", ksefEnvironment: environment });
      expect(requests[0]!.url).toContain(`ksefEnvironment=${environment}`);
    }
  });
  test("POSTs XML with companyKey + ksefEnvironment in the query", async () => {
    const xml = "<Invoice>payload</Invoice>";
    const { client, requests } = clientWith(202, JSON.stringify({
      statusCode: 202,
      documentStatus: "queued-for-delivery",
      documentId: "doc-1",
      message: "queued",
      publicUrl: "https://pub/doc-1",
    }));
    const result = await client.deliverDocument(xml, { companyKey: "ck-abc", ksefEnvironment: "TEST" });
    expect(requests[0]!.method).toBe("POST");
    expect(requests[0]!.url).toBe(
      "https://test-api.digisense.dk/ap/api/rest/deliver-document?companyKey=ck-abc&ksefEnvironment=TEST",
    );
    expect(requests[0]!.headers["Content-Type"]).toBe("application/xml");
    expect(requests[0]!.body).toBe(xml);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.status).toBe(202);
      expect(result.data.documentStatus).toBe("queued-for-delivery");
    }
  });
});

describe("digisense client — document-status", () => {
  test("puts documentId in path and companyKey in query", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({
      statusCode: 200,
      documentStatus: "delivered",
      documentId: "doc-1",
      message: "delivered",
      publicUrl: "https://pub/doc-1",
    }));
    const result = await client.documentStatus("doc-1", "ck-abc");
    expect(requests[0]!.url).toBe(
      "https://test-api.digisense.dk/ap/api/rest/document-status/doc-1?companyKey=ck-abc",
    );
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.data.documentStatus).toBe("delivered");
  });
});

describe("digisense client — list-received-documents", () => {
  test("builds the query and parses the documents array with internalId", async () => {
    const { client, requests } = clientWith(200, JSON.stringify({
      documents: [{
        documentId: "d1",
        documentType: "Invoice",
        documentTypeUrn: "urn:...:Invoice-2",
        profileId: "p",
        customizationId: "c",
        receivedAt: "2026-06-20T10:00:00Z",
        senderParticipantType: "DK:CVR",
        senderParticipantId: "DK98765432",
        senderName: "Leverandør ApS",
        destinationParticipantType: "DK:CVR",
        destinationParticipantId: "DK12345678",
        destinationName: "Min Virksomhed ApS",
        sourceNetwork: "nemhandel",
        internalId: "stable-dedup-key-1",
        downloadUrlExpiresAt: "2026-06-20T11:00:00Z",
        downloadUrl: "https://signed/d1",
      }],
      participants: [],
      licenseKey: LICENSE_KEY,
      searchParams: {},
      nextPageUrl: null,
    }));
    const result = await client.listReceivedDocuments({ limit: 50, companyKey: "ck-abc", networkType: "nemhandel" });
    expect(requests[0]!.url).toBe(
      "https://test-api.digisense.dk/ap/api/rest/list-received-documents?limit=50&companyKey=ck-abc&networkType=nemhandel",
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.documents[0]!.internalId).toBe("stable-dedup-key-1");
      expect(result.data.nextPageUrl).toBeNull();
    }
  });
});

describe("digisense client — error handling", () => {
  test("non-2xx maps to a typed error result (not a throw)", async () => {
    const { client } = clientWith(401, "unauthorized");
    const result = await client.validateAuth();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.status).toBe(401);
      expect(result.error.body).toBe("unauthorized");
      expect(result.error.message).toContain("401");
    }
  });

  test("transport exception is captured as status 0", async () => {
    const fetch: HttpFetch = async () => {
      throw new Error("socket reset");
    };
    const client = createDigisenseClient({ apiLicenseKey: LICENSE_KEY, environment: "test" }, { fetch });
    const result = await client.validateAuth();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.status).toBe(0);
      expect(result.error.message).toContain("socket reset");
    }
  });

  test("unparseable 2xx body maps to an error result", async () => {
    const { client } = clientWith(200, "<<not json>>");
    const result = await client.validateAuth();
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.message).toContain("unparseable");
  });
});
