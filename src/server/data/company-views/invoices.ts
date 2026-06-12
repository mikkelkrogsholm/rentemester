import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import { buildInvoiceList } from "../../../core/invoice-list";
import { renderIssuedInvoicePdf } from "../../../core/invoice-pdf";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";

// --------------------------------------------------------------------------
// Per-company issued invoices (Fakturaer, year-aware) — cockpit-redesign it. 5
// --------------------------------------------------------------------------

/**
 * Resolves an issued-invoice document into the on-disk PDF the cockpit can
 * serve to the owner. The same `renderIssuedInvoicePdf` core that the CLI's
 * `invoice render` command uses is called — re-rendering is idempotent: when
 * the payload has not changed since issuance, the existing PDF row is returned
 * unchanged; if it has, the row is updated in place. The thrown shapes match
 * `resolveCompanyDocumentFile` so the route handler can rely on the same
 * not-found mapping. (#378)
 */
export function resolveCompanyIssuedInvoicePdf(
  workspaceRoot: string,
  slug: string,
  invoiceDocumentId: number,
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
    const result = renderIssuedInvoicePdf(db, companyRoot, {
      invoiceDocumentId,
    });
    if (!result.ok || !result.storedPath || !result.invoiceNumber) {
      const reason = result.errors[0] ?? "issued invoice PDF could not be rendered";
      throw ApiError.notFound(reason);
    }
    return {
      path: result.storedPath,
      mimeType: "application/pdf",
      filename: `${result.invoiceNumber}.pdf`,
    };
  } finally {
    db.close();
  }
}

/**
 * Cockpit-facing PEPPOL/e-faktura status (#428) — verbatim copy of the
 * core `InvoicePeppolStatus`, exposed so the Cockpit can flag
 * "Sendt som e-faktura" on a row and gate the send-action.
 */
export type CompanyInvoicePeppolStatus = {
  status: "prepared" | "acknowledged";
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
    };
  } finally {
    ctx.db.close();
  }
}
