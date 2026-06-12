/**
 * Runtime bookkeeper agent — run entrypoint + report formatting (#183).
 *
 * `formatRunReport` renders an `AgentRunReport` as a human-readable end-of-run
 * report. It is pure (no I/O), so it is exercised directly by tests and reused
 * by the `agent run` CLI command.
 */

import type { AgentRunReport } from "./loop";
import { AGENT_RULE_ID } from "./contract";
import { formatKronerDa, normalizeCurrency } from "../core/money";

/**
 * Renders a booked-expense amount for the end-of-run report.
 *
 * #314: a DKK amount delegates to the single canonical formatter
 * `formatKronerDa` in `core/money.ts`, so it emits the identical
 * `"1.234,50 kr."` string as every other human-facing surface (the historical
 * `" DKK"` suffix is gone). A foreign-currency amount keeps its own code
 * ("100,00 EUR") rather than being mislabelled "kr." (cf. #269).
 */
function fmtAmount(n: number, ccy: string): string {
  if (normalizeCurrency(ccy) === "DKK") return formatKronerDa(n);
  const sign = n < 0 ? "-" : "";
  const abs = Math.abs(n);
  return `${sign}${abs.toLocaleString("da-DK", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })} ${normalizeCurrency(ccy)}`;
}

/**
 * Renders the deterministic end-of-run report: what was booked, what was
 * left in the exception queue, and which deadlines are near. The output is a
 * pure function of the report — same report in, same text out.
 */
export function formatRunReport(report: AgentRunReport): string {
  const lines: string[] = [];
  lines.push("Rentemester runtime-agent — kørsel");
  lines.push("=".repeat(34));
  lines.push(`regel:    ${AGENT_RULE_ID}`);
  lines.push(`actor:    ${report.actor}`);
  lines.push(`as-of:    ${report.asOf}`);
  lines.push(`company:  ${report.company}`);
  lines.push(`faser:    ${report.phases.join(" → ")}`);

  if (!report.ok) {
    lines.push("");
    lines.push("KØRSEL FEJLEDE:");
    for (const err of report.errors) lines.push(`  ✗ ${err}`);
    return lines.join("\n");
  }

  lines.push("");
  lines.push("— Bogført automatisk —");
  if (report.expensesBooked.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const e of report.expensesBooked) {
      lines.push(
        `  ✓ ${e.supplier} ${fmtAmount(e.amount, e.currency)} → konto ${e.expenseAccount} ` +
          `(${e.label}, moms=${e.vatTreatment})` +
          (e.journalEntryNo ? ` [${e.journalEntryNo}]` : ""),
      );
    }
  }

  lines.push("");
  lines.push("— Kreditorposter afregnet automatisk —");
  if (report.payablesMatched.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const p of report.payablesMatched) {
      lines.push(
        `  ✓ ${p.supplier} ${formatKronerDa(p.amount)} → kreditorpost #${p.payableId}` +
          (p.journalEntryNo ? ` [${p.journalEntryNo}]` : ""),
      );
    }
  }

  lines.push("");
  lines.push("— Exception-kø (kræver et menneske) —");
  if (report.openExceptions.length === 0) {
    lines.push("  ✓ ingen åbne exceptions");
  } else {
    for (const ex of report.openExceptions) {
      lines.push(`  ! [${ex.severity}] #${ex.exceptionId} ${ex.type}: ${ex.message}`);
      if (ex.requiredAction) lines.push(`      → ${ex.requiredAction}`);
    }
  }

  lines.push("");
  lines.push("— Kommende deadlines —");
  if (report.upcomingDeadlines.length === 0) {
    lines.push("  ✓ ingen deadlines inden for horisonten");
  } else {
    for (const d of report.upcomingDeadlines) {
      const label = d.kind === "vat_quarter" ? "Momsangivelse" : "Årsrapport";
      lines.push(
        `  ${d.ready ? "✓" : "!"} ${label} ${d.periodStart}..${d.periodEnd} — ` +
          `forfalder ${d.dueDate} (${d.daysRemaining} dage)`,
      );
      lines.push(`      ${d.note}`);
    }
  }

  lines.push("");
  lines.push("=== Kørsel afsluttet ===");
  for (const s of report.summary) lines.push(`  • ${s}`);

  return lines.join("\n");
}
