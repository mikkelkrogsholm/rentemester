import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import { getCompanySettings } from "../../../core/company";
import { resolveDocumentFile } from "../../../core/documents";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  roundKroner,
  statementCompanyBlock,
} from "../shared";

// --------------------------------------------------------------------------
// Per-company documents (Bilag) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type DocumentRow = {
  id: number;
  documentNo: string | null;
  source: string;
  filename: string | null;
  documentType: string;
  supplierName: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  currency: string;
  status: string;
  /** The voucher reference the document was matched on, when linked. */
  voucherRef: string | null;
  /** The linked journal entry's number, when one exists. */
  journalEntryNo: string | null;
  /** The linked journal entry's id, for drill-through. */
  journalEntryId: number | null;
  /** The linked journal entry's posting text — what the voucher is for. */
  journalEntryText: string | null;
  /** The linked journal entry's total (summed debit side), kroner. */
  journalEntryTotal: number | null;
  /** True when the document has a stored file the cockpit can open. */
  hasFile: boolean;
};

export type CompanyDocuments = ReturnType<typeof buildCompanyDocuments>;

/**
 * Bilag — the ingested documents/receipts in the company's `documents` table,
 * each carrying the voucher and posted journal entry it is linked to through
 * `import_document_links` (#196) where one exists. Newest upload first.
 */
export function buildCompanyDocuments(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    // A bilag is "bogført" when either an import_document_links row binds it
    // to a journal entry (the legacy archive-import flow) OR a journal entry
    // directly references it via journal_entries.document_id (the native
    // bookkeeping flow used by `expense book`, `invoice post` and the
    // Bogfør-bilag modal in #407). Without the COALESCE the document keeps
    // looking "Ikke bogført" in the cockpit even after the entry posts, which
    // is the loop the modal is closing.
    const rows = db
      .query(
        `SELECT d.id              AS id,
                d.document_no     AS documentNo,
                d.source          AS source,
                d.original_filename AS filename,
                d.document_type   AS documentType,
                d.supplier_name   AS supplierName,
                d.invoice_no      AS invoiceNo,
                d.invoice_date    AS invoiceDate,
                d.amount_inc_vat  AS amountIncVat,
                d.currency        AS currency,
                d.status          AS status,
                d.stored_path     AS storedPath,
                idl.voucher_ref   AS voucherRef,
                COALESCE(je_link.id, je_direct.id)             AS journalEntryId,
                COALESCE(je_link.entry_no, je_direct.entry_no) AS journalEntryNo,
                COALESCE(je_link.text, je_direct.text)         AS journalEntryText,
                COALESCE(
                  (SELECT COALESCE(SUM(debit_amount), 0)
                     FROM journal_lines
                    WHERE journal_entry_id = je_link.id),
                  (SELECT COALESCE(SUM(debit_amount), 0)
                     FROM journal_lines
                    WHERE journal_entry_id = je_direct.id)
                )                                              AS journalEntryTotal
           FROM documents d
           LEFT JOIN import_document_links idl ON idl.document_id = d.id
           LEFT JOIN journal_entries je_link   ON je_link.id = idl.journal_entry_id
           LEFT JOIN journal_entries je_direct ON je_direct.document_id = d.id
          ORDER BY d.upload_datetime DESC, d.id DESC`,
      )
      .all() as Array<{
      id: number;
      documentNo: string | null;
      source: string;
      filename: string | null;
      documentType: string;
      supplierName: string | null;
      invoiceNo: string | null;
      invoiceDate: string | null;
      amountIncVat: number | null;
      currency: string;
      status: string;
      storedPath: string | null;
      voucherRef: string | null;
      journalEntryId: number | null;
      journalEntryNo: string | null;
      journalEntryText: string | null;
      journalEntryTotal: number | null;
    }>;

    const documents: DocumentRow[] = rows.map((r) => ({
      id: r.id,
      documentNo: r.documentNo,
      source: r.source,
      filename: r.filename,
      documentType: r.documentType,
      supplierName: r.supplierName,
      invoiceNo: r.invoiceNo,
      invoiceDate: r.invoiceDate,
      amountIncVat:
        r.amountIncVat === null || r.amountIncVat === undefined
          ? null
          : roundKroner(r.amountIncVat),
      currency: r.currency,
      status: r.status,
      voucherRef: r.voucherRef,
      journalEntryNo: r.journalEntryNo,
      journalEntryId: r.journalEntryId,
      journalEntryText: r.journalEntryText,
      journalEntryTotal:
        r.journalEntryId === null || r.journalEntryTotal === null
          ? null
          : roundKroner(r.journalEntryTotal),
      hasFile: r.storedPath != null,
    }));
    const linkedCount = documents.filter(
      (d) => d.journalEntryNo !== null,
    ).length;

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      documents,
      linkedCount,
      unlinkedCount: documents.length - linkedCount,
    };
  } finally {
    db.close();
  }
}

/**
 * Resolves a single ingested document's stored file so the cockpit can serve
 * it back to a human. The same notFound shape the other document reads use is
 * thrown for an unknown company, a missing ledger, or a document without a
 * readable file.
 */
export function resolveCompanyDocumentFile(
  workspaceRoot: string,
  slug: string,
  documentId: number,
): { path: string; mimeType: string; filename: string } {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const resolved = resolveDocumentFile(db, companyRoot, documentId);
    if (!resolved.ok) {
      throw ApiError.notFound(resolved.error);
    }
    return resolved.file;
  } finally {
    db.close();
  }
}

// --- documents / Bilag-bogføring (#407) -----------------------------------
//
// The Cockpit's Bilag table shows "Ikke bogført" rows without an action — the
// owner can see the queue but cannot post anything from the browser. The pair
//
//   GET  .../documents/:id/booking-options    (this builder)
//   POST .../documents/book-expense           (handleDocumentBookExpense)
//
// closes that loop. The GET hands the modal everything it needs (the document
// fields to prefill, the bookable expense accounts, the unmatched outgoing
// bank transactions to pair with) and the POST is a thin adapter over the
// SAME `bookExpenseFromBank` core function the CLI's `expense book` command
// uses, so the Cockpit never reimplements bookkeeping.

/** One bookable expense account — the picker rows in the modal. */
export type ExpenseAccountOption = {
  accountNo: string;
  name: string;
  /** Hint for default VAT treatment; null when not configured. */
  defaultVatCode: string | null;
};

/** One unmatched outgoing bank transaction the owner can pair the bilag with. */
export type UnmatchedBankOption = {
  id: number;
  date: string;
  text: string;
  /** Original signed kroner amount — outgoing payments are negative. */
  amount: number;
  currency: string;
  reference: string | null;
};

/** The minimum bilag fields the modal prefills its form from. */
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

export type DocumentBookingOptions = {
  document: DocumentBookingOptionsDocument;
  expenseAccounts: ExpenseAccountOption[];
  unmatchedOutgoingBank: UnmatchedBankOption[];
};

/**
 * Read-only view backing the Bogfør-bilag modal: the document being booked,
 * the bookable expense-account picker rows, and the unmatched outgoing bank
 * transactions the owner can pair it with. A 404 is thrown when the company
 * / ledger / document is missing; an already-linked document is allowed and
 * returns its current row (the core booker is the one that refuses a
 * double-post, with a clean Danish error).
 */
export function buildDocumentBookingOptions(
  workspaceRoot: string,
  slug: string,
  documentId: number,
): DocumentBookingOptions {
  if (!Number.isInteger(documentId) || documentId <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const doc = db
      .query(
        `SELECT id, document_no, document_type, invoice_no, invoice_date,
                supplier_name, amount_inc_vat, vat_amount, currency
           FROM documents
          WHERE id = ?`,
      )
      .get(documentId) as
      | {
          id: number;
          document_no: string | null;
          document_type: string;
          invoice_no: string | null;
          invoice_date: string | null;
          supplier_name: string | null;
          amount_inc_vat: number | null;
          vat_amount: number | null;
          currency: string;
        }
      | null;
    if (!doc) {
      throw ApiError.notFound(`document ${documentId} does not exist`);
    }
    const expenseAccounts = (db
      .query(
        `SELECT account_no, name, default_vat_code
           FROM accounts
          WHERE type = 'expense' AND active = 1
          ORDER BY account_no ASC`,
      )
      .all() as Array<{
      account_no: string;
      name: string;
      default_vat_code: string | null;
    }>).map((r) => ({
      accountNo: r.account_no,
      name: r.name,
      defaultVatCode: r.default_vat_code,
    }));
    // Outgoing = amount < 0. Unmatched = no journal entry already references
    // this row. Newest first, capped to a sensible picker length.
    const unmatchedOutgoingBank = (db
      .query(
        `SELECT bt.id          AS id,
                bt.transaction_date AS date,
                bt.text         AS text,
                bt.amount       AS amount,
                bt.currency     AS currency,
                bt.reference    AS reference
           FROM bank_transactions bt
          WHERE bt.amount < 0
            AND NOT EXISTS (
              SELECT 1 FROM journal_entries je
               WHERE je.source_bank_transaction_id = bt.id
            )
          ORDER BY bt.transaction_date DESC, bt.id DESC
          LIMIT 200`,
      )
      .all() as Array<{
      id: number;
      date: string;
      text: string;
      amount: number;
      currency: string;
      reference: string | null;
    }>).map((r) => ({
      id: r.id,
      date: r.date,
      text: r.text,
      amount: roundKroner(r.amount),
      currency: r.currency,
      reference: r.reference,
    }));
    return {
      document: {
        id: doc.id,
        documentNo: doc.document_no,
        documentType: doc.document_type,
        invoiceNo: doc.invoice_no,
        invoiceDate: doc.invoice_date,
        supplierName: doc.supplier_name,
        amountIncVat:
          doc.amount_inc_vat === null
            ? null
            : roundKroner(doc.amount_inc_vat),
        vatAmount:
          doc.vat_amount === null ? null : roundKroner(doc.vat_amount),
        currency: doc.currency,
      },
      expenseAccounts,
      unmatchedOutgoingBank,
    };
  } finally {
    db.close();
  }
}
