import { asArray, asCount, asText, appendWarnings, formatKroner } from "./common";

// --- Bank reconciliation ---------------------------------------------------

export function renderBankReconciliation(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(
    `Bankafstemning for perioden ${asText(result.periodStart)} til ${asText(result.periodEnd)}`,
  );
  lines.push("");
  const matchedCount = asCount(result.matchedCount);
  const unmatchedCount = asCount(result.unmatchedCount);
  lines.push(
    `${matchedCount} afstemt${matchedCount === 1 ? "" : "e"} og ${unmatchedCount} uafstemt${unmatchedCount === 1 ? "" : "e"} transaktion${matchedCount + unmatchedCount === 1 ? "" : "er"}.`,
  );
  lines.push(`  Afstemt beløb i alt:           ${formatKroner(result.matchedAmountTotal)}`);

  // A single netted "Uafstemt beløb i alt" hides the real workload: an
  // outflow of -1.250 and an inflow of +2.500 net to +1.250, masking that
  // there are two rows to reconcile. Show inflows and outflows separately
  // so an owner sees both the count and the gross size of each side.
  const unmatchedRows = asArray(result.unmatched);
  let inflowTotal = 0;
  let inflowCount = 0;
  let outflowTotal = 0;
  let outflowCount = 0;
  for (const row of unmatchedRows) {
    const amount = Number(row.amount);
    if (!Number.isFinite(amount)) continue;
    if (amount < 0) {
      outflowTotal += amount;
      outflowCount += 1;
    } else {
      inflowTotal += amount;
      inflowCount += 1;
    }
  }
  lines.push(
    `  Uafstemte indbetalinger (${inflowCount}):  ${formatKroner(inflowTotal)}`,
  );
  lines.push(
    `  Uafstemte udbetalinger (${outflowCount}):   ${formatKroner(-outflowTotal)}`,
  );

  const matched = asArray(result.matched);
  if (matched.length > 0) {
    lines.push("");
    lines.push("Afstemte transaktioner:");
    for (const row of matched) {
      lines.push(
        `  - ${asText(row.transactionDate)} | ${formatKroner(row.amount)} | ${asText(row.text)} → postering ${asText(row.journalEntryNo)}`,
      );
    }
  }

  const unmatched = unmatchedRows;
  if (unmatched.length > 0) {
    lines.push("");
    lines.push("Uafstemte transaktioner:");
    for (const row of unmatched) {
      lines.push(
        `  - ${asText(row.transactionDate)} | ${formatKroner(row.amount)} | ${asText(row.text)}`,
      );
    }
  }

  if (matched.length === 0 && unmatched.length === 0) {
    lines.push("");
    lines.push("Ingen transaktioner for det valgte filter.");
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}
