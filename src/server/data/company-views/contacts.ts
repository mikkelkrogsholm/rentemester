import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import { getCompanySettings } from "../../../core/company";
import { buildInvoiceList } from "../../../core/invoice-list";
import { listCustomers, listVendors } from "../../../core/master-data";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  buildCompanyFiscalYears,
  roundKroner,
  statementCompanyBlock,
  todayIsoDate,
} from "../shared";

// --------------------------------------------------------------------------
// Per-company contacts (Kontakter — customers + vendors) — cockpit-redesign it. 5
// --------------------------------------------------------------------------

/** One customer in the master data. */
export type ContactCustomerRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  email: string | null;
  paymentTermsDays: number;
  defaultCurrency: string;
  // #390 — surface the remaining stamdata fields so the Cockpit edit-modal can
  // prefill them without a second round-trip.
  address: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  notes: string | null;
  /**
   * #439 — aggregated open receivables per customer so the Kontakter-side
   * answers "hvad skylder de mig?" directly. Derived from the same
   * `buildInvoiceList` pipeline that powers `/invoices` and the Overblik
   * "Tilgodehavender"-summary; no business logic is duplicated. A customer
   * with no open invoices reports `openBalance: 0`, `openInvoiceCount: 0`,
   * `overdueCount: 0`.
   */
  openBalance: number;
  openInvoiceCount: number;
  overdueCount: number;
};

/** One vendor (supplier) in the master data. */
export type ContactVendorRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
  // #390 — surface the remaining stamdata fields so the Cockpit edit-modal can
  // prefill them without a second round-trip.
  address: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  notes: string | null;
};

export type CompanyContacts = ReturnType<typeof buildCompanyContacts>;

/**
 * Kontakter — the company's customers and vendors (master data). This is
 * reference data, not year-scoped; the company sub-nav still carries the
 * selected `?year=` so it follows the user across views, so the fiscal years
 * for the selector are fetched alongside. Both lists come straight from
 * `core/master-data`. A company with no contacts returns empty lists — a
 * correct, expected state.
 */
export function buildCompanyContacts(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const years = buildCompanyFiscalYears(workspaceRoot, slug).years;

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    // #439 — aggregate open receivables per customer from the same
    // `buildInvoiceList` pipeline that powers `/invoices`. Run it once across
    // all years (no `from`/`to` filter) with `status: "open"` so paid /
    // credited / written-off invoices drop out and the surviving rows each
    // carry a fresh `openBalance` and `isOverdue` from `getInvoiceStatus`.
    // `asOfDate: todayIsoDate()` so "forfalden" measures against today (the
    // default `comparisonDate` is the invoice's own due date, which would
    // mean nothing is ever overdue).
    // Matching is by trimmed customer name — the same join Kontakter uses to
    // prefill the "Send på mail" dialog (#429).
    const openInvoiceRows = buildInvoiceList(db, {
      status: "open",
      asOfDate: todayIsoDate(),
    }).rows;
    type CustomerOpenAgg = {
      openBalance: number;
      openInvoiceCount: number;
      overdueCount: number;
    };
    const openByCustomerName = new Map<string, CustomerOpenAgg>();
    for (const row of openInvoiceRows) {
      const key = (row.customerName ?? "").trim();
      if (!key) continue;
      const agg = openByCustomerName.get(key) ?? {
        openBalance: 0,
        openInvoiceCount: 0,
        overdueCount: 0,
      };
      agg.openBalance += row.openBalance;
      agg.openInvoiceCount += 1;
      if (row.isOverdue) agg.overdueCount += 1;
      openByCustomerName.set(key, agg);
    }

    const customers: ContactCustomerRow[] = listCustomers(db).rows.map((c) => {
      const agg = openByCustomerName.get((c.name ?? "").trim());
      return {
        id: c.id,
        name: c.name,
        vatOrCvr: c.vatOrCvr,
        email: c.email,
        paymentTermsDays: c.paymentTermsDays,
        defaultCurrency: c.defaultCurrency,
        address: c.address,
        phone: c.phone,
        website: c.website,
        eanNumber: c.eanNumber,
        notes: c.notes,
        openBalance: agg ? roundKroner(agg.openBalance) : 0,
        openInvoiceCount: agg?.openInvoiceCount ?? 0,
        overdueCount: agg?.overdueCount ?? 0,
      };
    });
    // #439 — bring customers with forfaldne fakturaer to the top, then those
    // with any open invoice, then the rest. Within each bucket keep the
    // master-data ordering so the table is stable. Mirrors PortfolioView's
    // `sortByAttention`.
    customers.sort((a, b) => {
      const aAttn = a.overdueCount > 0 ? 2 : a.openInvoiceCount > 0 ? 1 : 0;
      const bAttn = b.overdueCount > 0 ? 2 : b.openInvoiceCount > 0 ? 1 : 0;
      if (aAttn !== bAttn) return bAttn - aAttn;
      if (b.openBalance !== a.openBalance) return b.openBalance - a.openBalance;
      return 0;
    });
    const vendors: ContactVendorRow[] = listVendors(db).rows.map((v) => ({
      id: v.id,
      name: v.name,
      vatOrCvr: v.vatOrCvr,
      defaultExpenseAccount: v.defaultExpenseAccount,
      defaultVatTreatment: v.defaultVatTreatment,
      address: v.address,
      email: v.email,
      phone: v.phone,
      website: v.website,
      notes: v.notes,
    }));

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      fiscalYears: years,
      customers,
      vendors,
    };
  } finally {
    db.close();
  }
}
