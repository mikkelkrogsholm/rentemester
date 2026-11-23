import type { Database } from "bun:sqlite";
import type { AccountRole } from "./account-roles";
import { dedicatedReverseChargeAccount, resolveAccountRole } from "./account-roles";

export type VatAmountSide = "output" | "input";

/** Persistable line classifications understood by the current ledger/report. */
export const VAT_LINE_CODES: ReadonlySet<string> = new Set([
  "DK_PURCHASE_25",
  "DK_PURCHASE_EXEMPT",
  "DK_SALE_25",
  "DK_SALE_EXEMPT",
  "EU_SERVICE_REVERSE_CHARGE",
  "EU_GOODS_ACQUISITION",
  "NON_EU_SERVICE_REVERSE_CHARGE",
  "REPRESENTATION_SPECIAL",
  "REPRESENTATION_NON_DEDUCTIBLE_VAT",
  "DK_BAD_DEBT_25",
  "REVERSE_CHARGE_EXEMPT",
  "DOMESTIC_REVERSE_CHARGE_EXEMPT",
  "OSS_EU_CONSUMER",
]);

/**
 * Dinero's source-native VAT control accounts.
 *
 * These exact identifiers are evidence from the Dinero chart format, not a
 * general account-number range.  A legacy import may have persisted them as
 * liabilities; newer imports normalise them to `type = 'vat'`.  The output
 * controls can be recognised from that chart evidence. A legacy 64060 is
 * not recognised from its number alone. Historical vouchers use their
 * original evidence; a source-backed operator review additionally classifies
 * new postings on the reviewed input control.
 */
export const DINERO_VAT_CONTROL_ACCOUNTS = {
  "64000": { role: "output_vat", normalBalance: "credit", side: "output" },
  // Dinero's separate output-VAT control for foreign goods acquisitions.
  // Rentemester still does not infer the corresponding goods-purchase base,
  // but the source-posted VAT amount must remain identifiable during import,
  // reporting, and year-opening VAT consolidation.
  "64020": { role: "reverse_charge_vat", normalBalance: "credit", side: "output" },
  "64040": { role: "reverse_charge_vat", normalBalance: "credit", side: "output" },
  "64060": { role: "input_vat", normalBalance: "debit", side: "input" },
} as const satisfies Record<
  string,
  { role: AccountRole; normalBalance: "debit" | "credit"; side: VatAmountSide }
>;

export type VatAccountSemantics = {
  /** VAT amount accounts and whether their signed movement is output or input VAT. */
  amountSideByAccountNo: ReadonlyMap<string, VatAmountSide>;
  /** Includes confirmed legacy controls only for journal entries created after the review. */
  amountSideForEntry(accountNo: string, entryId: number): VatAmountSide | undefined;
  dedicatedReverseChargeForEntry(entryId: number): string | null;
  /** Output-VAT accounts dedicated to reverse charge rather than ordinary sales. */
  reverseChargeOutputAccountNos: ReadonlySet<string>;
  /** Settlement accounts move the net balance but are not themselves VAT amounts. */
  settlementAccountNos: ReadonlySet<string>;
};

/**
 * Load the canonical VAT-account meaning for one ledger.
 *
 * Recognition is deliberately narrow and chart-aware:
 *
 * - every native/imported account explicitly typed `vat` is classified from
 *   its normal balance, including inactive accounts needed by old periods;
 * - the exact source-validated Dinero output controls are accepted in their
 *   legacy liability shape, never an arbitrary `64000..64099` range;
 * - explicitly confirmed account roles are authoritative and included; and
 * - the confirmed VAT-settlement role is kept separate from VAT amounts.
 */
export function loadVatAccountSemantics(db: Database): VatAccountSemantics {
  const rows = db
    .query("SELECT account_no, type, normal_balance, active FROM accounts")
    .all() as Array<{
    account_no: string;
    type: string;
    normal_balance: "debit" | "credit";
    active: number;
  }>;

  const amountSideByAccountNo = new Map<string, VatAmountSide>();
  for (const row of rows) {
    if (row.type === "vat") {
      amountSideByAccountNo.set(
        row.account_no,
        row.normal_balance === "debit" ? "input" : "output",
      );
      continue;
    }
    // `active` gates future posting, not historical interpretation. Explicit
    // type=vat above therefore remains meaningful after an account is retired;
    // number-based legacy compatibility below stays limited to active charts.
    if (row.active !== 1) continue;
    const dinero =
      DINERO_VAT_CONTROL_ACCOUNTS[
        row.account_no as keyof typeof DINERO_VAT_CONTROL_ACCOUNTS
      ];
    if (
      dinero &&
      // A 64060 liability is too ambiguous outside a verified historical
      // Dinero voucher. `buildVatReport` admits it only after checking the
      // matching 64040/64060 control pair and provenance.
      row.account_no !== "64060" &&
      row.type === "liability" &&
      row.normal_balance === dinero.normalBalance
    ) {
      amountSideByAccountNo.set(row.account_no, dinero.side);
    }
  }

  // A review is prospective. An ordinary movement on an old liability must
  // retain its original interpretation even when the first purchase is booked
  // after review, or the role is later superseded. IDs order writes reliably
  // for backdated postings and multiple confirmations in the same second.
  const reviews = db.query(`SELECT m.account_no, m.role, r.last_existing_entry_id, r.previous_dedicated_reverse_charge_account
    FROM dinero_vat_role_reviews r JOIN account_role_mappings m ON m.id = r.mapping_id
    ORDER BY r.last_existing_entry_id, r.mapping_id`).all() as Array<{
    account_no: string; role: AccountRole; last_existing_entry_id: number; previous_dedicated_reverse_charge_account: string | null;
  }>;
  const firstReview = new Map<string, (typeof reviews)[number]>();
  for (const review of reviews) {
    if (!firstReview.has(review.account_no)) firstReview.set(review.account_no, review);
  }
  const dedicatedChanges = reviews.filter((row) => row.role === "output_vat" || row.role === "reverse_charge_vat");
  const currentDedicated = dedicatedReverseChargeAccount(db);
  const reverseChargeOutputAccountNos = new Set<string>();
  // Dinero's exact 64040 control is source-native evidence of a dedicated
  // reverse-charge output account, even before its imported role proposal is
  // confirmed.
  if (amountSideByAccountNo.get("64040") === "output") {
    reverseChargeOutputAccountNos.add("64040");
  }
  const settlementAccountNos = new Set<string>();
  const settlement = resolveAccountRole(db, "vat_settlement");
  if (settlement.ok) settlementAccountNos.add(settlement.accountNo);

  return {
    amountSideByAccountNo,
    amountSideForEntry(accountNo, entryId) {
      const side = amountSideByAccountNo.get(accountNo);
      if (side) return side;
      const review = firstReview.get(accountNo);
      if (review && entryId > review.last_existing_entry_id) return review.role === "input_vat" ? "input" : "output";
    },
    dedicatedReverseChargeForEntry(entryId) {
      const nextChange = dedicatedChanges.find((row) => entryId <= row.last_existing_entry_id);
      return nextChange ? nextChange.previous_dedicated_reverse_charge_account : currentDedicated;
    },
    reverseChargeOutputAccountNos,
    settlementAccountNos,
  };
}
