import type { DocumentsResponse } from "../types";
import { request } from "./_shared";

export const documentsApi = {
  documents: (slug: string) =>
    request<DocumentsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/documents`,
    ).then((r) => r.documents),

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

/** Wire type for one unmatched outgoing bank transaction (#407). */
export type UnmatchedBankOption = {
  id: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
  reference: string | null;
};

/** Wire type for the bilag fields the modal prefills its form from (#407). */
export type DocumentBookingOptionsDocument = {
  id: number;
  documentNo: string | null;
  documentType: string;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
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
  | "exempt";

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
  documentType?: "purchase_sale" | "cash_register_receipt";
  issueDate?: string;
  invoiceNo?: string;
  deliveryDescription?: string;
  amountIncVat?: number;
  currency?: string;
  sender?: { name?: string; address?: string; vatOrCvr?: string };
  recipient?: { name?: string; address?: string; vatOrCvr?: string };
  vatAmount?: number;
  paymentDetails?: string;
};

/** Input for `api.ingestDocument` — the base64 file plus its metadata. */
export type DocumentIngestInput = {
  fileName: string;
  fileBase64: string;
  metadata: DocumentIngestMetadata;
  vendorId?: number;
  force?: boolean;
};
