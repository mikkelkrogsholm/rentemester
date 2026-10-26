import { existsSync } from "node:fs";
import { openCurrentLedgerReadOnly } from "../../../core/ledger-inspection";
import { buildInvoiceList } from "../../../core/invoice-list";
import { listImportedReceivables } from "../../../core/imported-receivables";
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
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import { dataCoverage } from "../../../data-coverage";

// --------------------------------------------------------------------------
// Per-company issued invoices (Fakturaer, year-aware) — cockpit-redesign it. 5
// --------------------------------------------------------------------------

/**
 * Resolves existing issued-invoice PDF evidence into an immutable byte
 * snapshot. GET never invokes the renderer: rendering/replacing invoice
 * evidence is an explicit issuing workflow, not a read-side repair.
 */
export function resolveCompanyIssuedInvoicePdf(
  workspaceRoot: string,
  slug: string,
  invoiceDocumentId: number,
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

  const db = openCurrentLedgerReadOnly(dbPath);
  try {
    db.exec("PRAGMA query_only = ON");
    const invoice = db.query(
      `SELECT invoice_no AS invoiceNo, payload_json AS payloadJson FROM documents
        WHERE id = ? AND document_type = 'issued_invoice'`,
    ).get(invoiceDocumentId) as { invoiceNo: string | null; payloadJson: string | null } | null;
    if (!invoice?.invoiceNo || !invoice.payloadJson) {
      throw ApiError.notFound("faktura-PDF er ikke tilgængelig");
    }
    // A PDF artifact is immutable evidence for exactly one issued invoice.
    // Old/ambiguous ledgers are denied rather than guessing the newest row.
    const pdfRows = db.query(
      `SELECT id, stored_path AS storedPath, sha256_hash AS sha256Hash,
              mime_type AS mimeType
         FROM documents
        WHERE document_type = 'issued_invoice_pdf'
          AND invoice_no = ?
          AND payload_json = ?
        ORDER BY id ASC`,
    ).all(invoice.invoiceNo, invoice.payloadJson) as Array<{
      id: number;
      storedPath: string | null;
      sha256Hash: string;
      mimeType: string | null;
    }>;
    if (pdfRows.length !== 1 || !pdfRows[0]!.storedPath || !pdfRows[0]!.sha256Hash) {
      throw ApiError.notFound("faktura-PDF er ikke tilgængelig");
    }
    try {
      return readVerifiedEvidenceFile({
        companyRoot,
        storedPath: pdfRows[0]!.storedPath!,
        expectedSha256: pdfRows[0]!.sha256Hash,
        documentType: "issued_invoice_pdf",
        mimeType: pdfRows[0]!.mimeType,
        filename: evidenceDownloadFilename(invoiceDocumentId, ".pdf"),
      });
    } catch (error) {
      if (error instanceof EvidenceFileUnavailable) {
        throw ApiError.notFound("faktura-PDF er ikke tilgængelig");
      }
      throw error;
    }
  } finally {
    db.close();
  }
}

/**
 * Cockpit-facing PEPPOL/e-faktura status (#428) — verbatim copy of the
 * core `InvoicePeppolStatus`, exposed so the Cockpit can flag
 * truthful delivery status on a row and gate send/status actions.
 */
export type CompanyInvoicePeppolStatus = {
  status: "queued" | "failed" | "uncertain" | "retryable" | "in_progress" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

/** One issued invoice — the fields the Fakturaer view renders. */
export type CompanyInvoiceRow = {
  documentId: number;
  invoiceNo: string;
  invoiceDate: string | null;
  customerName: string | null;
  /** Explicit customer/recipient party relation on this invoice document. */
  partyId: string | null;
  /**
   * Customer's e-mail when set on the kontaktkort (#429). Surfaced so the
   * cockpit can render a "Send på mail" action only on rows where there is
   * a stored e-mail to prefill, and the dialog has no second round-trip.
   */
  customerEmail: string | null;
  /**
   * Buyer's EAN-number normalised to 13 digits, or `null`. #428: the
   * Cockpit row offers "Send som e-faktura" only when this is set AND the
   * buyer is marked as a public recipient.
   */
  buyerEanNumber: string | null;
  /** True when the invoice was issued to a public-recipient buyer. */
  buyerPublicRecipient: boolean;
  /**
   * The most recent recorded PEPPOL submission/transmission status, or
   * `null` when the invoice has never been sent as an e-faktura.
   */
  peppolStatus: CompanyInvoicePeppolStatus | null;
  /**
   * ISO-8601 timestamp of the most recent `email_send_log` row for this
   * invoice (#429), or `null` when the invoice has never been emailed
   * from the cockpit. Read-only audit data — the cockpit uses it only to
   * surface "Sendt {dato}" beside the settlement status.
   */
  lastEmailedAt: string | null;
  /**
   * Timestamp (ISO-8601) of the most recently registered payment reminder
   * for the invoice (#434), or `null` when no reminder has been registered.
   * Surfaced so the cockpit can show "{n}. rykker sendt {dato}" and gate the
   * "Send rykker" action against the 3-reminder cap (rentel. § 9b).
   */
  lastReminderAt: string | null;
  /**
   * Count of registered reminders for the invoice (#434). 0 when no reminder
   * has been sent. The cockpit hides the "Send rykker" action once this
   * reaches 3.
   */
  lastReminderSequence: number;
  /** Gross amount inc. VAT, kroner. */
  grossAmount: number;
  /** Still-outstanding balance on the invoice, kroner. */
  openBalance: number;
  currency: string;
  /** Settlement state, plus "overdue" for an open invoice past its due date. */
  status: "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off" | "overdue";
  effectiveDueDate: string | null;
  overdueDays: number;
};

export type CompanyInvoices = ReturnType<typeof buildCompanyInvoices>;

/**
 * Fakturaer — the company's issued (sales) invoices for the selected calendar
 * fiscal year. Each row carries its settlement status; an open invoice past
 * its due date is surfaced as "overdue". Every figure comes from
 * `core/invoice-list` (which derives status via `core/invoice-payments`) — no
 * business logic is duplicated. A company with no issued invoices returns an
 * empty list, which is a correct, expected state.
 */
export function buildCompanyInvoices(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: `${ctx.selectedLabel}-01-01`,
        periodEnd: `${ctx.selectedLabel}-12-31`,
        invoices: [] as CompanyInvoiceRow[],
        totalGross: 0,
        totalOpen: 0,
        overdueCount: 0,
        coverage: dataCoverage("final", `${ctx.selectedLabel}-12-31`, "not_comparable", "archived", ["Arkiveret periode har ingen native udstedte fakturaer."]),
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    const list = buildInvoiceList(ctx.db, { from: yearStart, to: yearEnd });
    // #429 — match the row's `customerName` against the kontakter so the
    // cockpit can prefill the "Send på mail" dialog with the stored e-mail.
    // Read once and lookup in-memory: a typical company has tens of customers,
    // and this view already runs a per-row payments query through the core.
    const customerEmailByName = new Map<string, string>();
    try {
      const customerRows = ctx.db
        .query(
          `SELECT name, email FROM customers
           WHERE archived = 0 AND email IS NOT NULL AND TRIM(email) <> ''`,
        )
        .all() as Array<{ name: string; email: string | null }>;
      for (const c of customerRows) {
        if (c.name && typeof c.email === "string" && c.email.trim() !== "") {
          customerEmailByName.set(c.name.trim(), c.email.trim());
        }
      }
    } catch {
      // Older fixture without a customers table — degrade to no prefill.
    }
    // #429 — look up the most recent email_send_log row per invoice so the
    // cockpit can surface "Sendt {dato}". The table is append-only; an
    // older db without the migration is treated as "never sent".
    const lastEmailedAtById = new Map<number, string>();
    try {
      const sendRows = ctx.db
        .query(
          `SELECT invoice_document_id, MAX(created_at) AS last_at
           FROM email_send_log
           WHERE kind = 'invoice'
           GROUP BY invoice_document_id`,
        )
        .all() as Array<{ invoice_document_id: number; last_at: string | null }>;
      for (const s of sendRows) {
        if (s.last_at) lastEmailedAtById.set(s.invoice_document_id, s.last_at);
      }
    } catch {
      // email_send_log not migrated — no rows to surface.
    }
    // #434 — surface the most recent registered reminder per invoice so the
    // cockpit row can flag "{n}. rykker sendt {dato}" and decide whether the
    // statutory cap of 3 reminders has been reached. The table is append-only;
    // an older db without the migration is treated as "no reminders".
    const reminderInfoById = new Map<
      number,
      { lastAt: string; sequence: number }
    >();
    try {
      const reminderRows = ctx.db
        .query(
          `SELECT invoice_document_id,
                  COUNT(*) AS sequence,
                  MAX(COALESCE(created_at, reminder_date)) AS last_at
           FROM invoice_reminders
           GROUP BY invoice_document_id`,
        )
        .all() as Array<{
          invoice_document_id: number;
          sequence: number;
          last_at: string | null;
        }>;
      for (const row of reminderRows) {
        if (row.last_at && Number.isFinite(row.sequence)) {
          reminderInfoById.set(row.invoice_document_id, {
            lastAt: row.last_at,
            sequence: Number(row.sequence),
          });
        }
      }
    } catch {
      // invoice_reminders not migrated — no rows to surface.
    }
    const invoices: CompanyInvoiceRow[] = list.rows.map((r) => ({
      documentId: r.documentId,
      invoiceNo: r.invoiceNumber,
      invoiceDate: r.invoiceDate,
      customerName: r.customerName,
      partyId: (ctx.db.query(`SELECT party_id AS partyId FROM current_document_party_links
        WHERE document_id=? AND party_role IN ('customer','recipient','payer')
        ORDER BY CASE party_role WHEN 'customer' THEN 0 WHEN 'recipient' THEN 1 WHEN 'payer' THEN 2 END, id DESC LIMIT 1`).get(r.documentId) as { partyId: string } | null)?.partyId ?? null,
      customerEmail: r.customerName
        ? customerEmailByName.get(r.customerName.trim()) ?? null
        : null,
      buyerEanNumber: r.buyerEanNumber,
      buyerPublicRecipient: r.buyerPublicRecipient,
      peppolStatus: r.peppolStatus,
      lastEmailedAt: lastEmailedAtById.get(r.documentId) ?? null,
      lastReminderAt: reminderInfoById.get(r.documentId)?.lastAt ?? null,
      lastReminderSequence:
        reminderInfoById.get(r.documentId)?.sequence ?? 0,
      grossAmount: roundKroner(r.grossAmount),
      openBalance: roundKroner(r.openBalance),
      currency: r.currency,
      status: r.isOverdue ? "overdue" : r.status,
      effectiveDueDate: r.effectiveDueDate,
      overdueDays: r.overdueDays,
    }));
    // Newest invoice first — the most recent activity is what the user wants.
    invoices.sort((a, b) => {
      const dateA = a.invoiceDate ?? "";
      const dateB = b.invoiceDate ?? "";
      if (dateA !== dateB) return dateB.localeCompare(dateA);
      return b.invoiceNo.localeCompare(a.invoiceNo);
    });

    const totalGross = roundKroner(
      invoices.reduce((acc, r) => acc + r.grossAmount, 0),
    );
    const totalOpen = roundKroner(
      invoices.reduce((acc, r) => acc + r.openBalance, 0),
    );
    const overdueCount = invoices.filter((r) => r.status === "overdue").length;

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      invoices,
      totalGross,
      totalOpen,
      overdueCount,
      coverage: dataCoverage("current", list.asOfDate ?? null, "available", "native", ["Native, udstedte Rentemester-fakturaer."]),
    };
  } finally {
    ctx.db.close();
  }
}

/** Read-only archive side of the receivables cut-over.  This deliberately
 * does not join issued invoices: native invoices remain the other canonical
 * read model and combining them here would make a cut-over double count easy.
 */
export function buildCompanyImportedReceivables(
  workspaceRoot: string,
  slug: string,
  asOf: string,
) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  const dbPath = companyPaths(companyRootForSlug(workspaceRoot, slug)).db;
  if (!existsSync(dbPath)) throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  const db = openCurrentLedgerReadOnly(dbPath);
  try {
    db.exec("PRAGMA query_only = ON");
    return listImportedReceivables(db, asOf);
  } finally {
    db.close();
  }
}
