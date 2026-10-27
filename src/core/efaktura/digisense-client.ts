// Digisense e-faktura REST-klient (#efaktura) — fundamentet for send/modtag
// via NemHandel + Peppol access point hos Digisense.
//
// Kilde: https://api.digisense.dk/ap/api/rest/openapi-spec.json (OpenAPI 3.1).
// Klienten er DETERMINISTISK og INJICÉRBAR: den tager en fetch-lignende
// afhængighed ind (`HttpFetch`), så unit-tests kan levere en fake uden netværk —
// præcis samme trust-boundary og injection-mønster som `PeppolTransmitter` i
// src/core/public-einvoice.ts. Den rigtige klient wires i CLI/MCP, men køres
// aldrig i tests.
//
// TRUST-BOUNDARY: API license-key er ÉN nøgle for hele licensen og lever ALDRIG
// i ledger-DB'en. Den hentes fra config/secret-laget (digisense-config.ts) og
// sendes i `Authorization`-headeren. companyKey scoper næsten alle kald og er
// derimod almindelig (ikke-secret) state i ledgeren.

// ============================================================================
// Base-URL / environment
// ============================================================================

/** Digisense-miljø: prod vs sandbox. Bestemmer base-URL. */
export type DigisenseEnvironment = "production" | "test";

// Servere fra OpenAPI-spec'en. Base path er fælles for begge.
const PRODUCTION_BASE_URL = "https://api.digisense.dk";
const TEST_BASE_URL = "https://test-api.digisense.dk";
const BASE_PATH = "/ap/api/rest";

export function digisenseBaseUrl(environment: DigisenseEnvironment): string {
  return `${environment === "production" ? PRODUCTION_BASE_URL : TEST_BASE_URL}${BASE_PATH}`;
}

// ============================================================================
// Config + injicérbar fetch
// ============================================================================

export type DigisenseClientConfig = {
  /** API license-key — sendes i Authorization-header. Aldrig i ledgeren. */
  apiLicenseKey: string;
  /** prod/test base-URL switch. */
  environment: DigisenseEnvironment;
};

// Minimal fetch-lignende kontrakt. Holder klienten fri af DOM/Bun-globals så
// tests kan levere en fake der returnerer et forudbestemt svar uden netværk.
export type HttpRequest = {
  method: "GET" | "POST";
  url: string;
  headers: Record<string, string>;
  body?: string;
};

export type HttpResponse = {
  status: number;
  /** Rå svar-body (JSON eller XML som tekst). */
  text(): Promise<string>;
};

export type HttpFetch = (request: HttpRequest) => Promise<HttpResponse>;

export type DigisenseClientDeps = {
  /** Injicérbar transport. Default wrapper om global fetch når udeladt. */
  fetch?: HttpFetch;
};

// Default-transporten wrapper den globale fetch. Bruges KUN i produktion —
// tests injicerer altid en fake, så denne sti rammes aldrig i bun test.
function defaultHttpFetch(): HttpFetch {
  return async (request) => {
    const response = await fetch(request.url, {
      method: request.method,
      headers: request.headers,
      body: request.body,
    });
    return {
      status: response.status,
      text: () => response.text(),
    };
  };
}

// ============================================================================
// Fejl-håndtering: ikke-2xx mappes til et tydeligt Result/throw der senere kan
// oversættes til en envelope. Vi følger Result-mønstret fra public-einvoice.ts:
// kaldere får et { ok } discriminated union i stedet for at skulle fange throws.
// ============================================================================

export type DigisenseError = {
  /** HTTP-status fra access point (eller 0 ved transport-/parse-fejl). */
  status: number;
  /** Kort, sikker besked egnet til at lægge i en envelope. */
  message: string;
  /** Rå svar-body, hvis tilgængelig (til diagnostik/audit). */
  body?: string;
};

export type DigisenseResult<T> =
  | { ok: true; status: number; data: T }
  | { ok: false; error: DigisenseError };

// ============================================================================
// Endpoint request/response-typer (følger API-kontrakten 1:1)
// ============================================================================

// --- validate-auth ---------------------------------------------------------
export type ValidateAuthResponse = {
  apiLicenseKey: string;
  label: string;
  /** UUID — HMAC-secret til webhook-signaturer. */
  signatureSecret: string;
  testGlnNumber: string;
  /** Begrænsning på companyKey, eller null hvis ubegrænset. */
  companyKeyConstraint: string | null;
};

// --- register-company ------------------------------------------------------
export type DigisenseCompanyType =
  | { type: "DK:CVR"; id: string }
  | { type: "NIP"; id: string };

export type RegisterCompanyRequest = {
  companyType: DigisenseCompanyType;
  companyName: string;
};

export type RegisterCompanyResponse = {
  /** base64url — scoper næsten alle efterfølgende kald. */
  companyKey: string;
  message: string;
};

// --- register-participant/{network} ----------------------------------------
export type DigisenseNetwork = "nemhandel" | "peppol";
export type ParticipantDirection = "inbound" | "outbound";
export type ParticipantType = "DK:CVR" | "GLN";

export type RegisterParticipantRequest = {
  direction: ParticipantDirection;
  participantType: ParticipantType;
  participantId: string;
  companyKey: string;
  /** null => ingen webhook; man poller selv (bekræftet designvalg). */
  webhookUrl: string | null;
  documentProfiles: string;
};

export type RegisterParticipantResponse = {
  registeredOnNetwork: boolean;
  webhookRegistered: boolean;
};

// --- validate-document (schematron) ----------------------------------------
export type ValidateDocumentError = {
  context: string;
  pattern: string;
  description: string;
  xpath: string | null;
  type: "error" | "warning";
};

export type ValidateDocumentResponse = {
  statusCode: number;
  success: boolean;
  errors: ValidateDocumentError[];
};

// --- lookup-participant ----------------------------------------------------
export type LookupNetworkType = "PEPPOL" | "Nemhandel";

export type LookupParticipantQuery = {
  networkType: LookupNetworkType;
  documentType?: string;
  /** fx "DK:CVR", "0184", "0088", "GLN". */
  participantScheme: string;
  participantValue: string;
};

export type LookupParticipantResponse = {
  participantAbleToReceive: boolean;
  matchedServiceEndpointUrls: string[];
  matchedDocumentTypes: Array<{ scheme: string; value: string }>;
};

// --- deliver-document ------------------------------------------------------
/** KSeF's documented environment values (not our config labels). */
export type KsefEnvironment = "PROD" | "TEST" | "DEMO";

export type DeliverDocumentQuery = {
  companyKey: string;
  ksefEnvironment?: KsefEnvironment;
};

export type DigisenseDocumentStatus =
  | "delivered"
  | "queued-for-delivery"
  | "document-not-valid"
  | "unable-to-deliver"
  | "unknown-server-error"
  | "temporary-upstream-error";

export type DeliverDocumentResponse = {
  /** 200 delivered | 202 queued | 4xx/5xx. */
  statusCode: number;
  documentStatus: DigisenseDocumentStatus;
  documentId: string;
  message: string;
  publicUrl: string;
};

// --- document-status/{documentId} ------------------------------------------
export type DocumentStatusResponse = {
  statusCode: number;
  documentStatus: DigisenseDocumentStatus;
  documentId: string;
  message: string;
  publicUrl: string;
};

// --- list-received-documents -----------------------------------------------
export type ListReceivedDocumentsQuery = {
  /** <=100. */
  limit: number;
  offset?: number;
  maxTimestamp?: string;
  companyKey?: string;
  networkType?: string;
  participantType?: ParticipantType;
  participantId?: string;
};

export type ReceivedDocument = {
  documentId: string;
  documentType: string;
  documentTypeUrn: string;
  profileId: string;
  customizationId: string;
  receivedAt: string;
  senderParticipantType: string;
  senderParticipantId: string;
  senderName: string;
  destinationParticipantType: string;
  destinationParticipantId: string;
  destinationName: string;
  sourceNetwork: string;
  /** STABIL dedup-nøgle. */
  internalId: string;
  downloadUrlExpiresAt: string;
  /** Signeret URL, udløber. */
  downloadUrl: string;
};

export type ListReceivedDocumentsResponse = {
  documents: ReceivedDocument[];
  participants: unknown[];
  licenseKey: string;
  searchParams: Record<string, unknown>;
  nextPageUrl: string | null;
};

// ============================================================================
// Klient-interface + factory
// ============================================================================

export type DigisenseClient = {
  validateAuth(): Promise<DigisenseResult<ValidateAuthResponse>>;
  registerCompany(
    body: RegisterCompanyRequest,
  ): Promise<DigisenseResult<RegisterCompanyResponse>>;
  registerParticipant(
    network: DigisenseNetwork,
    body: RegisterParticipantRequest,
  ): Promise<DigisenseResult<RegisterParticipantResponse>>;
  /** Schematron-validér FØR send. XML-body som string. */
  validateDocument(
    xml: string,
  ): Promise<DigisenseResult<ValidateDocumentResponse>>;
  lookupParticipant(
    query: LookupParticipantQuery,
  ): Promise<DigisenseResult<LookupParticipantResponse>>;
  /** Lever XML-faktura. XML-body som string. 202 => poll status. */
  deliverDocument(
    xml: string,
    query: DeliverDocumentQuery,
  ): Promise<DigisenseResult<DeliverDocumentResponse>>;
  documentStatus(
    documentId: string,
    companyKey: string,
  ): Promise<DigisenseResult<DocumentStatusResponse>>;
  listReceivedDocuments(
    query: ListReceivedDocumentsQuery,
  ): Promise<DigisenseResult<ListReceivedDocumentsResponse>>;
};

// Bygger en query-streng fra et objekt — udelader undefined/null felter og
// URL-encoder værdier. Tom (ingen felter) => tom streng (intet "?").
function buildQuery(params: Record<string, unknown>): string {
  const pairs: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") continue;
    pairs.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`);
  }
  return pairs.length > 0 ? `?${pairs.join("&")}` : "";
}

export function createDigisenseClient(
  config: DigisenseClientConfig,
  deps: DigisenseClientDeps = {},
): DigisenseClient {
  if (!config.apiLicenseKey?.trim()) {
    throw new Error("digisense client: apiLicenseKey is required");
  }
  const baseUrl = digisenseBaseUrl(config.environment);
  const httpFetch = deps.fetch ?? defaultHttpFetch();

  // Fælles headers. Authorization bærer license-key'en (én nøgle pr. licens).
  function authHeaders(contentType?: string): Record<string, string> {
    const headers: Record<string, string> = {
      Authorization: config.apiLicenseKey,
      Accept: "application/json",
    };
    if (contentType) headers["Content-Type"] = contentType;
    return headers;
  }

  // Udfør et kald og parse JSON-svaret. Ikke-2xx mappes til en tydelig
  // DigisenseError; transport-/parse-fejl bliver status 0 så kaldere altid får
  // et Result i stedet for en throw der lækker stack-traces til envelopen.
  async function requestJson<T>(request: HttpRequest): Promise<DigisenseResult<T>> {
    let response: HttpResponse;
    try {
      response = await httpFetch(request);
    } catch (error) {
      return {
        ok: false,
        error: {
          status: 0,
          message: `digisense transport error: ${error instanceof Error ? error.message : String(error)}`,
        },
      };
    }

    const rawBody = await response.text();

    if (response.status < 200 || response.status >= 300) {
      return {
        ok: false,
        error: {
          status: response.status,
          message: `digisense responded ${response.status}`,
          body: rawBody,
        },
      };
    }

    if (rawBody.trim().length === 0) {
      // Tomt 2xx-svar => tomt objekt (fx endpoints der kun signalerer succes).
      return { ok: true, status: response.status, data: {} as T };
    }

    try {
      return { ok: true, status: response.status, data: JSON.parse(rawBody) as T };
    } catch (error) {
      return {
        ok: false,
        error: {
          status: response.status,
          message: `digisense returned unparseable JSON: ${error instanceof Error ? error.message : String(error)}`,
          body: rawBody,
        },
      };
    }
  }

  return {
    validateAuth() {
      return requestJson<ValidateAuthResponse>({
        method: "GET",
        url: `${baseUrl}/validate-auth`,
        headers: authHeaders(),
      });
    },

    registerCompany(body) {
      return requestJson<RegisterCompanyResponse>({
        method: "POST",
        url: `${baseUrl}/register-company`,
        headers: authHeaders("application/json"),
        body: JSON.stringify(body),
      });
    },

    registerParticipant(network, body) {
      return requestJson<RegisterParticipantResponse>({
        method: "POST",
        // v1 is deprecated. v2 remains under the configured REST base URL.
        url: `${baseUrl}/v2/register-participant/${network}`,
        headers: authHeaders("application/json"),
        body: JSON.stringify(body),
      });
    },

    validateDocument(xml) {
      // Schematron-validering: XML-body som string, application/xml.
      return requestJson<ValidateDocumentResponse>({
        method: "POST",
        url: `${baseUrl}/validate-document`,
        headers: authHeaders("application/xml"),
        body: xml,
      });
    },

    lookupParticipant(query) {
      return requestJson<LookupParticipantResponse>({
        method: "GET",
        url: `${baseUrl}/lookup-participant${buildQuery({
          networkType: query.networkType,
          documentType: query.documentType,
          participantScheme: query.participantScheme,
          participantValue: query.participantValue,
        })}`,
        headers: authHeaders(),
      });
    },

    deliverDocument(xml, query) {
      // XML-faktura (<=10MB). companyKey + valgfrit ksefEnvironment i query.
      return requestJson<DeliverDocumentResponse>({
        method: "POST",
        url: `${baseUrl}/deliver-document${buildQuery({
          companyKey: query.companyKey,
          ksefEnvironment: query.ksefEnvironment,
        })}`,
        headers: authHeaders("application/xml"),
        body: xml,
      });
    },

    documentStatus(documentId, companyKey) {
      return requestJson<DocumentStatusResponse>({
        method: "GET",
        url: `${baseUrl}/document-status/${encodeURIComponent(documentId)}${buildQuery({ companyKey })}`,
        headers: authHeaders(),
      });
    },

    listReceivedDocuments(query) {
      return requestJson<ListReceivedDocumentsResponse>({
        method: "GET",
        url: `${baseUrl}/list-received-documents${buildQuery({
          limit: query.limit,
          offset: query.offset,
          maxTimestamp: query.maxTimestamp,
          companyKey: query.companyKey,
          networkType: query.networkType,
          participantType: query.participantType,
          participantId: query.participantId,
        })}`,
        headers: authHeaders(),
      });
    },
  };
}
