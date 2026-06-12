import type { Database } from "bun:sqlite";
import { buildVatReport, type VatPeriodReport } from "./vat";

/**
 * OSS (One Stop Shop) — deterministic first slice.
 *
 * Selling digital services (and distance sales) to private consumers in other
 * EU member states is, above the EU-wide threshold, taxed in the consumer's
 * country. A business can settle that foreign VAT through the OSS scheme via a
 * single quarterly OSS return instead of registering in every member state.
 *
 * OSS was previously declared entirely out of scope. This module is a NARROW
 * first slice with one purpose: stop digital-service sales to EU consumers
 * from being silently miscategorised. It does that with:
 *
 *  1. A dedicated `OSS_EU_CONSUMER` VAT code. A revenue line booked with this
 *     code is recognised by {@link buildVatReport} as an OSS sale: it is
 *     counted in its own `ossConsumerSalesBase`, carries NO Danish output VAT,
 *     and is kept OUT of the standard 25% sales base and the SKAT rubrikker.
 *  2. {@link buildOssReport} — a deterministic per-period OSS skeleton that
 *     surfaces the OSS consumer-sales base from real ledger data.
 *
 * What this slice deliberately does NOT do (still out of scope):
 *  - it does not split the base per destination member state or per rate;
 *  - it does not compute the foreign VAT owed in each country;
 *  - it does not transmit or file an OSS return to SKAT.
 *
 * The OSS return is filed by the business; Rentemester only makes the
 * underlying figure traceable and uncontaminated.
 */

export const OSS_RULE_ID = "DK-VAT-OSS-001";

export type OssReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  /**
   * Total net value (DKK) of digital-service sales to EU consumers booked
   * with the OSS_EU_CONSUMER VAT code in the period.
   */
  ossConsumerSalesBase: number;
  /** Number of journal entries that include an OSS_EU_CONSUMER line. */
  entryCount: number;
  /**
   * Always false: this is a deterministic skeleton, not an OSS submission.
   * Filing the OSS return remains the business's responsibility.
   */
  submission: false;
  /** The underlying raw VAT report, for traceability. */
  vatReport: VatPeriodReport;
  warnings: string[];
  errors: string[];
};

/**
 * Build a deterministic OSS report skeleton for a VAT period.
 *
 * The figure is derived from {@link buildVatReport} so it shares the exact
 * same period filtering and reversal handling as the momsangivelse — an OSS
 * sale that is later reversed drops out of both consistently.
 *
 * Unlike the momsangivelse this does NOT require a closed accounting period:
 * the OSS skeleton is a non-binding preview of the OSS base.
 */
export function buildOssReport(db: Database, periodStart: string, periodEnd: string): OssReport {
  const vatReport = buildVatReport(db, periodStart, periodEnd);
  if (!vatReport.ok) {
    return {
      ok: false,
      appliedRules: [OSS_RULE_ID],
      periodStart,
      periodEnd,
      ossConsumerSalesBase: 0,
      entryCount: 0,
      submission: false,
      vatReport,
      warnings: [],
      errors: [...vatReport.errors],
    };
  }

  const warnings: string[] = [...vatReport.warnings];
  if (vatReport.ossConsumerSalesBase > 0) {
    warnings.push(
      "OSS first slice: this report shows the total OSS consumer-sales base only. " +
        "It does not split the base per destination member state or per rate, and it does not " +
        "file an OSS return — that remains your responsibility.",
    );
  }

  return {
    ok: true,
    appliedRules: [OSS_RULE_ID],
    periodStart,
    periodEnd,
    ossConsumerSalesBase: vatReport.ossConsumerSalesBase,
    entryCount: vatReport.ossConsumerSalesEntryCount,
    submission: false,
    vatReport,
    warnings,
    errors: [],
  };
}
