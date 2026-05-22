import { vatFilingDeadline } from "../core/vat";
import {
  asArray,
  asCount,
  asText,
  appendWarnings,
  formatKroner,
} from "./common";

// ===========================================================================
// HUMAN-READABLE REPORT RENDERING (#211)
// ---------------------------------------------------------------------------
// Read/report commands emit deterministic JSON with English field names for
// agents. When a human runs them (`--format human` / a TTY) that JSON is
// unreadable. These renderers render the same payloads in clear Danish with
// money in kroner-og-øre. The `--format json` path is untouched.
// ===========================================================================

// --- VAT report ------------------------------------------------------------

export function renderVatReport(result: Record<string, unknown>): string {
  const from = asText(result.periodStart);
  const to = asText(result.periodEnd);
  const net = typeof result.netVatPayable === "number" ? result.netVatPayable : Number(result.netVatPayable);
  const lines: string[] = [];
  lines.push(`Momsrapport for perioden ${from} til ${to}`);
  lines.push("");

  if (Number.isFinite(net) && net > 0) {
    lines.push(`Du skal betale ${formatKroner(net)} i moms for perioden.`);
  } else if (Number.isFinite(net) && net < 0) {
    lines.push(`Du har ${formatKroner(-net)} til gode i moms for perioden.`);
  } else {
    lines.push("Momstilsvaret for perioden er 0,00 kr.");
  }
  lines.push("");
  lines.push(`  Salgsmoms (udgående moms):     ${formatKroner(result.outputVat)}`);
  lines.push(`  Købsmoms (indgående moms):     ${formatKroner(result.inputVat)}`);
  lines.push(`  Momstilsvar (netto):           ${formatKroner(result.netVatPayable)}`);
  lines.push("");
  lines.push("Momsgrundlag:");
  lines.push(`  Køb med 25% moms:              ${formatKroner(result.purchaseBase25)}`);
  lines.push(`  Salg med 25% moms:             ${formatKroner(result.salesBase25)}`);
  if (asCount(result.reverseChargeSalesBase) !== 0) {
    lines.push(`  Salg med omvendt betalingspligt: ${formatKroner(result.reverseChargeSalesBase)}`);
  }
  if (asCount(result.reverseChargePurchaseBase) !== 0) {
    lines.push(`  Køb med omvendt betalingspligt:  ${formatKroner(result.reverseChargePurchaseBase)}`);
  }
  if (asCount(result.representationPurchaseBase) !== 0) {
    lines.push(`  Repræsentationskøb:            ${formatKroner(result.representationPurchaseBase)}`);
  }
  if (asCount(result.badDebtReliefBase25) !== 0) {
    lines.push(`  Tab på debitorer (25%):        ${formatKroner(result.badDebtReliefBase25)}`);
  }
  lines.push("");
  const entries = asCount(result.journalEntryCount);
  const postLines = asCount(result.linesConsidered);
  lines.push(
    `Rapporten dækker ${entries} finanspostering${entries === 1 ? "" : "er"}` +
      ` (${postLines} posteringslinje${postLines === 1 ? "" : "r"}).`,
  );
  // SKAT filing/payment deadline — the date that costs money if missed. (#236)
  const deadline = vatFilingDeadline(to);
  if (deadline) {
    lines.push("");
    lines.push(`SKAT-frist for indberetning og betaling: ${deadline}`);
    lines.push(
      "  (kvartalsmoms skal angives og betales senest den 1. i tredje måned efter periodens udløb)",
    );
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- VAT filing (momsangivelse) --------------------------------------------

/**
 * Render a `vat momsangivelse` / `vat filing` payload as Danish human text.
 * The payload is the `VatFilingReport` from `buildVatFiling` — the SKAT
 * rubrikker plus momstilsvar and the filing deadline. (#235, #236)
 */
export function renderVatFiling(result: Record<string, unknown>): string {
  const from = asText(result.periodStart);
  const to = asText(result.periodEnd);
  const lines: string[] = [];
  lines.push(`Momsangivelse for perioden ${from} til ${to}`);
  lines.push("");

  const rubrikker = (typeof result.rubrikker === "object" && result.rubrikker !== null
    ? result.rubrikker
    : {}) as Record<string, unknown>;
  const tilsvar = typeof rubrikker.momstilsvar === "number"
    ? rubrikker.momstilsvar
    : Number(rubrikker.momstilsvar);

  if (Number.isFinite(tilsvar) && tilsvar > 0) {
    lines.push(`Du skal betale ${formatKroner(tilsvar)} i moms til SKAT for perioden.`);
  } else if (Number.isFinite(tilsvar) && tilsvar < 0) {
    lines.push(`Du har ${formatKroner(-tilsvar)} til gode i moms (negativt momstilsvar).`);
  } else {
    lines.push("Momstilsvaret for perioden er 0,00 kr.");
  }
  lines.push("");
  lines.push("Rubrikker til TastSelv Erhverv:");
  lines.push(`  Salgsmoms:                       ${formatKroner(rubrikker.salgsmoms)}`);
  lines.push(`  Moms af varekøb i udlandet:      ${formatKroner(rubrikker.momsAfVarekobUdland)}`);
  lines.push(`  Moms af ydelseskøb i udlandet:   ${formatKroner(rubrikker.momsAfYdelseskobUdland)}`);
  lines.push(`  Købsmoms:                        ${formatKroner(rubrikker.kobsmoms)}`);
  lines.push(`  Momstilsvar:                     ${formatKroner(rubrikker.momstilsvar)}`);
  lines.push(`  Rubrik A (køb i udlandet):       ${formatKroner(rubrikker.rubrikA)}`);
  lines.push(`  Rubrik B (salg i udlandet):      ${formatKroner(rubrikker.rubrikB)}`);
  lines.push(`  Rubrik C (øvrigt momsfrit salg): ${formatKroner(rubrikker.rubrikC)}`);
  lines.push("");

  const statusDa = result.periodStatus === "reported"
    ? "indberettet"
    : result.periodStatus === "closed"
      ? "lukket"
      : "åben";
  lines.push(`  Periodestatus:                   ${statusDa}`);
  if (result.periodReference) {
    lines.push(`  Periodereference:                ${asText(result.periodReference)}`);
  }
  const deadline = asText(result.filingDeadline, "") || vatFilingDeadline(to) || "";
  if (deadline) {
    lines.push("");
    lines.push(`SKAT-frist for indberetning og betaling: ${deadline}`);
    lines.push(
      "  (kvartalsmoms skal angives og betales senest den 1. i tredje måned efter periodens udløb)",
    );
  }
  lines.push("");
  lines.push(
    "Rentemester producerer tallene — du indberetter dem selv via TastSelv Erhverv.",
  );
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Annual report (årsrapport) --------------------------------------------

/**
 * Render a `report annual` payload as Danish human text — resultatopgørelse,
 * balance, årets resultat, noter-skelet and ledelsespåtegning. The payload is
 * the `AnnualReport` from `buildAnnualReport`. (#235)
 */
export function renderAnnualReport(result: Record<string, unknown>): string {
  const from = asText(result.fiscalYearStart);
  const to = asText(result.fiscalYearEnd);
  const company = (typeof result.company === "object" && result.company !== null
    ? result.company
    : {}) as Record<string, unknown>;
  const lines: string[] = [];
  lines.push(`Årsrapport (regnskabsklasse B) for regnskabsåret ${from} til ${to}`);
  if (company.name) {
    const cvr = company.cvr ? `, CVR ${asText(company.cvr)}` : "";
    lines.push(`${asText(company.name)}${cvr}`);
  }
  lines.push("");

  // Resultatopgørelse — reuse the profit-and-loss renderer's body.
  const pl = (typeof result.profitAndLoss === "object" && result.profitAndLoss !== null
    ? result.profitAndLoss
    : {}) as Record<string, unknown>;
  lines.push("Resultatopgørelse");
  lines.push("");
  const income = asArray(pl.income);
  lines.push("Indtægter:");
  if (income.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const line of income) {
      lines.push(`  ${asText(line.accountNo)} ${asText(line.name)}: ${formatKroner(line.amount)}`);
    }
  }
  lines.push(`  Indtægter i alt:               ${formatKroner(pl.totalIncome)}`);
  lines.push("");
  const expense = asArray(pl.expense);
  lines.push("Omkostninger:");
  if (expense.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const line of expense) {
      lines.push(`  ${asText(line.accountNo)} ${asText(line.name)}: ${formatKroner(line.amount)}`);
    }
  }
  lines.push(`  Omkostninger i alt:            ${formatKroner(pl.totalExpense)}`);
  lines.push("");

  const arets = typeof result.aretsResultat === "number"
    ? result.aretsResultat
    : Number(result.aretsResultat);
  if (Number.isFinite(arets) && arets > 0) {
    lines.push(`Årets resultat: overskud på ${formatKroner(arets)}`);
  } else if (Number.isFinite(arets) && arets < 0) {
    lines.push(`Årets resultat: underskud på ${formatKroner(-arets)}`);
  } else {
    lines.push("Årets resultat: 0,00 kr.");
  }
  lines.push("");

  // Balance.
  const bs = (typeof result.balanceSheet === "object" && result.balanceSheet !== null
    ? result.balanceSheet
    : {}) as Record<string, unknown>;
  lines.push(`Balance pr. ${asText(bs.asOfDate, to)}`);
  lines.push("");
  renderBalanceSection(lines, "Aktiver", bs.assets);
  lines.push("");
  renderBalanceSection(lines, "Passiver", bs.liabilities);
  lines.push("");
  renderBalanceSection(lines, "Egenkapital", bs.equity);
  lines.push(`  Periodens resultat:`.padEnd(33) + formatKroner(bs.periodResult));
  lines.push("");
  lines.push(`  Aktiver i alt:                 ${formatKroner(bs.totalAssets)}`);
  lines.push(`  Passiver og egenkapital i alt: ${formatKroner(bs.totalLiabilitiesAndEquity)}`);
  lines.push(
    bs.balanced === true
      ? "  Balancen går op (aktiver = passiver + egenkapital)."
      : "  ADVARSEL: balancen går ikke op.",
  );
  lines.push("");

  // Notes skeleton.
  const notes = asArray(result.notes);
  if (notes.length > 0) {
    lines.push("Noter:");
    for (const note of notes) {
      const flag = note.placeholder === true ? " (skabelon — udfyldes)" : "";
      lines.push(`  - ${asText(note.title)}${flag}`);
    }
    lines.push("");
  }

  // Ledelsespåtegning.
  const led = (typeof result.ledelsespategning === "object" && result.ledelsespategning !== null
    ? result.ledelsespategning
    : {}) as Record<string, unknown>;
  if (led.text) {
    lines.push("Ledelsespåtegning:");
    lines.push(`  ${asText(led.text)}`);
    if (led.placeholder === true) {
      lines.push("  (skabelon — gennemgås og godkendes af ledelsen)");
    }
    lines.push("");
  }

  if (result.disclaimer) {
    lines.push(asText(result.disclaimer));
  }
  appendWarnings(lines, result);
  if (lines[lines.length - 1] === "") lines.pop();
  return lines.join("\n");
}

// --- Balance section helper ------------------------------------------------
// Shared by the annual report (above) and the balance-sheet renderer.

export function renderBalanceSection(lines: string[], heading: string, section: unknown): void {
  const data = (typeof section === "object" && section !== null ? section : {}) as Record<string, unknown>;
  lines.push(`${heading}:`);
  const sectionLines = asArray(data.lines);
  if (sectionLines.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const line of sectionLines) {
      lines.push(`  ${asText(line.accountNo)} ${asText(line.name)}: ${formatKroner(line.amount)}`);
    }
  }
  lines.push(`  ${heading} i alt:`.padEnd(33) + formatKroner(data.total));
}
