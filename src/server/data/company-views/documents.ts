import { existsSync } from "node:fs";
import { getCompanySettings } from "../../../core/company";
import { openCurrentLedgerReadOnly } from "../../../core/ledger-inspection";
import { purchaseVatLinesFromPayload } from "../../../core/documents";
import { companyPaths } from "../../../core/paths";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  type EvidenceFileSnapshot,
  EvidenceFileUnavailable,
  evidenceDownloadFilename,
  readVerifiedEvidenceFile,
} from "../evidence-file";
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
  internalVoucherKind: "bank_evidenced" | "non_cash_balance_correction" | "legacy_opening_creditor_reclassification" | null;
  legacyOpeningJournalEntryId: number | null;
  legacyOpeningJournalLineId: number | null;
  sourceBankTransactionId: number | null;
  accountingRationale: string | null;
  preparedBy: string | null;
  preparedByProgram: string | null;
  preparedAt: string | null;
  supplierName: string | null;
  supplierVatOrCvr: string | null;
  supplierCountryCode: string | null;
  supplierIdentifierKind: string | null;
  supplierIdentityStatus: string | null;
  /** Current explicit document-to-canonical-party relation, when present. */
  partyId: string | null;
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
  purchaseVatLines: unknown[] | null;
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

  const db = openCurrentLedgerReadOnly(dbPath);
  try {
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
                d.sender_vat_cvr  AS supplierVatOrCvr,
                d.supplier_country_code AS supplierCountryCode,
                d.supplier_identifier_kind AS supplierIdentifierKind,
                d.supplier_identity_status AS supplierIdentityStatus,
                (SELECT party_id FROM current_document_party_links party_link
                  WHERE party_link.document_id=d.id
                  ORDER BY CASE party_link.party_role WHEN 'supplier' THEN 0 WHEN 'vendor' THEN 1 WHEN 'issuer' THEN 2 WHEN 'customer' THEN 3 WHEN 'recipient' THEN 4 ELSE 99 END, party_link.id DESC LIMIT 1) AS partyId,
                d.invoice_no      AS invoiceNo,
                d.invoice_date    AS invoiceDate,
                d.amount_inc_vat  AS amountIncVat,
                d.currency        AS currency,
                d.status          AS status,
                ive.bank_transaction_id AS sourceBankTransactionId,
                CASE WHEN legacy.document_id IS NOT NULL THEN 'legacy_opening_creditor_reclassification' WHEN ncc.document_id IS NOT NULL THEN 'non_cash_balance_correction' WHEN ive.document_id IS NOT NULL THEN 'bank_evidenced' ELSE NULL END AS internalVoucherKind,
                legacy.opening_journal_entry_id AS legacyOpeningJournalEntryId, legacy.opening_journal_line_id AS legacyOpeningJournalLineId,
                COALESCE(ive.accounting_rationale,ncc.accounting_rationale) AS accountingRationale,
                COALESCE(ive.prepared_by,ncc.prepared_by) AS preparedBy,
                COALESCE(ive.prepared_by_program,ncc.prepared_by_program) AS preparedByProgram,
                COALESCE(ive.created_at,ncc.created_at) AS preparedAt,
                d.payload_json    AS payloadJson,
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
           LEFT JOIN internal_voucher_evidence ive ON ive.document_id = d.id
           LEFT JOIN non_cash_balance_correction_evidence ncc ON ncc.document_id = d.id
           LEFT JOIN legacy_opening_creditor_reclassification_evidence legacy ON legacy.document_id = d.id
          -- EJER-15: the 'issued_invoice_pdf' row is the invoice's OWN rendered
          -- PDF — an internal artifact Rentemester writes when it issues a sales
          -- invoice, NOT an inbound voucher the owner must process. Including it
          -- made the company's own invoice PDF appear as an "ubehandlet bilag"
          -- in the list and inflate the unlinked counter. It is excluded here so
          -- the bilag list/counter only counts real inbound documents. (The
          -- canonical 'issued_invoice' JSON row and the PDF are still served on
          -- the invoice's own row via the invoice views.)
          WHERE d.document_type != 'issued_invoice_pdf'
          ORDER BY d.upload_datetime DESC, d.id DESC`,
      )
      .all() as Array<{
      id: number;
      documentNo: string | null;
      source: string;
      filename: string | null;
      documentType: string;
      internalVoucherKind: "bank_evidenced" | "non_cash_balance_correction" | "legacy_opening_creditor_reclassification" | null;
      legacyOpeningJournalEntryId: number | null;
      legacyOpeningJournalLineId: number | null;
      sourceBankTransactionId: number | null;
      accountingRationale: string | null;
      preparedBy: string | null;
      preparedByProgram: string | null;
      preparedAt: string | null;
      supplierName: string | null;
      supplierVatOrCvr: string | null;
      supplierCountryCode: string | null;
      supplierIdentifierKind: string | null;
      supplierIdentityStatus: string | null;
      partyId: string | null;
      invoiceNo: string | null;
      invoiceDate: string | null;
      amountIncVat: number | null;
      currency: string;
      status: string;
      payloadJson: string | null;
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
      internalVoucherKind: r.internalVoucherKind,
      legacyOpeningJournalEntryId: r.legacyOpeningJournalEntryId,
      legacyOpeningJournalLineId: r.legacyOpeningJournalLineId,
      sourceBankTransactionId: r.sourceBankTransactionId,
      accountingRationale: r.accountingRationale,
      preparedBy: r.preparedBy,
      preparedByProgram: r.preparedByProgram,
      preparedAt: r.preparedAt,
      supplierName: r.supplierName,
      supplierVatOrCvr: r.supplierVatOrCvr,
      supplierCountryCode: r.supplierCountryCode,
      supplierIdentifierKind: r.supplierIdentifierKind,
      supplierIdentityStatus: r.supplierIdentityStatus,
      partyId: r.partyId,
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
      purchaseVatLines: purchaseVatLinesFromPayload(r.payloadJson),
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
): EvidenceFileSnapshot {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  // A download is evidence retrieval, never a ledger maintenance path.  In
  // particular it must not call `migrate()` (which can repair/write state) or
  // even open the ledger read-write.
  const db = openCurrentLedgerReadOnly(dbPath);
  try {
    const row = db.query(
      `SELECT stored_path AS storedPath, mime_type AS mimeType,
              sha256_hash AS sha256Hash, document_type AS documentType
         FROM documents WHERE id = ?`,
    ).get(documentId) as {
      storedPath: string | null;
      mimeType: string | null;
      sha256Hash: string;
      documentType: string;
    } | null;
    if (!row?.storedPath || !row.sha256Hash) {
      throw ApiError.notFound("bilagsfil er ikke tilgængelig");
    }
    try {
      return readVerifiedEvidenceFile({
        companyRoot,
        storedPath: row.storedPath,
        expectedSha256: row.sha256Hash,
        documentType: row.documentType,
        mimeType: row.mimeType,
        filename: evidenceDownloadFilename(documentId, extensionForMime(row.mimeType)),
      });
    } catch (error) {
      if (error instanceof EvidenceFileUnavailable) {
        throw ApiError.notFound("bilagsfil er ikke tilgængelig");
      }
      throw error;
    }
  } finally {
    db.close();
  }
}

function extensionForMime(mimeType: string | null): string {
  switch ((mimeType ?? "").trim().toLowerCase()) {
    case "application/pdf": return ".pdf";
    case "text/plain": return ".txt";
    case "image/jpeg": return ".jpg";
    case "image/png": return ".png";
    case "image/gif": return ".gif";
    case "image/webp": return ".webp";
    default: return "";
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
  amountDkk: number | null;
  fxRateToDkk: number | null;
  reference: string | null;
};

/** The minimum bilag fields the modal prefills its form from. */
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
  purchaseVatLines: unknown[] | null;
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
  const db = openCurrentLedgerReadOnly(dbPath);
  try {
    const doc = db
      .query(
        `SELECT d.id, d.document_no, d.document_type, d.invoice_no, d.invoice_date,
                d.supplier_name, d.sender_vat_cvr, d.supplier_country_code,
                d.supplier_identifier_kind, d.supplier_identity_status,
                d.amount_inc_vat, d.vat_amount, d.currency, d.payload_json,
                ive.bank_transaction_id AS source_bank_transaction_id
           FROM documents d
           LEFT JOIN internal_voucher_evidence ive ON ive.document_id = d.id
          WHERE d.id = ?`,
      )
      .get(documentId) as
      | {
          id: number;
          document_no: string | null;
          document_type: string;
          invoice_no: string | null;
          invoice_date: string | null;
          supplier_name: string | null;
          sender_vat_cvr: string | null;
          supplier_country_code: string | null;
          supplier_identifier_kind: string | null;
          supplier_identity_status: string | null;
          amount_inc_vat: number | null;
          vat_amount: number | null;
          currency: string;
          payload_json: string | null;
          source_bank_transaction_id: number | null;
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
                bt.amount_dkk   AS amountDkk,
                bt.fx_rate_to_dkk AS fxRateToDkk,
                bt.reference    AS reference
           FROM bank_transactions bt
          WHERE bt.amount < 0
            AND NOT EXISTS (
              SELECT 1 FROM bank_journal_reconciliations br
               WHERE br.bank_transaction_id = bt.id
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
      amountDkk: number | null;
      fxRateToDkk: number | null;
      reference: string | null;
    }>).map((r) => ({
      id: r.id,
      date: r.date,
      text: r.text,
      amount: roundKroner(r.amount),
      currency: r.currency,
      amountDkk: r.amountDkk === null ? null : roundKroner(r.amountDkk),
      fxRateToDkk: r.fxRateToDkk === null ? null : Number(r.fxRateToDkk),
      reference: r.reference,
    }));
    return {
      document: {
        id: doc.id,
        documentNo: doc.document_no,
        documentType: doc.document_type,
        sourceBankTransactionId: doc.source_bank_transaction_id,
        invoiceNo: doc.invoice_no,
        invoiceDate: doc.invoice_date,
        supplierName: doc.supplier_name,
        supplierVatOrCvr: doc.sender_vat_cvr,
        supplierCountryCode: doc.supplier_country_code,
        supplierIdentifierKind: doc.supplier_identifier_kind,
        supplierIdentityStatus: doc.supplier_identity_status,
        purchaseVatLines: purchaseVatLinesFromPayload(doc.payload_json),
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
