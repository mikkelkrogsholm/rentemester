/**
 * Billy postings parser — year-to-date activity after the cut-over date.
 *
 * Parses `postings.json` from the Billy API export and groups postings by
 * `transactionId` into balanced vouchers, mirroring how the Dinero postings
 * parser groups by `Bilag` number. Each voucher becomes an
 * `ImportHistoricalEntry` that the framework replays as a journal entry.
 *
 * The parser is PURE and DETERMINISTIC: the same export always yields the
 * same ordered list of historical entries.
 */

import type { ImportHistoricalEntry, ImportOpeningBalanceLine } from "./types";

type BillyPosting = {
  id: string;
  transactionId: string;
  entryDate: string;
  text: string;
  accountId: string;
  amount: number;
  side: string;
  isVoided: boolean;
  priority: number;
};

export type ParseBillyPostingsOptions = {
  /** Cut-over date (YYYY-MM-DD): only postings ON or AFTER this date are included. */
  cutOverDate: string;
  /** Map of Billy account IDs to Rentemester account numbers. */
  accountIdToNo: Map<string, string>;
};

/**
 * Parses Billy postings into historical entries (vouchers) for the import
 * framework. Only non-voided postings on or after `cutOverDate` are included.
 * Postings are grouped by `transactionId`; each group forms one balanced
 * journal entry.
 */
export function parseBillyPostings(
  postingsText: string,
  options: ParseBillyPostingsOptions,
  errors: string[],
): ImportHistoricalEntry[] {
  let postings: BillyPosting[];
  try {
    postings = JSON.parse(postingsText);
  } catch {
    errors.push("postings.json: invalid JSON");
    return [];
  }
  if (!Array.isArray(postings)) {
    errors.push("postings.json: expected a JSON array");
    return [];
  }

  const { cutOverDate, accountIdToNo } = options;

  if (!/^\d{4}-\d{2}-\d{2}$/.test(cutOverDate)) {
    errors.push(`cutOverDate '${cutOverDate}' is not a valid YYYY-MM-DD date`);
    return [];
  }

  // Filter: non-voided, on or after cut-over date
  const active = postings.filter(
    (p) => !p.isVoided && p.entryDate >= cutOverDate,
  );

  if (active.length === 0) return [];

  // Group by transactionId, preserving first-seen order
  const vouchers = new Map<
    string,
    {
      transactionId: string;
      entryDate: string;
      text: string;
      lines: ImportOpeningBalanceLine[];
    }
  >();

  // Sort by entryDate then priority for deterministic ordering
  const sorted = [...active].sort((a, b) => {
    const dateCmp = a.entryDate.localeCompare(b.entryDate);
    if (dateCmp !== 0) return dateCmp;
    return (a.priority ?? 0) - (b.priority ?? 0);
  });

  for (const p of sorted) {
    const accountNo = accountIdToNo.get(p.accountId);
    if (!accountNo) continue;

    let voucher = vouchers.get(p.transactionId);
    if (!voucher) {
      voucher = {
        transactionId: p.transactionId,
        entryDate: p.entryDate,
        text: p.text && p.text.trim().length > 0 ? p.text.trim() : `Billy txn ${p.transactionId.slice(0, 8)}`,
        lines: [],
      };
      vouchers.set(p.transactionId, voucher);
    }

    if (p.amount === 0) continue;

    const lineText = p.text && p.text.trim().length > 0 ? p.text.trim() : undefined;
    const absAmount = Math.abs(p.amount);
    if (p.side === "debit") {
      voucher.lines.push({ accountNo, debitAmount: absAmount, text: lineText });
    } else if (p.side === "credit") {
      voucher.lines.push({ accountNo, creditAmount: absAmount, text: lineText });
    }
    // Unknown side values are silently skipped — the voucher will have fewer
    // lines and may be filtered out as single-line if all lines are unknown.
  }

  // Convert to ImportHistoricalEntry, filtering out single-line vouchers
  const entries: ImportHistoricalEntry[] = [];
  for (const v of vouchers.values()) {
    if (v.lines.length < 2) continue;
    entries.push({
      transactionDate: v.entryDate,
      text: v.text,
      voucherRef: v.transactionId,
      lines: v.lines,
    });
  }

  return entries;
}
