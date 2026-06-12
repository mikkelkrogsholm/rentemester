import type { Database } from "bun:sqlite";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { addDkk, roundDkk } from "./money";
import { normalizeEuVatNumber } from "./vies";

/**
 * EU-salg uden moms-liste — the VIES recapitulative statement.
 *
 * This is a SEPARATE mandatory Danish filing, distinct from the
 * momsangivelse: a business that sells cross-border B2B goods/services to
 * VAT-registered buyers in other EU member states without charging Danish
 * VAT must, per momsloven § 54, stk. 1, periodically report a per-customer
 * listing — each buyer's EU VAT number and the total value sold to them.
 *
 * Rentemester already holds this data: every such sale is an issued invoice
 * with `vatTreatment: "foreign_reverse_charge"`. This module derives the
 * listing deterministically from those invoices for a given period; it does
 * not transmit anything to SKAT — the business files the numbers itself.
 *
 * Deliberately a FIRST SLICE: it lists foreign reverse-charge B2B sales by
 * invoice issue date. It does not split goods vs. services, does not handle
 * triangulation (trekantshandel), and does not file. All amounts are
 * integer-øre-deterministic via the money helpers.
 */

export const EU_SALES_LIST_RULE_ID = "DK-VAT-EU-SALES-LIST-001";

export type ViesRecapitulativeCustomer = {
  /** The buyer's EU VAT number, normalised (e.g. "DE123456789"). */
  vatNumber: string;
  /** The buyer's EU VAT country code (e.g. "DE"). */
  countryCode: string;
  /** Buyer name as recorded on the most recent invoice in the period. */
  customerName: string | null;
  /** Number of cross-border B2B invoices to this customer in the period. */
  invoiceCount: number;
  /** Total value (net, DKK) sold to this customer without Danish VAT. */
  totalValue: number;
  /** Invoice numbers included for this customer, in issue order. */
  invoiceNumbers: string[];
};

export type ViesRecapitulativeStatement = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  /** Per-customer listing, sorted by VAT number for deterministic output. */
  customers: ViesRecapitulativeCustomer[];
  /** Total value across all customers in the period. */
  totalValue: number;
  /** Total cross-border B2B invoices in the period. */
  invoiceCount: number;
  warnings: string[];
  errors: string[];
};

type IssuedInvoiceRow = {
  invoice_no: string;
  invoice_date: string | null;
  recipient_name: string | null;
  recipient_vat_cvr: string | null;
  amount_inc_vat: number | null;
  vat_amount: number | null;
  currency: string;
  payload_json: string | null;
};

function failure(periodStart: string, periodEnd: string, errors: string[]): ViesRecapitulativeStatement {
  return {
    ok: false,
    appliedRules: [EU_SALES_LIST_RULE_ID],
    periodStart,
    periodEnd,
    customers: [],
    totalValue: 0,
    invoiceCount: 0,
    warnings: [],
    errors,
  };
}

/**
 * Net value of an issued invoice in DKK.
 *
 * A foreign reverse-charge invoice carries no VAT, so gross == net. For a
 * non-DKK invoice the DKK conversion totals on the payload are used; if those
 * are missing the invoice is reported with a warning rather than dropped.
 */
function invoiceNetValueDkk(row: IssuedInvoiceRow): { value: number; warning: string | null } {
  const payload = row.payload_json ? safeParse(row.payload_json) : null;
  const currency = (row.currency ?? "DKK").trim().toUpperCase();
  if (currency === "DKK") {
    const gross = roundDkk(Number(row.amount_inc_vat ?? payload?.totals?.grossAmount ?? 0));
    const vat = roundDkk(Number(row.vat_amount ?? payload?.totals?.vatAmount ?? 0));
    return { value: roundDkk(gross - vat), warning: null };
  }
  const netDkk = Number(payload?.totals?.netAmountDkk);
  if (Number.isFinite(netDkk) && netDkk > 0) {
    return { value: roundDkk(netDkk), warning: null };
  }
  const grossDkk = Number(payload?.totals?.grossAmountDkk);
  if (Number.isFinite(grossDkk) && grossDkk > 0) {
    return { value: roundDkk(grossDkk), warning: null };
  }
  return {
    value: 0,
    warning: `invoice ${row.invoice_no} is in ${currency} but has no DKK conversion total — counted as 0 on the EU sales list`,
  };
}

function safeParse(json: string): any {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

/**
 * Build the EU-salg uden moms-liste for a VAT period.
 *
 * Unlike the momsangivelse this does NOT require a closed accounting period —
 * the recapitulative statement is a separate, lighter filing and is useful as
 * a running preview. The period is still validated as a pair of ISO dates.
 */
export function buildViesRecapitulativeStatement(
  db: Database,
  periodStart: string,
  periodEnd: string,
): ViesRecapitulativeStatement {
  const errors: string[] = [];
  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (errors.length === 0 && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (errors.length > 0) return failure(periodStart, periodEnd, errors);

  // Cross-border B2B sales without Danish VAT are exactly the issued invoices
  // with vatTreatment "foreign_reverse_charge". The treatment lives in
  // payload_json; filtering happens in JS so a missing/odd payload cannot make
  // an invoice silently disappear.
  const rows = db.query(
    `SELECT invoice_no, invoice_date, recipient_name, recipient_vat_cvr,
            amount_inc_vat, vat_amount, currency, payload_json
       FROM documents
      WHERE document_type = 'issued_invoice'
        AND invoice_date >= ? AND invoice_date <= ?
      ORDER BY invoice_date ASC, id ASC`,
  ).all(periodStart, periodEnd) as IssuedInvoiceRow[];

  const warnings: string[] = [];
  const byCustomer = new Map<string, ViesRecapitulativeCustomer>();
  let invoiceCount = 0;
  let totalValue = 0;

  for (const row of rows) {
    const payload = row.payload_json ? safeParse(row.payload_json) : null;
    if (payload?.vatTreatment !== "foreign_reverse_charge") continue;

    const parsedVat = normalizeEuVatNumber(row.recipient_vat_cvr);
    if (!parsedVat) {
      warnings.push(
        `invoice ${row.invoice_no} is a foreign reverse-charge sale but has no parseable EU VAT number — excluded from the EU sales list`,
      );
      continue;
    }
    if (parsedVat.countryCode === "DK") {
      warnings.push(
        `invoice ${row.invoice_no} is marked foreign reverse-charge but the buyer VAT number is Danish — excluded from the EU sales list`,
      );
      continue;
    }

    const { value, warning } = invoiceNetValueDkk(row);
    if (warning) warnings.push(warning);

    invoiceCount += 1;
    totalValue = addDkk(totalValue, value);

    const existing = byCustomer.get(parsedVat.normalized);
    if (existing) {
      existing.invoiceCount += 1;
      existing.totalValue = addDkk(existing.totalValue, value);
      existing.invoiceNumbers.push(row.invoice_no);
      // The latest invoice's recipient name wins (rows are issue-date sorted).
      if (row.recipient_name) existing.customerName = row.recipient_name;
    } else {
      byCustomer.set(parsedVat.normalized, {
        vatNumber: parsedVat.normalized,
        countryCode: parsedVat.countryCode,
        customerName: row.recipient_name,
        invoiceCount: 1,
        totalValue: value,
        invoiceNumbers: [row.invoice_no],
      });
    }
  }

  const customers = [...byCustomer.values()].sort((a, b) =>
    a.vatNumber < b.vatNumber ? -1 : a.vatNumber > b.vatNumber ? 1 : 0,
  );

  return {
    ok: true,
    appliedRules: [EU_SALES_LIST_RULE_ID],
    periodStart,
    periodEnd,
    customers,
    totalValue: roundDkk(totalValue),
    invoiceCount,
    warnings,
    errors: [],
  };
}
