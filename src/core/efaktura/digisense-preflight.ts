import { normalizeEanNumber } from "../ean";
import type { DigisenseClient } from "./digisense-client";

// DigiSense documents a 10 MB maximum for deliver-document. Use the lower
// decimal interpretation so a preview never approves an oversized request.
const MAX_DOCUMENT_BYTES = 10_000_000;
const OIOUBL_INVOICE_TYPE = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.02::2.0";

/**
 * Check the exact XML with DigiSense and check recipient reachability without
 * transmitting it. A successful result is a preflight, not a delivery receipt
 * or proof that the recipient will retain or display an embedded attachment.
 */
export async function checkDigisenseInvoiceReadiness(
  client: Pick<DigisenseClient, "validateDocument" | "lookupParticipant"> & Partial<Pick<DigisenseClient, "validateAuth" | "listParticipantsForCompany">>,
  xml: string,
  recipientEan: string,
  environment: "test" | "production" = "production",
): Promise<{ ok: true } | { ok: false; error: string }> {
  const ean = normalizeEanNumber(recipientEan);
  if (!ean) return { ok: false, error: "A valid recipient EAN is required" };
  if (!xml || Buffer.byteLength(xml, "utf8") > MAX_DOCUMENT_BYTES) {
    return { ok: false, error: "Invoice XML is empty or exceeds DigiSense's 10 MB limit" };
  }
  const validation = await client.validateDocument(xml);
  if (!validation.ok || validation.data?.success !== true || validation.data.statusCode !== 200 ||
      !Array.isArray(validation.data.errors) || validation.data.errors.some((item) => item?.type !== "warning")) {
    return { ok: false, error: "DigiSense could not validate the invoice XML" };
  }
  const recipient = await client.lookupParticipant({
    networkType: "Nemhandel", documentType: OIOUBL_INVOICE_TYPE, participantScheme: "GLN", participantValue: ean,
  });
  // The test service's public participant lookup queries the live registry and
  // can return 404 for its own registered sandbox GLN. Only that license's
  // explicitly assigned test GLN may use the test-only registration list.
  if (!recipient.ok && recipient.error.status === 404 && environment === "test" && client.validateAuth && client.listParticipantsForCompany) {
    const auth = await client.validateAuth();
    if (auth.ok && normalizeEanNumber(auth.data.testGlnNumber) === ean) {
      const registered = await client.listParticipantsForCompany();
      if (registered.ok && Array.isArray(registered.data?.participants) && registered.data.participants.some((p) =>
        p.networkType === "nemhandel" && p.direction === "inbound" && p.participantType === "GLN" &&
        p.participantId === ean && typeof p.companyKey === "string" && p.companyKey.length > 0 &&
        (p.documentProfiles === "default-nemhandel" || p.documentProfiles === "default-nemhandel-with-order-only"))) return { ok: true };
    }
  }
  if (!recipient.ok) {
    const status = recipient.error.status;
    return { ok: false, error: status === 404
      ? "DigiSense recipient lookup returned HTTP 404 for the supplied GLN and OIOUBL 2.02 invoice profile; confirm the registration and profile before sending"
      : `DigiSense recipient lookup failed${typeof status === "number" ? ` (HTTP ${status})` : ""}; recipient capability could not be confirmed` };
  }
  if (recipient.data?.participantAbleToReceive !== true ||
      !Array.isArray(recipient.data.matchedServiceEndpointUrls) ||
      !recipient.data.matchedServiceEndpointUrls.some((url) => typeof url === "string" && url.trim().length > 0) ||
      !Array.isArray(recipient.data.matchedDocumentTypes) ||
      !recipient.data.matchedDocumentTypes.some((type) => type?.scheme === "busdox-docid-qns" && type.value === OIOUBL_INVOICE_TYPE)) {
    return { ok: false, error: "DigiSense could not confirm that the recipient can receive an invoice" };
  }
  return { ok: true };
}
