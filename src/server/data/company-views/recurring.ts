import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import {
  listRecurringInvoiceGenerations,
  listRecurringInvoiceTemplates,
} from "../../../core/recurring-invoices";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";

// --------------------------------------------------------------------------
// Per-company recurring invoice templates
// --------------------------------------------------------------------------

/** One previously generated invoice for a template, ordered period-ascending. */
export type RecurringInvoiceGenerationRow = {
  id: number;
  periodIndex: number;
  invoiceNumber: string;
  issueDate: string;
  documentId: number;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};

/** One recurring-invoice template plus the invoices it has already issued. */
export type RecurringInvoiceTemplateRow = {
  id: number;
  name: string;
  interval: "monthly" | "quarterly" | "yearly";
  firstIssueDate: string;
  /** Next date `generateRecurringInvoice` will materialize a new invoice for. */
  nextIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode: "issue_month" | "interval_window" | "none";
  notes: string | null;
  active: boolean;
  createdAt: string;
  generations: RecurringInvoiceGenerationRow[];
};

export type CompanyRecurringInvoices = {
  slug: string;
  templates: RecurringInvoiceTemplateRow[];
};

/**
 * The cockpit's read of the recurring-invoice templates plus a flat list of
 * every invoice each template has already produced. Retired templates are
 * included so a human can see history; the UI filters as needed.
 */
export function buildCompanyRecurringInvoices(
  workspaceRoot: string,
  slug: string,
): CompanyRecurringInvoices {
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
    const list = listRecurringInvoiceTemplates(db, { includeInactive: true });
    const templates: RecurringInvoiceTemplateRow[] = list.rows.map((row) => ({
      id: row.id,
      name: row.name,
      interval: row.interval,
      firstIssueDate: row.firstIssueDate,
      nextIssueDate: row.nextIssueDate,
      paymentTermsDays: row.paymentTermsDays,
      deliveryPeriodMode: row.deliveryPeriodMode,
      notes: row.notes,
      active: row.active,
      createdAt: row.createdAt,
      generations: listRecurringInvoiceGenerations(db, row.id).rows.map((g) => ({
        id: g.id,
        periodIndex: g.periodIndex,
        invoiceNumber: g.invoiceNumber,
        issueDate: g.issueDate,
        documentId: g.documentId,
        deliveryPeriodStart: g.deliveryPeriodStart,
        deliveryPeriodEnd: g.deliveryPeriodEnd,
      })),
    }));
    return { slug: entry.slug, templates };
  } finally {
    db.close();
  }
}
