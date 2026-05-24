// CSV-eksport af de tre kerne-rapporter — Resultatopgørelse, Balance og
// Saldobalance (#372, atomar slice: CSV-only).
//
// Hver builder genbruger den eksisterende `buildCompanyIncomeStatement` /
// `buildCompanyBalance` / `buildCompanyTrialBalance` så CSV'en altid afspejler
// de samme tal som cockpittet viser på skærmen. Output er deterministisk:
// samme regnskab + samme år ⇒ byte-identisk CSV. UTF-8 med BOM (Excel/Numbers
// på dansk Windows åbner uden mojibake), CRLF linjeendelser per RFC 4180,
// stabile danske kolonnenavne i en header-række, og en kort metadata-blok
// først (virksomhed, CVR, periode, valuta, dato for udtræk) så bilaget kan
// arkiveres som dokumentation.
//
// PDF-eksporten er bevidst udskudt (#372 er auto-review:needs-split: PDF
// kræver en ny render-pipeline og ligger i et opfølger-issue).

import {
  buildCompanyBalance,
  buildCompanyIncomeStatement,
  buildCompanyTrialBalance,
  type BalanceLine,
  type IncomeStatementLine,
  type TrialBalanceRow,
} from "./statements";
import { roundKroner } from "./shared";

/** CRLF per RFC 4180 — Excel på Windows insisterer. */
const CRLF = "\r\n";

/** UTF-8 BOM — gør at Excel ikke læser CSV'en som CP-1252. */
const BOM = "﻿";

/**
 * Feltadskiller. Vi bruger semikolon i stedet for komma — det er
 * Excel-på-dansk-Windows konventionen og lader os bevare dansk decimalkomma
 * (`1000,00`) i tal-felter uden at de bliver split som to kolonner. Excel +
 * Numbers + Google Sheets auto-detekterer separatoren ud fra BOM-prefixed
 * `sep=;` præfiks eller fra header-rækken; vi sender en `sep=;`-preamble
 * for at gøre det 100 % sikkert.
 */
const SEP = ";";

/**
 * En enkelt CSV-celle. Felter med separator, citationstegn eller linjeskift
 * dobbeltquotes; interne `"` escapes som `""`. Tal formatteres med dansk
 * decimalkomma. Returnerer altid en streng (`null` ⇒ tom).
 */
export function csvCell(value: string | number | null | undefined): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return "";
    return formatDecimalDa(value);
  }
  const text = String(value);
  if (text.includes(SEP) || /["\r\n]/.test(text)) {
    return `"${text.replace(/"/g, '""')}"`;
  }
  return text;
}

/** Format a kroner-amount with two decimals and a Danish comma. */
function formatDecimalDa(value: number): string {
  const rounded = roundKroner(value);
  // toFixed(2) on a rounded value reliably gives two decimals without drift.
  return rounded.toFixed(2).replace(".", ",");
}

/** Joins a row's cells with `;` + CRLF — the canonical CSV record. */
function csvRow(cells: Array<string | number | null | undefined>): string {
  return cells.map(csvCell).join(SEP) + CRLF;
}

/** Strips characters not safe in a filename — keeps alnum, dash, underscore. */
function safeFilenameSegment(value: string): string {
  return value.replace(/[^A-Za-z0-9_-]/g, "-").replace(/-+/g, "-");
}

/**
 * CSV-eksport af Resultatopgørelsen for et regnskabsår.
 *
 * Header-rækken er: Konto, Navn, Beløb {år}, Beløb {år-1}.
 * Inden header er der en metadata-blok (virksomhed, CVR, valuta, år, dato
 * for udtræk) og en blank linje, så CSV'en kan arkiveres som selvstændigt
 * dokument og samtidig parses af Excel/Numbers (de stopper bare ved første
 * blank linje hvis man importerer "alle felter"; her bruger man Import →
 * fra linje 7).
 */
export type StatementCsvExport = {
  /** Den færdige CSV — string med UTF-8 BOM og CRLF. */
  content: string;
  /** Filnavn (slug + årstal + rapporttype), brugt i Content-Disposition. */
  filename: string;
};

/**
 * Valgfri ekstra-input til de tre eksport-byggere: lader test'en pinde en
 * deterministisk "Udtrukket"-dato. Produktion lader den være udefineret og
 * får dagens UTC-dato.
 */
export type StatementCsvOptions = {
  /** YYYY-MM-DD — overstyrer "Udtrukket"-feltet i metadata-blokken. */
  generatedAtIsoDate?: string;
};

export function exportIncomeStatementCsv(
  workspaceRoot: string,
  slug: string,
  year: number | null,
  opts: StatementCsvOptions = {},
): StatementCsvExport {
  const s = buildCompanyIncomeStatement(workspaceRoot, slug, year);
  const currentYear = s.selectedYear;
  const priorYear = String(parseInt(currentYear, 10) - 1);

  const sections: string[] = [];
  sections.push(metadataBlock("Resultatopgørelse", s.company, currentYear, opts));
  sections.push(
    csvRow(["Konto", "Navn", `Beløb ${currentYear}`, `Beløb ${priorYear}`]),
  );

  // Indtægter
  sections.push(csvRow(["", "Indtægter", "", ""]));
  for (const line of s.income as IncomeStatementLine[]) {
    sections.push(
      csvRow([line.accountNo, line.name, line.amount, line.priorAmount]),
    );
  }
  sections.push(
    csvRow(["", "Indtægter i alt", s.totalIncome, s.priorTotalIncome]),
  );

  // Udgifter
  sections.push(csvRow(["", "Udgifter", "", ""]));
  for (const line of s.expense as IncomeStatementLine[]) {
    sections.push(
      csvRow([line.accountNo, line.name, line.amount, line.priorAmount]),
    );
  }
  sections.push(
    csvRow(["", "Udgifter i alt", s.totalExpense, s.priorTotalExpense]),
  );

  // Resultat
  sections.push(csvRow(["", "Årets resultat", s.result, s.priorResult]));

  // `sep=;` preamble lader Excel auto-detektere semicolon-separator selv på
  // engelske Office-installationer; ignoreres af Numbers/Sheets uden skade.
  const content = BOM + `sep=${SEP}` + CRLF + sections.join("");
  const filename = `resultatopgorelse-${safeFilenameSegment(slug)}-${currentYear}.csv`;
  return { content, filename };
}

/**
 * CSV-eksport af Balancen pr. ultimo regnskabsår.
 *
 * Sektioner: Aktiver, Passiver, Egenkapital — hver med konto-linjer og en
 * total-linje. Synthetic-linjen "Årets resultat" (egenkapital) skrives med
 * tom konto-celle. Beløb-kolonne er ultimo-året; sammenlignings-kolonne er
 * ultimo året før (eller tom hvis ledgeren ingen forrige år har).
 */
export function exportBalanceCsv(
  workspaceRoot: string,
  slug: string,
  year: number | null,
  opts: StatementCsvOptions = {},
): StatementCsvExport {
  const b = buildCompanyBalance(workspaceRoot, slug, year);
  const currentYear = b.selectedYear;
  const priorYear = String(parseInt(currentYear, 10) - 1);

  const sections: string[] = [];
  sections.push(metadataBlock("Balance", b.company, currentYear, opts));
  sections.push(
    csvRow([
      "Konto",
      "Navn",
      `Pr. ${b.asOfDate}`,
      `Pr. ${priorYear}-12-31`,
    ]),
  );

  const writeSection = (
    heading: string,
    lines: BalanceLine[],
    total: number,
    priorTotal: number | null,
    totalLabel: string,
  ) => {
    sections.push(csvRow(["", heading, "", ""]));
    for (const line of lines) {
      sections.push(
        csvRow([
          line.accountNo === "—" ? "" : line.accountNo,
          line.name,
          line.amount,
          line.priorAmount,
        ]),
      );
    }
    sections.push(csvRow(["", totalLabel, total, priorTotal]));
  };

  writeSection(
    "Aktiver",
    b.assets.lines,
    b.assets.total,
    b.assets.priorTotal,
    "Aktiver i alt",
  );
  writeSection(
    "Passiver",
    b.liabilities.lines,
    b.liabilities.total,
    b.liabilities.priorTotal,
    "Gæld i alt",
  );
  writeSection(
    "Egenkapital",
    b.equity.lines,
    b.equity.total,
    b.equity.priorTotal,
    "Egenkapital i alt",
  );

  sections.push(
    csvRow([
      "",
      "Passiver og egenkapital i alt",
      b.totalLiabilitiesAndEquity,
      b.priorTotalLiabilitiesAndEquity,
    ]),
  );

  // `sep=;` preamble lader Excel auto-detektere semicolon-separator selv på
  // engelske Office-installationer; ignoreres af Numbers/Sheets uden skade.
  const content = BOM + `sep=${SEP}` + CRLF + sections.join("");
  const filename = `balance-${safeFilenameSegment(slug)}-${currentYear}.csv`;
  return { content, filename };
}

/**
 * CSV-eksport af Saldobalancen for et regnskabsår.
 *
 * Header: Konto, Navn, Type, Debet, Kredit, Saldo. En total-linje afslutter
 * filen — den er "balanceret" når debet i alt = kredit i alt.
 */
export function exportTrialBalanceCsv(
  workspaceRoot: string,
  slug: string,
  year: number | null,
  opts: StatementCsvOptions = {},
): StatementCsvExport {
  const t = buildCompanyTrialBalance(workspaceRoot, slug, year);
  const currentYear = t.selectedYear;

  const sections: string[] = [];
  sections.push(metadataBlock("Saldobalance", t.company, currentYear, opts));
  sections.push(
    csvRow(["Konto", "Navn", "Type", "Debet", "Kredit", "Saldo"]),
  );

  for (const row of t.rows as TrialBalanceRow[]) {
    sections.push(
      csvRow([row.accountNo, row.name, row.type, row.debit, row.credit, row.balance]),
    );
  }
  sections.push(
    csvRow(["", "I alt", "", t.totalDebit, t.totalCredit, ""]),
  );

  // `sep=;` preamble lader Excel auto-detektere semicolon-separator selv på
  // engelske Office-installationer; ignoreres af Numbers/Sheets uden skade.
  const content = BOM + `sep=${SEP}` + CRLF + sections.join("");
  const filename = `saldobalance-${safeFilenameSegment(slug)}-${currentYear}.csv`;
  return { content, filename };
}

/**
 * Skriver den lille metadata-blok hver CSV starter med — virksomhedsnavn,
 * CVR, valuta, regnskabsår, dato for udtræk og en undertekst om at filen er
 * genereret af Rentemester. Slutter med en blank linje før header-rækken
 * starter; det matcher revisor-konventionen "metadata først, så data".
 */
function metadataBlock(
  reportTitle: string,
  company: { name: string; cvr: string | null; currency: string | null; country: string | null },
  yearLabel: string,
  opts: StatementCsvOptions,
): string {
  let generatedAt: string;
  if (opts.generatedAtIsoDate) {
    generatedAt = opts.generatedAtIsoDate;
  } else {
    const today = new Date();
    const y = today.getUTCFullYear();
    const m = String(today.getUTCMonth() + 1).padStart(2, "0");
    const d = String(today.getUTCDate()).padStart(2, "0");
    generatedAt = `${y}-${m}-${d}`;
  }
  const currency = company.currency ?? "DKK";
  const lines: string[] = [];
  lines.push(csvRow(["Rapport", reportTitle]));
  lines.push(csvRow(["Virksomhed", company.name]));
  if (company.cvr) lines.push(csvRow(["CVR", company.cvr]));
  lines.push(csvRow(["Regnskabsår", yearLabel]));
  lines.push(csvRow(["Valuta", currency]));
  lines.push(csvRow(["Udtrukket", generatedAt]));
  lines.push(csvRow(["Kilde", "Rentemester"]));
  lines.push(CRLF);
  return lines.join("");
}
