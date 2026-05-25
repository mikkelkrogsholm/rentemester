// Import framework — the Dinero pre-cut-over fiscal-year archive. Issue #197
// (epic #173).
//
// A real Dinero export has one `<year>/` folder per fiscal year (e.g. 2023–
// 2026). Only the LATEST year is the cut-over year — its Primobeholdning rows
// become the posted opening balance (#194). The EARLIER years must NOT be
// posted into the hash-chained live ledger — but discarding them loses audit
// history and matching context.
//
// This module keeps the pre-cut-over years as a READ-ONLY ARCHIVE: each
// `<year>/Posteringer.csv` and `<year>/SaldoBalance.csv` is stored in the
// dedicated `import_archive_*` tables (src/core/schema.sql, #197), tagged by
// source system and fiscal year, clearly OUTSIDE `journal_entries` /
// `journal_lines`. The archive is queryable for audit and as matching context
// for the agent, and is never part of the live journal.
//
// The module is deliberately SELF-CONTAINED so the parallel #195 work on the
// shared `dinero.ts` does not collide with it. It exposes:
//   - `parseArchiveYears`    — pure: a `MultiArtifactSource` -> archive years.
//   - `archiveDineroYears`   — persists the pre-cut-over years to the DB.
//   - `queryArchive`         — the read path (CLI / MCP back this).
//   - `checkRollForward`     — the closing-balance roll-forward consistency
//                              check across the archived years + cut-over year.

import type { Database } from "bun:sqlite";
import type { MultiArtifactSource } from "./types";

const SYSTEM = "dinero";

// `Beløb` rounding: Dinero exports up to six decimals; archive amounts are
// compared/summed in øre-grained integers to keep the roll-forward check exact.
const SCALE = 100;

/** One archived `Posteringer.csv` line — a historical posting, never posted. */
export type ArchivePostingLine = {
  lineNo: number;
  accountNo: string;
  accountName: string;
  transactionDate: string;
  voucher: string;
  voucherType: string;
  text: string;
  vatType: string;
  amount: number;
  runningBalance: number | null;
};

/** One archived `SaldoBalance.csv` line — a year's closing account balance. */
export type ArchiveBalanceLine = {
  lineNo: number;
  accountNo: string;
  accountName: string;
  amount: number;
};

/** One pre-cut-over fiscal year, parsed from a `<year>/` export folder. */
export type ArchiveYear = {
  fiscalYear: number;
  postings: ArchivePostingLine[];
  balances: ArchiveBalanceLine[];
};

/** The outcome of parsing the pre-cut-over years out of an export. */
export type ParseArchiveResult = {
  ok: boolean;
  /** The cut-over (latest) fiscal year — NOT archived; posted to the ledger. */
  cutOverYear: number | null;
  /** Pre-cut-over years, ascending by `fiscalYear` — deterministic. */
  years: ArchiveYear[];
  errors: string[];
};

/** Splits a Dinero semicolon-delimited CSV record. The format has no quoting. */
function splitRecord(line: string): string[] {
  return line.split(";").map((cell) => cell.trim());
}

/**
 * Parses a Dinero `Beløb` cell — a signed decimal with a comma decimal
 * separator (e.g. `30116,010000`, `-40000,00`) — into a kroner Number.
 * Returns `null` on a malformed cell, `undefined` on an empty cell.
 */
function parseBelob(cell: string): number | null | undefined {
  const trimmed = cell.trim();
  if (trimmed.length === 0) return undefined;
  const normalised = trimmed.replace(",", ".");
  if (!/^-?\d+(\.\d+)?$/.test(normalised)) return null;
  const value = Number(normalised);
  return Number.isFinite(value) ? value : null;
}

/** Rounds a kroner amount to integer øre so balances compare exactly. */
function toOreInt(kroner: number): number {
  return Math.round(kroner * SCALE);
}

/**
 * Discovers every `<year>/` folder in a resolved export. A folder qualifies
 * when its name is a four-digit year and it carries a `Posteringer.csv`. The
 * result is sorted ascending so the caller sees a deterministic ordering.
 */
function discoverYears(input: MultiArtifactSource): number[] {
  const years = new Set<number>();
  for (const name of Object.keys(input.files)) {
    const match = /^(\d{4})\/Posteringer\.csv$/i.exec(name);
    if (match) years.add(Number(match[1]));
  }
  return [...years].sort((a, b) => a - b);
}

/**
 * Parses a `<year>/Posteringer.csv` into archived posting lines. EVERY data
 * row is archived (the Primobeholdning rows AND the year's movements) — the
 * archive is the year's full reference history, not just its opening balance.
 */
function parsePosteringer(text: string, sourceName: string, errors: string[]): ArchivePostingLine[] {
  const out: ArchivePostingLine[] = [];
  const lines = text.split(/\r?\n/);
  let sawHeader = false;
  let lineNo = 0;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line.length === 0) continue;
    const cells = splitRecord(line);
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "konto") {
        sawHeader = true;
        continue;
      }
      errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Dato;...' header row`);
      return [];
    }
    const accountNo = cells[0] ?? "";
    if (!accountNo) {
      errors.push(`${sourceName} line ${i + 1}: posting row is missing a Konto`);
      continue;
    }
    const amount = parseBelob(cells[7] ?? "");
    if (amount === null) {
      errors.push(
        `${sourceName} line ${i + 1}: posting row for account '${accountNo}' has an invalid Beløb '${cells[7] ?? ""}'`,
      );
      continue;
    }
    const runningBalance = parseBelob(cells[8] ?? "");
    if (runningBalance === null) {
      errors.push(
        `${sourceName} line ${i + 1}: posting row for account '${accountNo}' has an invalid Saldo '${cells[8] ?? ""}'`,
      );
      continue;
    }
    out.push({
      lineNo,
      accountNo,
      accountName: cells[1] ?? "",
      transactionDate: cells[2] ?? "",
      voucher: cells[3] ?? "",
      voucherType: cells[4] ?? "",
      text: cells[5] ?? "",
      vatType: cells[6] ?? "",
      amount: amount ?? 0,
      runningBalance: runningBalance ?? null,
    });
    lineNo += 1;
  }
  if (!sawHeader) {
    errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Dato;...' header row`);
  }
  return out;
}

/**
 * Parses a `<year>/SaldoBalance.csv` (`Konto;Kontonavn;Beløb`) into archived
 * closing-balance lines. A missing `SaldoBalance.csv` is not an error here —
 * `archiveDineroYears` simply archives the year without closing balances.
 */
function parseSaldoBalance(
  text: string,
  sourceName: string,
  errors: string[],
): ArchiveBalanceLine[] {
  const out: ArchiveBalanceLine[] = [];
  const lines = text.split(/\r?\n/);
  let sawHeader = false;
  let lineNo = 0;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line.length === 0) continue;
    const cells = splitRecord(line);
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "konto") {
        sawHeader = true;
        continue;
      }
      errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Beløb' header row`);
      return [];
    }
    const accountNo = cells[0] ?? "";
    if (!accountNo) {
      errors.push(`${sourceName} line ${i + 1}: balance row is missing a Konto`);
      continue;
    }
    const amount = parseBelob(cells[2] ?? "");
    if (amount === null || amount === undefined) {
      errors.push(
        `${sourceName} line ${i + 1}: balance row for account '${accountNo}' has an invalid Beløb '${cells[2] ?? ""}'`,
      );
      continue;
    }
    out.push({ lineNo, accountNo, accountName: cells[1] ?? "", amount });
    lineNo += 1;
  }
  if (!sawHeader) {
    errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Beløb' header row`);
  }
  return out;
}

/**
 * Extracts an export's `<year>/Posteringer.csv` Primobeholdning rows into a
 * per-account opening-balance map (account number -> kroner amount, signed:
 * positive = debit, negative = credit). This is the same Primobeholdning
 * marker `dinero.ts` keys on (`Bilag = 0`, `Tekst = Primobeholdning`).
 */
function primobeholdningOf(text: string): Map<string, number> {
  const map = new Map<string, number>();
  const lines = text.split(/\r?\n/);
  let sawHeader = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line.length === 0) continue;
    const cells = splitRecord(line);
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "konto") sawHeader = true;
      continue;
    }
    const accountNo = cells[0] ?? "";
    const voucher = (cells[3] ?? "").trim();
    const text2 = (cells[5] ?? "").trim().toLowerCase();
    if (voucher !== "0" || text2 !== "primobeholdning") continue;
    const amount = parseBelob(cells[7] ?? "");
    if (typeof amount === "number" && accountNo) map.set(accountNo, amount);
  }
  return map;
}

/**
 * Parses the pre-cut-over years out of a resolved Dinero export. The cut-over
 * year (the latest `<year>/` folder) is identified but NOT included in
 * `years` — only the EARLIER years are archived. An export with a single year
 * yields an empty `years` list and that year as `cutOverYear`.
 *
 * Pure and deterministic with respect to `input`.
 */
export function parseArchiveYears(input: MultiArtifactSource): ParseArchiveResult {
  const errors: string[] = [];
  const allYears = discoverYears(input);
  if (allYears.length === 0) {
    return { ok: true, cutOverYear: null, years: [], errors: [] };
  }
  const cutOverYear = allYears[allYears.length - 1]!;
  const preCutOver = allYears.slice(0, -1);

  const years: ArchiveYear[] = [];
  for (const year of preCutOver) {
    const posteringer = input.files[`${year}/Posteringer.csv`];
    if (!posteringer) {
      errors.push(`pre-cut-over year ${year} is missing Posteringer.csv`);
      continue;
    }
    const postings = parsePosteringer(posteringer.text, `${year}/Posteringer.csv`, errors);
    const saldoBalance = input.files[`${year}/SaldoBalance.csv`];
    const balances = saldoBalance
      ? parseSaldoBalance(saldoBalance.text, `${year}/SaldoBalance.csv`, errors)
      : [];
    years.push({ fiscalYear: year, postings, balances });
  }

  return { ok: errors.length === 0, cutOverYear, years, errors };
}

/** The result of persisting the pre-cut-over years into the archive tables. */
export type ArchiveResult = {
  ok: boolean;
  /** Fiscal years newly written to the archive by this call. */
  archivedYears: number[];
  /** Fiscal years already present in the archive — left untouched. */
  skippedYears: number[];
  /** Ordered, human-readable description of what the archiving did. */
  auditTrail: string[];
  errors: string[];
};

/**
 * Persists the pre-cut-over fiscal years of a resolved Dinero export into the
 * `import_archive_*` tables. The cut-over year is left for the live-ledger
 * import (#194) — it is NOT archived.
 *
 * A year already present in `import_archive_years` for the same source system
 * is skipped (idempotent re-import). Nothing here touches `journal_entries` /
 * `journal_lines`: the archive is reference data outside the live ledger.
 */
export function archiveDineroYears(db: Database, input: MultiArtifactSource): ArchiveResult {
  const auditTrail: string[] = [];
  const parsed = parseArchiveYears(input);
  if (!parsed.ok) {
    return {
      ok: false,
      archivedYears: [],
      skippedYears: [],
      auditTrail,
      errors: parsed.errors,
    };
  }

  if (parsed.years.length === 0) {
    auditTrail.push(
      parsed.cutOverYear == null
        ? "Export carries no fiscal-year folders — nothing to archive"
        : `Export carries only the cut-over year ${parsed.cutOverYear} — nothing to archive`,
    );
    return { ok: true, archivedYears: [], skippedYears: [], auditTrail, errors: [] };
  }

  const archivedYears: number[] = [];
  const skippedYears: number[] = [];

  const existsStmt = db.query(
    "SELECT id FROM import_archive_years WHERE source_system = ? AND fiscal_year = ?",
  );
  const insertYear = db.query(
    `INSERT INTO import_archive_years (source_system, fiscal_year, posting_count, balance_count)
     VALUES (?, ?, ?, ?)`,
  );
  const insertPosting = db.query(
    `INSERT INTO import_archive_postings
       (archive_year_id, line_no, account_no, account_name, transaction_date,
        voucher, voucher_type, text, vat_type, amount, running_balance)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  );
  const insertBalance = db.query(
    `INSERT INTO import_archive_balances
       (archive_year_id, line_no, account_no, account_name, amount)
     VALUES (?, ?, ?, ?, ?)`,
  );

  const persist = db.transaction((years: ArchiveYear[]) => {
    for (const year of years) {
      if (existsStmt.get(SYSTEM, year.fiscalYear)) {
        skippedYears.push(year.fiscalYear);
        auditTrail.push(`Fiscal year ${year.fiscalYear} already archived — skipped`);
        continue;
      }
      const info = insertYear.run(
        SYSTEM,
        year.fiscalYear,
        year.postings.length,
        year.balances.length,
      );
      const yearId = Number(info.lastInsertRowid);
      for (const p of year.postings) {
        insertPosting.run(
          yearId,
          p.lineNo,
          p.accountNo,
          p.accountName,
          p.transactionDate,
          p.voucher,
          p.voucherType,
          p.text,
          p.vatType,
          p.amount,
          p.runningBalance,
        );
      }
      for (const b of year.balances) {
        insertBalance.run(yearId, b.lineNo, b.accountNo, b.accountName, b.amount);
      }
      archivedYears.push(year.fiscalYear);
      auditTrail.push(
        `Archived fiscal year ${year.fiscalYear}: ${year.postings.length} posting(s), ` +
          `${year.balances.length} closing-balance line(s) — read-only, not posted`,
      );
    }
  });
  persist(parsed.years);

  return { ok: true, archivedYears, skippedYears, auditTrail, errors: [] };
}

/** A single archived fiscal year as returned by `queryArchive`. */
export type ArchivedYearView = {
  fiscalYear: number;
  sourceSystem: string;
  postingCount: number;
  balanceCount: number;
  importedAt: string;
  /** Present only when a specific `fiscalYear` was requested. */
  postings?: ArchivePostingLine[];
  /** Present only when a specific `fiscalYear` was requested. */
  balances?: ArchiveBalanceLine[];
};

/** The outcome of the archive read path. */
export type QueryArchiveResult = {
  ok: boolean;
  years: ArchivedYearView[];
  errors: string[];
};

/**
 * The archive READ PATH (CLI / MCP back this). Called with no `fiscalYear` it
 * lists every archived year (headers only). Called with a `fiscalYear` it
 * returns that year's header AND its full archived Posteringer / SaldoBalance
 * detail rows — for audit and as matching context for the agent.
 */
export function queryArchive(
  db: Database,
  options: { sourceSystem?: string; fiscalYear?: number } = {},
): QueryArchiveResult {
  const sourceSystem = options.sourceSystem ?? SYSTEM;
  const headerRows = db
    .query(
      `SELECT id, fiscal_year, source_system, posting_count, balance_count, imported_at
         FROM import_archive_years
        WHERE source_system = ?
          ${options.fiscalYear != null ? "AND fiscal_year = ?" : ""}
        ORDER BY fiscal_year ASC`,
    )
    .all(
      ...(options.fiscalYear != null ? [sourceSystem, options.fiscalYear] : [sourceSystem]),
    ) as Array<{
    id: number;
    fiscal_year: number;
    source_system: string;
    posting_count: number;
    balance_count: number;
    imported_at: string;
  }>;

  if (options.fiscalYear != null && headerRows.length === 0) {
    return {
      ok: false,
      years: [],
      errors: [`no archived ${sourceSystem} data for fiscal year ${options.fiscalYear}`],
    };
  }

  const years: ArchivedYearView[] = headerRows.map((row) => {
    const view: ArchivedYearView = {
      fiscalYear: row.fiscal_year,
      sourceSystem: row.source_system,
      postingCount: row.posting_count,
      balanceCount: row.balance_count,
      importedAt: row.imported_at,
    };
    if (options.fiscalYear != null) {
      view.postings = (
        db
          .query(
            `SELECT line_no, account_no, account_name, transaction_date, voucher,
                    voucher_type, text, vat_type, amount, running_balance
               FROM import_archive_postings
              WHERE archive_year_id = ?
              ORDER BY line_no ASC`,
          )
          .all(row.id) as Array<Record<string, unknown>>
      ).map((p) => ({
        lineNo: p.line_no as number,
        accountNo: p.account_no as string,
        accountName: (p.account_name as string) ?? "",
        transactionDate: (p.transaction_date as string) ?? "",
        voucher: (p.voucher as string) ?? "",
        voucherType: (p.voucher_type as string) ?? "",
        text: (p.text as string) ?? "",
        vatType: (p.vat_type as string) ?? "",
        amount: p.amount as number,
        runningBalance: (p.running_balance as number | null) ?? null,
      }));
      view.balances = (
        db
          .query(
            `SELECT line_no, account_no, account_name, amount
               FROM import_archive_balances
              WHERE archive_year_id = ?
              ORDER BY line_no ASC`,
          )
          .all(row.id) as Array<Record<string, unknown>>
      ).map((b) => ({
        lineNo: b.line_no as number,
        accountNo: b.account_no as string,
        accountName: (b.account_name as string) ?? "",
        amount: b.amount as number,
      }));
    }
    return view;
  });

  return { ok: true, years, errors: [] };
}

/** One account where a year's closing balance fails to roll into the next. */
export type RollForwardBreak = {
  /** The closing year whose SaldoBalance is the "from" side of the comparison. */
  fromYear: number;
  /** The next year (or the cut-over year) whose Primobeholdning is the "to" side. */
  toYear: number;
  accountNo: string;
  /** The archived closing balance (kroner). */
  closingAmount: number;
  /** The opening balance the next year actually carries (kroner). */
  openingAmount: number;
};

/** One step's outcome in the roll-forward chain. */
export type RollForwardStep = {
  fromYear: number;
  toYear: number;
  /** True when the "to" year is the cut-over year (posted opening balance). */
  toIsCutOver: boolean;
  ok: boolean;
  breaks: RollForwardBreak[];
};

/** The outcome of the closing-balance roll-forward consistency check. */
export type RollForwardResult = {
  ok: boolean;
  steps: RollForwardStep[];
  /** Flat list of every break across all steps, for a concise summary. */
  breaks: RollForwardBreak[];
  errors: string[];
};

/**
 * Compares one year's closing `SaldoBalance` against the next year's opening
 * `Primobeholdning`. A break is any account whose closing balance differs from
 * the opening balance the next year carries (compared in integer øre so a
 * six-decimal Dinero amount and a two-decimal one still match exactly).
 *
 * An account present on one side and absent on the other is a break unless its
 * amount is zero — a zero-balance account need not be carried forward.
 *
 * #198 — kun BALANCE-SHEET-konti (asset/liability/vat) ruller forward. Income
 * og expense lukkes til nul ved årsafslutning og deres netto-resultat overføres
 * til 'Overført resultat' (equity); en P&L-konto med closing != 0 og opening 0
 * er korrekt regnskab, ikke et brud. Equity-konti (især retained-earnings)
 * skifter også legitimt år for år. `accountTypeOf` slår account_type op i
 * `accounts`-tabellen (Dinero parseren reconcilerer typerne dér via #193);
 * ukendte konti behandles konservativt som balance-sheet, så et reelt brud
 * stadig flagges hvis kontoen ikke er seedet endnu.
 */
function compareRollForward(
  fromYear: number,
  toYear: number,
  closing: Map<string, number>,
  opening: Map<string, number>,
  accountTypeOf: (accountNo: string) => string | null,
): RollForwardBreak[] {
  const breaks: RollForwardBreak[] = [];
  const accounts = new Set<string>([...closing.keys(), ...opening.keys()]);
  for (const accountNo of [...accounts].sort()) {
    const type = accountTypeOf(accountNo);
    // P&L og equity ruller IKKE forward — skip dem.
    if (type === "income" || type === "expense" || type === "equity") continue;
    const closingAmount = closing.get(accountNo) ?? 0;
    const openingAmount = opening.get(accountNo) ?? 0;
    if (toOreInt(closingAmount) !== toOreInt(openingAmount)) {
      breaks.push({ fromYear, toYear, accountNo, closingAmount, openingAmount });
    }
  }
  return breaks;
}

/**
 * THE CONSISTENCY CHECK (#197).
 *
 * Verifies that each archived year's closing `SaldoBalance` rolls forward into
 * the NEXT year's opening `Primobeholdning`, and that the LAST archived year
 * rolls into the CUT-OVER year's opening balance (the primobalance posted to
 * the live ledger, #194). Any account that does not carry forward is flagged
 * as a `RollForwardBreak`.
 *
 * `input` is the resolved Dinero export — the source for every year's opening
 * Primobeholdning and the cut-over year. The archived closing balances are
 * read from `import_archive_balances`, so the check runs against what was
 * actually archived.
 *
 * A break never throws: the check reports `ok: false` with the breaks listed,
 * so the import flow can surface them clearly without aborting.
 */
export function checkRollForward(db: Database, input: MultiArtifactSource): RollForwardResult {
  const errors: string[] = [];
  const allYears = discoverYears(input);
  if (allYears.length < 2) {
    // Nothing to roll forward: a single-year (or empty) export.
    return { ok: true, steps: [], breaks: [], errors: [] };
  }
  const cutOverYear = allYears[allYears.length - 1]!;

  // Archived closing balances, keyed by fiscal year.
  const closingByYear = new Map<number, Map<string, number>>();
  const archivedRows = db
    .query(
      `SELECT y.fiscal_year AS fiscal_year, b.account_no AS account_no, b.amount AS amount
         FROM import_archive_balances b
         JOIN import_archive_years y ON y.id = b.archive_year_id
        WHERE y.source_system = ?`,
    )
    .all(SYSTEM) as Array<{ fiscal_year: number; account_no: string; amount: number }>;
  for (const row of archivedRows) {
    let map = closingByYear.get(row.fiscal_year);
    if (!map) {
      map = new Map<string, number>();
      closingByYear.set(row.fiscal_year, map);
    }
    map.set(row.account_no, row.amount);
  }

  // Opening Primobeholdning, keyed by fiscal year, straight from the export.
  const openingByYear = new Map<number, Map<string, number>>();
  for (const year of allYears) {
    const file = input.files[`${year}/Posteringer.csv`];
    if (file) openingByYear.set(year, primobeholdningOf(file.text));
  }

  // #198 — slå konto-type op én gang og giv det videre til compareRollForward.
  // `accounts`-tabellen er authoritative efter Dinero-parseren har reconcileret
  // chart-of-accounts (#193). Ukendte konti (ikke endnu seedet) returnerer
  // null, og compareRollForward behandler dem som balance-sheet (konservativt:
  // hellere false-positivt advare end at gemme et reelt brud).
  const accountTypes = new Map<string, string>();
  const typeRows = db
    .query("SELECT account_no AS accountNo, type AS type FROM accounts")
    .all() as Array<{ accountNo: string; type: string }>;
  for (const row of typeRows) {
    accountTypes.set(row.accountNo, row.type);
  }
  const accountTypeOf = (accountNo: string): string | null =>
    accountTypes.get(accountNo) ?? null;

  const steps: RollForwardStep[] = [];
  const allBreaks: RollForwardBreak[] = [];
  // Roll each archived year's closing balance into the next year's opening.
  for (let i = 0; i < allYears.length - 1; i += 1) {
    const fromYear = allYears[i]!;
    const toYear = allYears[i + 1]!;
    const closing = closingByYear.get(fromYear);
    if (!closing) {
      errors.push(
        `fiscal year ${fromYear} has no archived SaldoBalance — cannot verify roll-forward into ${toYear}`,
      );
      continue;
    }
    const opening = openingByYear.get(toYear);
    if (!opening) {
      errors.push(`fiscal year ${toYear} has no Posteringer.csv — cannot read its opening balance`);
      continue;
    }
    const breaks = compareRollForward(fromYear, toYear, closing, opening, accountTypeOf);
    steps.push({
      fromYear,
      toYear,
      toIsCutOver: toYear === cutOverYear,
      ok: breaks.length === 0,
      breaks,
    });
    allBreaks.push(...breaks);
  }

  return {
    ok: errors.length === 0 && allBreaks.length === 0,
    steps,
    breaks: allBreaks,
    errors,
  };
}

/**
 * Formats the roll-forward result into ordered, human-readable audit lines —
 * one per step, with each break spelled out. Used by the import flow and the
 * CLI/MCP read path so a break is surfaced clearly, never silently dropped.
 */
export function describeRollForward(result: RollForwardResult): string[] {
  const lines: string[] = [];
  for (const step of result.steps) {
    const target = step.toIsCutOver ? `cut-over year ${step.toYear}` : `${step.toYear}`;
    if (step.ok) {
      lines.push(`Roll-forward ${step.fromYear} -> ${target}: closing balances carry forward`);
    } else {
      lines.push(
        `Roll-forward BREAK ${step.fromYear} -> ${target}: ${step.breaks.length} account(s) do not carry forward`,
      );
      for (const b of step.breaks) {
        lines.push(
          `  account ${b.accountNo}: closing ${b.closingAmount} != opening ${b.openingAmount}`,
        );
      }
    }
  }
  for (const error of result.errors) lines.push(`Roll-forward check error: ${error}`);
  return lines;
}
