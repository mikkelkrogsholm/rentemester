// Strict fake: all requests are intercepted; unknown routes never reach a network.
export function syntheticDigisenseFetch(record: (route: string, body: string) => void) {
  return async (input: string | URL | Request, init?: RequestInit) => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    const body = typeof init?.body === "string" ? init.body : "";
    if (url.hostname !== "test-api.digisense.dk") throw new Error("Unexpected network host in synthetic test");
    record(url.pathname, body);
    if (url.pathname.endsWith("/validate-document")) return Response.json({ statusCode: 200, success: true, errors: [] });
    if (url.pathname.endsWith("/lookup-participant")) return Response.json({ participantAbleToReceive: true, matchedServiceEndpointUrls: ["https://example.invalid"], matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.1::2.1" }] });
    if (url.pathname.includes("/document-status/")) return Response.json({ statusCode: 200, documentStatus: "delivered", documentId: "synthetic-queued", message: "ok" });
    if (url.pathname.endsWith("/deliver-document")) return Response.json({ statusCode: 200, documentStatus: "delivered", documentId: "synthetic-document", message: "ok" });
    throw new Error(`Unexpected synthetic request: ${url.pathname}`);
  };
}
