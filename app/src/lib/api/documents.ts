import type { DocumentsResponse } from "../types";
import { request } from "./_shared";

export const documentsApi = {
  documents: (slug: string) =>
    request<DocumentsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/documents`,
    ).then((r) => r.documents),

  /** #588: review-only party-link state, kept separate from invoice facts. */
  documentPartyLinks: (slug: string, status?: "linked" | "unlinked" | "resolved" | "internal_no_external_party" | "unresolved") =>
    request<{ ok: true; links: Array<{ id: number; document_no: string | null; linked: 0 | 1; resolution_state: "resolved" | "internal_no_external_party" | "unresolved" }> }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/party-links${status ? `?status=${status}` : ""}`,
    ).then((r) => r.links),

  documentPartyLinkHistory: (slug: string, documentId: number) =>
    request<{ ok: true; links: Array<Record<string, unknown>> }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/${documentId}/party-links`,
    ).then((r) => r.links),

  searchCanonicalParties: (slug: string, query: string) =>
    request<{ ok: true; rows: Array<{ partyId: string; name: string }>; count: number }>(
      `/api/companies/${encodeURIComponent(slug)}/workspace-parties?query=${encodeURIComponent(query)}`,
    ),

  planDocumentPartyLink: (slug: string, input: Record<string, unknown>) =>
    request<{ ok: boolean; plan?: { planHash: string }; errors?: string[] }>(`/api/companies/${encodeURIComponent(slug)}/documents/party-links/plan`, { method: "POST", body: JSON.stringify(input) }),

  applyDocumentPartyLink: (slug: string, input: Record<string, unknown>) =>
    request<{ ok: boolean; id?: number; errors?: string[] }>(`/api/companies/${encodeURIComponent(slug)}/documents/party-links/apply`, { method: "POST", body: JSON.stringify(input) }),

  confirmInternalNoExternalParty: (slug: string, input: Record<string, unknown>) =>
    request<{ ok: boolean; id?: number; errors?: string[] }>(`/api/companies/${encodeURIComponent(slug)}/documents/internal-no-external-party`, { method: "POST", body: JSON.stringify(input) }),

  /** #618: audited attribution separate from the source invoice and VAT gate. */
  setDocumentCompanyContext: (slug: string, input: { documentId: number; sourceReference: string; businessUseReason: string }) =>
    request<{ ok: boolean; applied?: boolean; errors?: string[] }>(`/api/companies/${encodeURIComponent(slug)}/documents/company-context`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),

  /**
   * URL of a stored bilag file — opened directly in a new browser tab, so it
   * is a plain URL builder rather than a fetch. The server serves the file
   * inline with its recorded MIME type.
   */
  documentFileUrl: (slug: string, id: number) =>
    `/api/companies/${encodeURIComponent(slug)}/documents/${id}/file`,

  /**
   * Imports an export file from another accounting system. The browser reads
   * the chosen file and passes its text as `content`; the server recognises
   * which system the file came from and routes it to the matching importer.
   * Destructive (it appends master data), so the body carries `confirm: true`.
   */
  importData: (slug: string, input: DataImportInput) =>
    request<{ ok: true; import: DataImportSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/import`,
      {
        method: "POST",
        body: JSON.stringify({
          fileName: input.fileName,
          content: input.content,
          enrichCvr: input.enrichCvr === true,
          confirm: true,
        }),
      },
    ).then((r) => r.import),

  /**
   * Ingests a document/voucher (bilag) (#213, slice 3). The browser reads the
   * (possibly binary) file and passes it base64-encoded; the server decodes it
   * to a temp file and calls the same core ingest the CLI/MCP use. Destructive,
   * so the body carries `confirm: true`.
   */
  ingestDocument: (slug: string, input: DocumentIngestInput) =>
    request<{
      ok: true;
      document: { id: number | null; documentNo: string | null };
    }>(`/api/companies/${encodeURIComponent(slug)}/documents/ingest`, {
      method: "POST",
      body: JSON.stringify({
        fileName: input.fileName,
        fileBase64: input.fileBase64,
        metadata: input.metadata,
        ...(input.vendorId ? { vendorId: input.vendorId } : {}),
        ...(input.force ? { force: true } : {}),
        confirm: true,
      }),
    }).then((r) => r.document),

  /**
   * #407 — read-side data backing the Bogfør-bilag modal: the bilag's fields
   * to prefill, the bookable expense accounts and the unmatched outgoing
   * bank transactions the owner can pair the bilag with.
   */
  documentBookingOptions: (slug: string, documentId: number) =>
    request<{ ok: true; options: DocumentBookingOptions }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/${documentId}/booking-options`,
    ).then((r) => r.options),

  /** Read-only preflight: no provider call and no state change. */
  documentVatPreflight: (slug: string, documentId: number) =>
    request<{ ok: true; preflight: DocumentVatPreflight }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/${documentId}/vat-preflight`,
    ).then((r) => r.preflight),

  /** Actor-attributed provider call, gated by the same mutation boundary as posting. */
  applyDocumentVatPreflight: (slug: string, documentId: number) =>
    request<{ ok: true; preflight: DocumentVatPreflight }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/${documentId}/vat-preflight/apply`,
      { method: "POST", body: JSON.stringify({ confirm: true }) },
    ).then((r) => r.preflight),

  /**
   * #407 — books an ingested purchase document (bilag) as an expense against
   * an unmatched outgoing bank transaction. Third caller of the SAME
   * `bookExpenseFromBank` core function the CLI's `expense book` and the MCP
   * tool use. Write-irreversible (it appends a journal entry that links both
   * the document and the bank transaction), so the body carries
   * `confirm: true`.
   */
  bookDocumentExpense: (slug: string, input: DocumentBookExpenseInput) =>
    request<{ ok: true; booking: DocumentBookExpenseSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/book-expense`,
      {
        method: "POST",
        body: JSON.stringify({
          documentId: input.documentId,
          bankTransactionId: input.bankTransactionId,
          expenseAccountNo: input.expenseAccountNo,
          ...(input.vatTreatment ? { vatTreatment: input.vatTreatment } : {}),
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.transactionDate
            ? { transactionDate: input.transactionDate }
            : {}),
          ...(input.text ? { text: input.text } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.booking),
};

/** Wire type for one bookable expense account (#407). */
export type ExpenseAccountOption = {
  accountNo: string;
  name: string;
  defaultVatCode: string | null;
};

export type DocumentVatPreflight = {
  ok: boolean;
  derivedRegion: "DK" | "EU" | "NON_EU" | "CONFLICT";
  requiredValidation: string | null;
  cache: { reused: boolean; freshUntil: string | null };
  applyWouldCallProvider: boolean;
  errors: string[];
  exception: { id: number; status: string; severity: string; message: string; requiredAction: string | null; createdAt: string } | null;
};

/** Wire type for one unmatched outgoing bank transaction (#407). */
export type UnmatchedBankOption = {
  id: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
  amountDkk: number | null;
  fxRateToDkk: number | null;
  reference: string | null;
};

/** Wire type for the bilag fields the modal prefills its form from (#407). */
export type DocumentBookingOptionsDocument = {
  id: number;
  documentNo: string | null;
  documentType: string;
  sourceBankTransactionId: number | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  supplierVatOrCvr: string | null;
  supplierCountryCode: string | null;
  supplierIdentifierKind: string | null;
  supplierIdentityStatus: string | null;
  purchaseVatLines: Array<{ classification: string; netAmount: number; vatAmount?: number }> | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

/** Combined read-side response for the Bogfør-bilag modal (#407). */
export type DocumentBookingOptions = {
  document: DocumentBookingOptionsDocument;
  expenseAccounts: ExpenseAccountOption[];
  unmatchedOutgoingBank: UnmatchedBankOption[];
};

/** Allowed VAT treatments for an expense booking (#407). */
export type ExpenseVatTreatment =
  | "standard"
  | "reverse_charge"
  | "representation"
  | "exempt"
  | "non_deductible";

/** Input for `api.bookDocumentExpense` (#407). */
export type DocumentBookExpenseInput = {
  documentId: number;
  bankTransactionId: number;
  expenseAccountNo: string;
  vatTreatment?: ExpenseVatTreatment;
  paymentAccountNo?: string;
  transactionDate?: string;
  text?: string;
};

/** The booking result the server echoes back (#407). */
export type DocumentBookExpenseSummary = {
  entryId: number | null;
  documentId: number | null;
  bankTransactionId: number | null;
  grossAmount: number | null;
  netAmount: number | null;
  vatAmount: number | null;
  vatTreatment: string | null;
  grossAmountForeign: number | null;
  grossAmountDkk: number | null;
  netAmountDkk: number | null;
  vatAmountDkk: number | null;
  fxRateToDkk: number | null;
};

/** Input for `api.importData` — the file name + text, plus the CVR-enrich opt-in. */
export type DataImportInput = {
  fileName: string;
  content: string;
  enrichCvr?: boolean;
};

/** The source system + data type the server recognised an imported file as. */
export type DataImportDetected = {
  id: string;
  label: string;
  system: string;
  dataType: string;
};

/** The contact-import counts the server echoes back. */
export type DataImportCounts = {
  parsed: number;
  customersCreated: number;
  vendorsCreated: number;
  skipped: number;
  enriched: number;
  enrichmentFailures: number;
};

/** The file-import result the server echoes back. */
export type DataImportSummary = {
  detected: DataImportDetected | null;
  summary: DataImportCounts;
  errors: string[];
};

/** Document metadata for `api.ingestDocument` — amounts are kroner (decimal DKK). */
export type DocumentIngestMetadata = {
  source: string;
  documentType?: "purchase_sale" | "cash_register_receipt" | "internal_voucher" | "external_accounting_evidence";
  internalVoucherKind?: "bank_evidenced" | "non_cash_balance_correction";
  issueDate?: string;
  invoiceNo?: string;
  deliveryDescription?: string;
  amountIncVat?: number;
  currency?: string;
  sender?: { name?: string; address?: string; vatOrCvr?: string; countryCode?: string; identifierKind?: "dk_cvr" | "eu_vat" | "non_eu" };
  recipient?: { name?: string; address?: string; vatOrCvr?: string };
  vatAmount?: number;
  purchaseVatLines?: Array<{ classification: "dk_purchase_25" | "exempt"; netAmount: number; vatAmount?: number }>;
  reverseChargeWordingConfirmed?: boolean;
  reverseChargeWordingEvidence?: { excerpt: string; location: string };
  /** Source fact only; company identity is recorded separately and never copied into recipient. */
  danishSimplifiedPurchaseInvoice?: boolean;
  incompleteStandardPurchaseInvoice?: boolean;
  paymentDetails?: string;
  sourceBankTransactionId?: number;
  accountingRationale?: string;
  externalAccountingEvidence?: { category: "payroll"; accountingPeriod: string; externalReference: string; totals: { debitAmount: number; creditAmount: number } };
};

/** Input for `api.ingestDocument` — the base64 file plus its metadata. */
export type DocumentIngestInput = {
  fileName: string;
  fileBase64: string;
  metadata: DocumentIngestMetadata;
  vendorId?: number;
  force?: boolean;
};
