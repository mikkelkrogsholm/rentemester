import { severityDa, exceptionStatusDa } from "../core/messages";
import {
  asArray,
  asCount,
  asText,
  appendWarnings,
  buildHumanError,
  formatKroner,
} from "./common";
import { renderBalanceSection } from "./vat";

// --- Backup status ---------------------------------------------------------

/**
 * Render a `system backup-status` payload as Danish human text. The payload is
 * the `BackupComplianceStatus` from `getBackupComplianceStatus`. States
 * plainly whether the weekly backup duty is met and when the next is due. (#240)
 */
export function renderBackupStatus(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`Backup-status pr. ${asText(result.checkedAt)}`);
  lines.push("");

  const backupsFound = asCount(result.backupsFound);
  const due = result.backupDue === true;
  const hasActivity = result.hasActivitySinceBackup === true;

  if (backupsFound === 0) {
    if (hasActivity) {
      lines.push("Backup-pligten er IKKE opfyldt: der er bogført data, men ingen backup er taget endnu.");
    } else {
      lines.push("Backup-pligten er opfyldt: der er endnu ingen bogført data, der kræver backup.");
    }
  } else if (due) {
    lines.push("Backup-pligten er IKKE opfyldt: en ny backup er forfalden.");
  } else {
    lines.push("Backup-pligten er opfyldt: seneste backup dækker den nuværende bogføring.");
  }
  lines.push("");

  if (result.latestBackupId) {
    lines.push(`  Seneste backup:                ${asText(result.latestBackupId)}`);
  } else {
    lines.push("  Seneste backup:                ingen registreret");
  }
  if (result.latestBackupAt) {
    lines.push(`  Taget:                         ${asText(result.latestBackupAt)}`);
  }
  const days = result.daysSinceLatestBackup;
  if (typeof days === "number" && Number.isFinite(days)) {
    lines.push(`  Dage siden seneste backup:     ${days}`);
  }
  lines.push(
    `  Ændringer siden seneste backup: ${hasActivity ? "ja" : "nej"}`,
  );
  lines.push("");

  if (backupsFound === 0 && hasActivity) {
    // No backup taken yet, but data has been booked — the duty is due now.
    lines.push("Tag en backup nu: der er bogført data, som endnu ikke er sikkerhedskopieret.");
  } else if (result.requiredBy) {
    const verb = due ? "Næste backup skulle have været taget senest" : "Næste backup skal tages senest";
    lines.push(`${verb} ${asText(result.requiredBy)}.`);
  } else if (!hasActivity) {
    lines.push("Ingen frist: backup kræves først, når der bogføres nye data.");
  }
  lines.push("Backup-pligten er ugentlig — tag en backup mindst hver 7. dag, når der bogføres.");
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Exceptions list -------------------------------------------------------

export function renderExceptionsList(result: Record<string, unknown>): string {
  const rows = asArray(result.rows);
  const lines: string[] = [];
  if (rows.length === 0) {
    lines.push("Ingen exceptions i køen for det valgte filter.");
    return lines.join("\n");
  }
  const open = rows.filter((row) => row.status === "open").length;
  lines.push(
    `Exceptions-kø: ${rows.length} sag${rows.length === 1 ? "" : "er"}` +
      (open > 0 ? ` (${open} åben${open === 1 ? "" : "e"})` : ""),
  );
  lines.push("");
  for (const row of rows) {
    // `severityDa`/`exceptionStatusDa` echo an unknown code back unchanged;
    // when the row carries no usable code at all (missing or empty), fall back
    // to `asText` so the value renders as the standard dash.
    const severityCode = asText(row.severity, "");
    const statusCodeRaw = asText(row.status, "");
    const severity = severityCode ? severityDa(severityCode, "short") : asText(row.severity);
    const status = statusCodeRaw ? exceptionStatusDa(statusCodeRaw) : asText(row.status);
    lines.push(`#${asText(row.id)} — ${asText(row.message)}`);
    lines.push(`  Type: ${asText(row.type)} | Alvorlighed: ${severity} | Status: ${status}`);
    if (row.requiredAction) {
      lines.push(`  Påkrævet handling: ${asText(row.requiredAction)}`);
    }
    const refs: string[] = [];
    if (row.relatedBankTransactionId != null) {
      refs.push(`banktransaktion ${asText(row.relatedBankTransactionId)}`);
    }
    if (row.relatedDocumentId != null) {
      refs.push(`bilag ${asText(row.relatedDocumentId)}`);
    }
    if (refs.length > 0) lines.push(`  Vedrører: ${refs.join(", ")}`);
    if (row.archived === true) lines.push("  (i arkiveret/lukket periode)");
    if (row.status === "resolved" && row.resolvedAt) {
      lines.push(
        `  Løst ${asText(row.resolvedAt)}` +
          (row.resolvedBy ? ` af ${asText(row.resolvedBy)}` : ""),
      );
    }
    lines.push("");
  }
  if (lines[lines.length - 1] === "") lines.pop();
  return lines.join("\n");
}

// --- Trial balance ---------------------------------------------------------

export function renderTrialBalance(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(
    `Saldobalance for perioden ${asText(result.periodStart)} til ${asText(result.periodEnd)}`,
  );
  lines.push("");
  const accounts = asArray(result.accounts);
  if (accounts.length === 0) {
    lines.push("Ingen konti har bevægelser i perioden.");
  } else {
    for (const account of accounts) {
      lines.push(
        `  ${asText(account.accountNo)} ${asText(account.name)}`,
      );
      lines.push(
        `      Debet ${formatKroner(account.debit)} | Kredit ${formatKroner(account.credit)} | Saldo ${formatKroner(account.balance)}`,
      );
    }
  }
  lines.push("");
  lines.push(`  Debet i alt:                   ${formatKroner(result.totalDebit)}`);
  lines.push(`  Kredit i alt:                  ${formatKroner(result.totalCredit)}`);
  lines.push(
    result.balanced === true
      ? "  Saldobalancen går op (debet = kredit)."
      : "  ADVARSEL: saldobalancen går ikke op (debet ≠ kredit).",
  );
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Profit and loss -------------------------------------------------------

export function renderProfitAndLoss(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(
    `Resultatopgørelse for perioden ${asText(result.periodStart)} til ${asText(result.periodEnd)}`,
  );
  lines.push("");
  const income = asArray(result.income);
  lines.push("Indtægter:");
  if (income.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const line of income) {
      lines.push(`  ${asText(line.accountNo)} ${asText(line.name)}: ${formatKroner(line.amount)}`);
    }
  }
  lines.push(`  Indtægter i alt:               ${formatKroner(result.totalIncome)}`);
  lines.push("");
  const expense = asArray(result.expense);
  lines.push("Omkostninger:");
  if (expense.length === 0) {
    lines.push("  (ingen)");
  } else {
    for (const line of expense) {
      lines.push(`  ${asText(line.accountNo)} ${asText(line.name)}: ${formatKroner(line.amount)}`);
    }
  }
  lines.push(`  Omkostninger i alt:            ${formatKroner(result.totalExpense)}`);
  lines.push("");
  const profit = typeof result.result === "number" ? result.result : Number(result.result);
  if (Number.isFinite(profit) && profit > 0) {
    lines.push(`Periodens resultat: overskud på ${formatKroner(profit)}`);
  } else if (Number.isFinite(profit) && profit < 0) {
    lines.push(`Periodens resultat: underskud på ${formatKroner(-profit)}`);
  } else {
    lines.push("Periodens resultat: 0,00 kr.");
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Balance sheet ---------------------------------------------------------

export function renderBalanceSheet(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`Balance pr. ${asText(result.asOfDate)}`);
  lines.push("");
  renderBalanceSection(lines, "Aktiver", result.assets);
  lines.push("");
  renderBalanceSection(lines, "Passiver", result.liabilities);
  lines.push("");
  renderBalanceSection(lines, "Egenkapital", result.equity);
  lines.push(`  Periodens resultat:`.padEnd(33) + formatKroner(result.periodResult));
  lines.push("");
  lines.push(`  Aktiver i alt:                 ${formatKroner(result.totalAssets)}`);
  lines.push(`  Passiver og egenkapital i alt: ${formatKroner(result.totalLiabilitiesAndEquity)}`);
  lines.push(
    result.balanced === true
      ? "  Balancen går op (aktiver = passiver + egenkapital)."
      : "  ADVARSEL: balancen går ikke op.",
  );
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Retention status ------------------------------------------------------

const RETENTION_TABLE_DA: Record<string, string> = {
  documents: "Bilag",
  journal_entries: "Finansposteringer",
  bank_transactions: "Banktransaktioner",
};

export function renderRetentionStatus(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`Opbevaringsstatus pr. ${asText(result.asOf)}`);
  lines.push("");
  const rows = asArray(result.rows);
  if (rows.length === 0) {
    lines.push("Ingen materiale registreret.");
  } else {
    for (const row of rows) {
      const name = RETENTION_TABLE_DA[String(row.table)] ?? asText(row.table);
      lines.push(`${name}:`);
      lines.push(`  I alt: ${asCount(row.total)} | Udløbet: ${asCount(row.expired)}`);
      if (row.nextExpiry) lines.push(`  Næste udløb: ${asText(row.nextExpiry)}`);
      if (row.oldestExpired) lines.push(`  Ældste udløbne: ${asText(row.oldestExpired)}`);
      lines.push("");
    }
    if (lines[lines.length - 1] === "") lines.pop();
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Contact lists (customers / vendors) -----------------------------------

/**
 * Render a `customer list` / `vendor list` payload as Danish human-readable
 * text. The payload is the `{ ok, count, rows }` object from
 * `listCustomers` / `listVendors`.
 */
export function renderContactList(
  result: Record<string, unknown>,
  heading: string,
  emptyMessage: string,
): string {
  if (result.ok === false) {
    return buildHumanError(heading, result).join("\n");
  }
  const rows = asArray(result.rows);
  const lines: string[] = [`${heading} (${rows.length})`];
  if (rows.length === 0) {
    lines.push(emptyMessage);
    return lines.join("\n");
  }
  for (const row of rows) {
    lines.push("");
    lines.push(`#${asText(row.id)} — ${asText(row.name)}`);
    if (row.vatOrCvr) lines.push(`  CVR/moms-nr.: ${asText(row.vatOrCvr)}`);
    if (row.address) lines.push(`  Adresse: ${asText(row.address)}`);
    if (row.email) lines.push(`  E-mail: ${asText(row.email)}`);
    if (row.phone) lines.push(`  Telefon: ${asText(row.phone)}`);
    if (row.eanNumber) lines.push(`  EAN-nummer: ${asText(row.eanNumber)}`);
    if (typeof row.paymentTermsDays === "number") {
      lines.push(`  Betalingsbetingelser: ${row.paymentTermsDays} dage`);
    }
    if (row.defaultCurrency) lines.push(`  Standardvaluta: ${asText(row.defaultCurrency)}`);
    if (row.defaultExpenseAccount) {
      lines.push(`  Standard omkostningskonto: ${asText(row.defaultExpenseAccount)}`);
    }
    if (row.defaultVatTreatment) {
      lines.push(`  Standard momsbehandling: ${asText(row.defaultVatTreatment)}`);
    }
    if (row.notes) lines.push(`  Noter: ${asText(row.notes)}`);
    if (row.archived === true) lines.push("  (arkiveret)");
  }
  return lines.join("\n");
}
