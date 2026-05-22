import { createHash } from "node:crypto";
import { basename, join } from "node:path";
import { copyFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { retainUntilForDate } from "./retention";
import { addDkk, compareDkk, equalsDkk, multiplyDkk, normalizeCurrency, roundDkk, roundRate6, subtractDkk } from "./money";
// ===== BANK CLUSTER (#186,#189) =====
import {
  getBankProfile,
  listBankProfileNames,
  type BankImportProfile,
  type ProfileFieldName,
} from "./bank-profiles";
import { recordException } from "./exceptions";
// ===== END BANK CLUSTER (#186,#189) =====

export type BankImportRow = {
  transactionDate: string;
  bookingDate?: string;
  text: string;
  amount: number;
  currency?: string;
  reference?: string;
  amountDkk?: number;
  fxRateToDkk?: number;
  // ===== BANK CLUSTER (#188,#189) =====
  // Extra columns preserved from the source export when a profile supplies
  // them. All optional; generic CSV imports leave them undefined.
  counterpartyName?: string;
  counterpartyAccount?: string;
  message?: string;
  archiveReference?: string;
  customerReference?: string;
  balanceAfter?: number;
  raw?: Record<string, string>;
  // ===== END BANK CLUSTER (#188,#189) =====
};

// ===== BANK CLUSTER (#187) =====
export type BankAccount = {
  id: number;
  slug: string;
  name: string;
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
  currency: string;
  ledgerAccountNo: string | null;
  active: boolean;
  createdAt: string;
};

export type AddBankAccountInput = {
  name: string;
  slug?: string;
  bankName?: string;
  registrationNo?: string;
  accountNo?: string;
  iban?: string;
  currency?: string;
  ledgerAccountNo?: string;
};

function slugify(value: string) {
  return value
    .trim()
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function mapBankAccountRow(row: any): BankAccount {
  return {
    id: Number(row.id),
    slug: row.slug,
    name: row.name,
    bankName: row.bank_name ?? null,
    registrationNo: row.registration_no ?? null,
    accountNo: row.account_no ?? null,
    iban: row.iban ?? null,
    currency: row.currency,
    ledgerAccountNo: row.ledger_account_no ?? null,
    active: Number(row.active) === 1,
    createdAt: row.created_at,
  };
}

/** Resolves a bank account by numeric id or slug. Returns null when absent. */
export function resolveBankAccount(db: Database, idOrSlug: string | number): BankAccount | null {
  const asNumber = typeof idOrSlug === "number" ? idOrSlug : Number(idOrSlug);
  const byId = Number.isInteger(asNumber) && asNumber > 0
    ? db.query("SELECT * FROM bank_accounts WHERE id = ?").get(asNumber)
    : null;
  if (byId) return mapBankAccountRow(byId);
  const slug = String(idOrSlug).trim();
  const bySlug = slug ? db.query("SELECT * FROM bank_accounts WHERE slug = ?").get(slug) : null;
  return bySlug ? mapBankAccountRow(bySlug) : null;
}

export function listBankAccounts(db: Database, includeInactive = true) {
  const rows = db.query(
    `SELECT * FROM bank_accounts ${includeInactive ? "" : "WHERE active = 1"} ORDER BY id ASC`,
  ).all() as any[];
  return { ok: true as const, count: rows.length, accounts: rows.map(mapBankAccountRow), errors: [] as string[] };
}

export function addBankAccount(db: Database, input: AddBankAccountInput) {
  const errors: string[] = [];
  const name = input.name?.trim();
  if (!name) errors.push("name is required");
  const slug = (input.slug?.trim() ? slugify(input.slug) : slugify(name ?? "")) || "";
  if (!slug) errors.push("slug could not be derived; pass an explicit --slug");
  const currency = normalizeCurrency(input.currency);
  if (currency.length !== 3) errors.push("currency must be a 3-letter ISO currency code");
  if (errors.length > 0) return { ok: false as const, account: undefined, errors };

  if (db.query("SELECT id FROM bank_accounts WHERE slug = ?").get(slug)) {
    return { ok: false as const, account: undefined, errors: [`a bank account with slug '${slug}' already exists`] };
  }
  const ledgerAccountNo = input.ledgerAccountNo?.trim() || null;
  if (ledgerAccountNo && !db.query("SELECT account_no FROM accounts WHERE account_no = ?").get(ledgerAccountNo)) {
    return { ok: false as const, account: undefined, errors: [`ledger account ${ledgerAccountNo} does not exist`] };
  }

  const row = db.query(
    `INSERT INTO bank_accounts (slug, name, bank_name, registration_no, account_no, iban, currency, ledger_account_no)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *`,
  ).get(
    slug,
    name,
    input.bankName?.trim() || null,
    input.registrationNo?.trim() || null,
    input.accountNo?.trim() || null,
    input.iban?.trim() || null,
    currency,
    ledgerAccountNo,
  );
  insertAuditLog(db, {
    eventType: "bank_account_add",
    entityType: "bank_account",
    entityId: String((row as any).id),
    message: `Added bank account '${slug}' (${name})`,
  });
  return { ok: true as const, account: mapBankAccountRow(row), errors: [] as string[] };
}
// ===== END BANK CLUSTER (#187) =====

export type BankImportResult = {
  ok: boolean;
  importBatchId?: string;
  imported?: number;
  skippedDuplicates?: number;
  /** Human-readable descriptions of rows skipped as duplicates, for review. */
  skippedDuplicateRows?: string[];
  sourceFileHash?: string;
  // ===== BANK CLUSTER (#186-189) =====
  /** Numeric id of the bank account these rows were imported into (#187). */
  bankAccountId?: number;
  /** Slug of the bank account these rows were imported into (#187). */
  bankAccountSlug?: string;
  /** Name of the CSV import profile used, when one was supplied (#186). */
  profile?: string;
  /** Running-balance continuity warnings detected after import (#189). */
  balanceWarnings?: string[];
  /** First/last running balance of the batch when a profile supplied it (#189). */
  firstBalance?: number;
  lastBalance?: number;
  // ===== END BANK CLUSTER (#186-189) =====
  errors: string[];
};

const RULE_ID = "DK-BOOKKEEPING-BANK-IMPORT-001";
const FX_RULE_ID = "DK-BOOKKEEPING-FX-001";
// ===== BANK CLUSTER (#189) =====
const BALANCE_CONTINUITY_RULE_ID = "DK-BANK-BALANCE-CONTINUITY-001";
// ===== END BANK CLUSTER (#189) =====

function sha256Bytes(data: Uint8Array | string) {
  return createHash("sha256").update(data).digest("hex");
}


function normalizeAmount(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? roundDkk(value) : NaN;
}

const HEADER_ALIASES: Record<string, string[]> = {
  transaction_date: ["transaction_date", "bogføringsdato", "bogforingsdato", "dato", "date"],
  booking_date: ["booking_date", "rentedato", "valørdato", "valordato", "posteringsdato"],
  text: ["text", "tekst", "description", "beskrivelse"],
  amount: ["amount", "beløb", "belob"],
  currency: ["currency", "valuta"],
  reference: ["reference", "ref", "bilagsnummer"],
  amount_dkk: ["amount_dkk", "beløb_dkk", "belob_dkk"],
  fx_rate_to_dkk: ["fx_rate_to_dkk", "kurs", "valutakurs"],
};

const REQUIRED_COLUMNS = ["transaction_date", "text", "amount"] as const;

type CsvParseResult = {
  rows: Record<string, string>[];
  errors: string[];
};

function normalizeHeader(value: string) {
  return value
    .trim()
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/\s+/g, "_");
}

function canonicalHeader(value: string) {
  const normalized = normalizeHeader(value);
  for (const [canonical, aliases] of Object.entries(HEADER_ALIASES)) {
    if (aliases.map(normalizeHeader).includes(normalized)) return canonical;
  }
  return normalized;
}

function countDelimiterOutsideQuotes(line: string, delimiter: string) {
  let count = 0;
  let inQuotes = false;
  for (let idx = 0; idx < line.length; idx += 1) {
    const char = line[idx];
    if (char === '"') {
      if (inQuotes && line[idx + 1] === '"') idx += 1;
      else inQuotes = !inQuotes;
    } else if (!inQuotes && char === delimiter) {
      count += 1;
    }
  }
  return count;
}

function detectDelimiter(headerLine: string) {
  const candidates = [",", ";", "\t"];
  return candidates
    .map((delimiter) => ({ delimiter, count: countDelimiterOutsideQuotes(headerLine, delimiter) }))
    .sort((a, b) => b.count - a.count)[0]?.delimiter ?? ",";
}

function parseCsvLine(line: string, delimiter: string) {
  const values: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let idx = 0; idx < line.length; idx += 1) {
    const char = line[idx];
    if (char === '"') {
      if (inQuotes && line[idx + 1] === '"') {
        current += '"';
        idx += 1;
      } else {
        inQuotes = !inQuotes;
      }
    } else if (!inQuotes && char === delimiter) {
      values.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  values.push(current.trim());
  return { values, unterminatedQuote: inQuotes };
}

function parseCsv(content: string): CsvParseResult {
  const lines = content.replace(/^\uFEFF/, "").split(/\r?\n/).filter((line) => line.trim().length > 0);
  if (lines.length < 2) return { rows: [], errors: [] };

  const delimiter = detectDelimiter(lines[0]);
  const headerParsed = parseCsvLine(lines[0], delimiter);
  const header = headerParsed.values.map(canonicalHeader);
  const errors: string[] = [];
  if (headerParsed.unterminatedQuote) errors.push("CSV header has unterminated quoted field");
  // Two source columns must not canonicalise to the same key, otherwise the
  // row object silently keeps only the rightmost column (last wins).
  const seenHeaders = new Set<string>();
  for (const key of header) {
    if (seenHeaders.has(key)) {
      errors.push(`CSV header has duplicate canonical column: ${key}`);
    }
    seenHeaders.add(key);
  }
  for (const required of REQUIRED_COLUMNS) {
    if (!header.includes(required)) {
      errors.push(`CSV header missing required column: ${required} (accepted: ${HEADER_ALIASES[required].join(", ")})`);
    }
  }

  const rows: Record<string, string>[] = [];
  for (const [lineOffset, line] of lines.slice(1).entries()) {
    const lineNumber = lineOffset + 2;
    const parsed = parseCsvLine(line, delimiter);
    if (parsed.unterminatedQuote) errors.push(`CSV row ${lineNumber} has unterminated quoted field`);
    if (parsed.values.length !== header.length) {
      errors.push(`CSV row ${lineNumber} has ${parsed.values.length} fields, header has ${header.length}`);
      continue;
    }
    const row: Record<string, string> = {};
    header.forEach((key, idx) => row[key] = parsed.values[idx] ?? "");
    rows.push(row);
  }
  return { rows, errors };
}

// Strict numeric token: optional sign, digits, optional single dot + fraction.
// Rejects hex (0xff), scientific notation (1e3), Infinity, NaN, etc.
const STRICT_NUMERIC = /^[+-]?\d+(\.\d+)?$/;

function parseLocalizedNumber(value: string | undefined) {
  if (!value) return undefined;
  let trimmed = value.trim();
  if (!trimmed) return undefined;
  // Strip a trailing 3-letter ISO currency code that is whitespace-separated
  // from the number (e.g. "1234,56 DKK"). The whitespace requirement avoids
  // mangling garbage like "0xff" into a plausible amount.
  trimmed = trimmed.replace(/\s+[A-Za-z]{3}$/, "").trim();
  // Surrounding parentheses denote a negative amount in many bank exports.
  let sign = "";
  const paren = /^\((.*)\)$/.exec(trimmed);
  if (paren) {
    sign = "-";
    trimmed = paren[1].trim();
  }
  const compact = trimmed.replace(/\s/g, "");

  let normalized: string;
  if (compact.includes(",")) {
    // Comma present: comma is the decimal separator, dots are thousands.
    normalized = compact.replace(/\./g, "").replace(",", ".");
  } else if (/^[+-]?\d+(\.\d{3})+$/.test(compact)) {
    // Only digits and dots, every dot followed by exactly 3 digits and no
    // comma => Danish thousands grouping (e.g. "1.234" => 1234, "1.234.567").
    normalized = compact.replace(/\./g, "");
  } else {
    // Otherwise treat a single dot as a decimal point ("1234.56", "1234").
    normalized = compact;
  }

  normalized = sign + normalized;
  if (!STRICT_NUMERIC.test(normalized)) return NaN;
  return Number(normalized);
}

function normalizeDateText(value: string | undefined) {
  const trimmed = value?.trim();
  if (!trimmed) return undefined;
  if (/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return trimmed;
  const match = /^(\d{2})[-/.](\d{2})[-/.](\d{4})$/.exec(trimmed);
  if (!match) return trimmed;
  const first = Number(match[1]);
  const second = Number(match[2]);
  // dd?dd?dddd is only unambiguously DD-MM-YYYY when one of the first two
  // components cannot be a month. If both are <= 12 the D/M order is a guess
  // (e.g. 05/04/2026), so refuse to reformat and let date validation reject it
  // rather than silently picking an interpretation.
  if (first <= 12 && second <= 12 && first !== second) return trimmed;
  return `${match[3]}-${match[2]}-${match[1]}`;
}

function normalizeAmountOrNull(value: unknown) {
  if (value === undefined || value === null) return null;
  const normalized = normalizeAmount(value);
  return Number.isFinite(normalized) ? normalized : null;
}

function toRow(input: Record<string, string>): BankImportRow {
  return {
    transactionDate: normalizeDateText(input.transaction_date) ?? "",
    bookingDate: normalizeDateText(input.booking_date),
    text: input.text,
    amount: parseLocalizedNumber(input.amount) ?? NaN,
    currency: normalizeCurrency(input.currency),
    reference: input.reference || undefined,
    amountDkk: parseLocalizedNumber(input.amount_dkk),
    fxRateToDkk: parseLocalizedNumber(input.fx_rate_to_dkk),
  };
}

// ===== BANK CLUSTER (#186) =====
/**
 * Normalises a date to ISO using a profile's *declared* day/month order. A
 * profile is an explicit statement of the format, so unlike the generic
 * `normalizeDateText` it never refuses an ambiguous `dd.mm.yyyy` — the order
 * was declared, not guessed. An impossible calendar date still fails later in
 * `validateBankImportRows`.
 */
function normalizeDateWithOrder(value: string | undefined, order: BankImportProfile["dateOrder"]) {
  const trimmed = value?.trim();
  if (!trimmed) return undefined;
  if (/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return trimmed;
  if (order === "iso") return trimmed;
  const ymd = /^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$/.exec(trimmed);
  if (order === "ymd" && ymd) {
    return `${ymd[1]}-${ymd[2].padStart(2, "0")}-${ymd[3].padStart(2, "0")}`;
  }
  const dmyMatch = /^(\d{1,2})[-/.](\d{1,2})[-/.](\d{4})$/.exec(trimmed);
  if (!dmyMatch) return trimmed;
  const a = dmyMatch[1].padStart(2, "0");
  const b = dmyMatch[2].padStart(2, "0");
  const year = dmyMatch[3];
  if (order === "mdy") return `${year}-${a}-${b}`;
  // dmy (default for non-ISO)
  return `${year}-${b}-${a}`;
}

/**
 * Decodes raw CSV bytes for a profile. UTF-8 (BOM tolerated) is the common
 * case; Windows-1252 is decoded for legacy Danish exports. A profile that
 * declares utf8 still falls back to a 1252 decode if the UTF-8 decode produced
 * replacement characters, so a mislabelled file is not silently mangled.
 */
function decodeForProfile(bytes: Uint8Array, encoding: BankImportProfile["encoding"]) {
  if (encoding === "windows-1252") {
    return new TextDecoder("windows-1252").decode(bytes);
  }
  const utf8 = new TextDecoder("utf-8").decode(bytes);
  if (utf8.includes("�")) {
    return new TextDecoder("windows-1252").decode(bytes);
  }
  return utf8;
}

type ProfileParseResult = { rows: BankImportRow[]; errors: string[] };

/**
 * Parses a CSV file with a pinned profile: fixed delimiter, declared encoding,
 * declared date order and an explicit column->field map. No alias guessing.
 */
export function parseCsvWithProfile(bytes: Uint8Array, profile: BankImportProfile): ProfileParseResult {
  const content = decodeForProfile(bytes, profile.encoding).replace(/^﻿/, "");
  const lines = content.split(/\r?\n/).filter((line) => line.trim().length > 0);
  if (lines.length < 2) return { rows: [], errors: [] };

  const headerParsed = parseCsvLine(lines[0], profile.delimiter);
  const errors: string[] = [];
  if (headerParsed.unterminatedQuote) errors.push("CSV header has unterminated quoted field");

  // Map each header column index to the canonical field the profile assigns.
  const columnField: (ProfileFieldName | null)[] = headerParsed.values.map((header) => {
    const key = normalizeHeader(header).replace(/_/g, " ");
    return profile.columns[key] ?? null;
  });
  for (const required of ["transaction_date", "text", "amount"] as const) {
    if (!columnField.includes(required)) {
      errors.push(`profile '${profile.name}' could not map a column to required field: ${required}`);
    }
  }
  if (errors.length > 0) return { rows: [], errors };

  const rows: BankImportRow[] = [];
  for (const [lineOffset, line] of lines.slice(1).entries()) {
    const lineNumber = lineOffset + 2;
    const parsed = parseCsvLine(line, profile.delimiter);
    if (parsed.unterminatedQuote) errors.push(`CSV row ${lineNumber} has unterminated quoted field`);
    if (parsed.values.length !== headerParsed.values.length) {
      errors.push(`CSV row ${lineNumber} has ${parsed.values.length} fields, header has ${headerParsed.values.length}`);
      continue;
    }
    const fields: Partial<Record<ProfileFieldName, string>> = {};
    const raw: Record<string, string> = {};
    headerParsed.values.forEach((header, idx) => {
      raw[header] = parsed.values[idx] ?? "";
      const field = columnField[idx];
      // First non-empty wins (e.g. Afsender vs Oprindelig afsender both map to
      // counterparty_name).
      if (field && !(fields[field]?.trim())) fields[field] = parsed.values[idx] ?? "";
    });

    rows.push({
      transactionDate: normalizeDateWithOrder(fields.transaction_date, profile.dateOrder) ?? "",
      bookingDate: normalizeDateWithOrder(fields.booking_date, profile.dateOrder),
      text: fields.text ?? "",
      amount: parseLocalizedNumber(fields.amount) ?? NaN,
      currency: normalizeCurrency(fields.currency),
      reference: fields.reference || undefined,
      amountDkk: parseLocalizedNumber(fields.amount_dkk),
      fxRateToDkk: parseLocalizedNumber(fields.fx_rate_to_dkk),
      counterpartyName: fields.counterparty_name || undefined,
      counterpartyAccount: fields.counterparty_account || undefined,
      message: fields.message || undefined,
      archiveReference: fields.archive_reference || undefined,
      customerReference: fields.customer_reference || undefined,
      balanceAfter: parseLocalizedNumber(fields.balance_after),
      raw,
    });
  }
  return { rows, errors };
}
// ===== END BANK CLUSTER (#186) =====

export function validateBankImportRows(rows: BankImportRow[]) {
  const errors: string[] = [];
  let needsFxRule = false;
  rows.forEach((row, idx) => {
    if (!looksLikeIsoDate(row.transactionDate)) errors.push(`rows[${idx}].transactionDate must be YYYY-MM-DD`);
    if (row.bookingDate && !looksLikeIsoDate(row.bookingDate)) errors.push(`rows[${idx}].bookingDate must be YYYY-MM-DD when present`);
    if (typeof row.text !== "string" || row.text.trim().length === 0) errors.push(`rows[${idx}].text is required`);
    if (!Number.isFinite(normalizeAmount(row.amount))) errors.push(`rows[${idx}].amount must be numeric`);

    const currency = normalizeCurrency(row.currency);
    if (currency.length !== 3) errors.push(`rows[${idx}].currency must be a 3-letter ISO currency code`);
    if (currency !== "DKK") {
      needsFxRule = true;
      if (!Number.isFinite(normalizeAmount(row.amountDkk))) errors.push(`rows[${idx}].amountDkk is required for non-DKK rows`);
      if (!Number.isFinite(roundRate6(Number(row.fxRateToDkk))) || (row.fxRateToDkk ?? 0) <= 0) errors.push(`rows[${idx}].fxRateToDkk must be positive for non-DKK rows`);
      if (Number.isFinite(normalizeAmount(row.amountDkk)) && Number.isFinite(Number(row.fxRateToDkk))) {
        const expectedAmountDkk = multiplyDkk(row.amount ?? 0, row.fxRateToDkk ?? 0);
        if (compareDkk(normalizeAmount(row.amountDkk), expectedAmountDkk) !== 0) {
          errors.push(`rows[${idx}].amountDkk must equal amount * fxRateToDkk (${expectedAmountDkk})`);
        }
      }
    }
  });
  return { ok: errors.length === 0, appliedRules: needsFxRule ? [RULE_ID, FX_RULE_ID] : [RULE_ID], errors };
}

// `occurrence` is the 0-based count of preceding rows in the SAME file with
// identical field content. This keeps the fingerprint deterministic on
// re-import while letting two legitimately-distinct identical transactions
// (e.g. two 50 kr fees on the same day) both import instead of one being
// wrongly skipped as a coarse-fingerprint duplicate.
//
// bank_account_id (#187) is part of the fingerprint so an identical
// transaction (same date/text/amount) in two different accounts is not
// wrongly skipped as a cross-account duplicate.
function transactionFingerprint(row: BankImportRow, occurrence: number, bankAccountId: number | null) {
  return sha256Bytes(JSON.stringify({
    bank_account_id: bankAccountId,
    transaction_date: row.transactionDate,
    booking_date: row.bookingDate ?? null,
    text: row.text.trim(),
    amount: normalizeAmount(row.amount),
    currency: normalizeCurrency(row.currency),
    reference: row.reference ?? null,
    amount_dkk: normalizeAmountOrNull(row.amountDkk),
    fx_rate_to_dkk: row.fxRateToDkk == null ? null : roundRate6(row.fxRateToDkk),
    occurrence,
  }));
}

// ===== BANK CLUSTER (#189) =====
type BalanceContinuityResult = {
  balanceWarnings?: string[];
  firstBalance?: number;
  lastBalance?: number;
};

type BalanceRow = {
  id: number;
  transaction_date: string;
  text: string;
  amount: number;
  balance_after: number | null;
};

/**
 * Reconstructs true intra-day order for one date's rows (#191).
 *
 * Bank exports are commonly ordered newest-first, so within a single date the
 * file lists same-day rows in the reverse of their true sequence. The walk must
 * not rely on insertion order. `balance_after` itself disambiguates: in a
 * correct cluster each row's balance equals the previous balance ± its amount,
 * so the rows form a chain `balance_after - amount == previous balance_after`.
 *
 * Starting from `incoming` (the balance carried in from the previous date, or
 * `null` for the very first cluster), this greedily walks that chain. When the
 * cluster chains cleanly the rows are returned in true order; when no clean
 * chain exists (a genuine gap) the rows are returned in insertion order so the
 * continuity walk still surfaces the break.
 */
function orderDayCluster(cluster: BalanceRow[], incoming: number | null): BalanceRow[] {
  if (cluster.length < 2) return cluster;

  const remaining = [...cluster];
  const ordered: BalanceRow[] = [];

  // For the first cluster there is no carried-in balance; the start row is the
  // one whose pre-transaction balance matches no other row's balance_after.
  let running = incoming;
  if (running === null) {
    const balancesAfter = cluster.map((row) => Number(row.balance_after));
    const start = cluster.find(
      (row) => !balancesAfter.some((bal) => equalsDkk(bal, subtractDkk(Number(row.balance_after), Number(row.amount)))),
    );
    if (!start) return cluster;
    running = subtractDkk(Number(start.balance_after), Number(start.amount));
  }

  while (remaining.length > 0) {
    const nextIdx = remaining.findIndex(
      (row) => equalsDkk(subtractDkk(Number(row.balance_after), Number(row.amount)), running as number),
    );
    if (nextIdx === -1) return cluster; // no clean chain: keep insertion order
    const [next] = remaining.splice(nextIdx, 1);
    ordered.push(next);
    running = Number(next.balance_after);
  }
  return ordered;
}

/**
 * Verifies running-balance continuity over a freshly-imported batch (#189).
 *
 * Most bank statements carry a running balance (`Saldo`) on every row. In a
 * complete, correctly ordered statement each row's balance equals the previous
 * row's balance plus this row's amount. A break means rows are missing,
 * duplicated or misordered — a silent import error that would undermine a
 * ledger meant to be trusted.
 *
 * Rows are walked in transaction-date order. Within a date the file's insertion
 * order is unreliable (newest-first exports list same-day rows reversed), so the
 * true intra-day sequence is reconstructed from `balance_after` before the walk
 * (#191). A break is surfaced as a warning AND a `BANK_BALANCE_GAP`
 * exception-queue entry; it never fails the import, because a legitimately
 * partial export is a valid input. Comparison is integer-øre (`equalsDkk`) so
 * float artefacts do not raise a false break.
 *
 * Rows without a stored balance (generic CSV import, or a profile column the
 * file omitted) are skipped — there is nothing to check.
 */
function verifyBatchBalanceContinuity(db: Database, importedRowIds: number[]): BalanceContinuityResult {
  if (importedRowIds.length === 0) return {};
  const placeholders = importedRowIds.map(() => "?").join(", ");
  const rows = db.query(
    `SELECT id, transaction_date, text, amount, balance_after
     FROM bank_transactions
     WHERE id IN (${placeholders})
     ORDER BY transaction_date ASC, id ASC`,
  ).all(...importedRowIds) as BalanceRow[];

  const rowsWithBalance = rows.filter((row) => row.balance_after !== null && row.balance_after !== undefined);
  if (rowsWithBalance.length < 2) {
    // 0 rows: nothing to report. 1 row: a single balance, no adjacency to check.
    return rowsWithBalance.length === 1
      ? { firstBalance: roundDkk(Number(rowsWithBalance[0].balance_after)), lastBalance: roundDkk(Number(rowsWithBalance[0].balance_after)) }
      : {};
  }

  // Reconstruct true intra-day order before the walk (#191). The DB query
  // returns rows in (transaction_date, id) order; `id` is insertion order =
  // file order, which is reversed within a date for a newest-first export.
  // Each date's rows are re-chained from `balance_after`, carrying the running
  // balance across date boundaries.
  const withBalance: BalanceRow[] = [];
  let clusterStart = 0;
  while (clusterStart < rowsWithBalance.length) {
    let clusterEnd = clusterStart + 1;
    while (
      clusterEnd < rowsWithBalance.length &&
      rowsWithBalance[clusterEnd].transaction_date === rowsWithBalance[clusterStart].transaction_date
    ) {
      clusterEnd += 1;
    }
    const cluster = rowsWithBalance.slice(clusterStart, clusterEnd);
    const incoming = withBalance.length > 0 ? Number(withBalance[withBalance.length - 1].balance_after) : null;
    withBalance.push(...orderDayCluster(cluster, incoming));
    clusterStart = clusterEnd;
  }

  const balanceWarnings: string[] = [];
  for (let idx = 1; idx < withBalance.length; idx += 1) {
    const prev = withBalance[idx - 1];
    const curr = withBalance[idx];
    const expected = addDkk(Number(prev.balance_after), Number(curr.amount));
    if (!equalsDkk(expected, Number(curr.balance_after))) {
      const drift = subtractDkk(Number(curr.balance_after), expected);
      const warning =
        `balance break before bank transaction ${curr.id} (${curr.transaction_date} '${curr.text.trim()}'): ` +
        `expected ${expected} from previous balance ${roundDkk(Number(prev.balance_after))} + amount ${roundDkk(Number(curr.amount))}, ` +
        `statement shows ${roundDkk(Number(curr.balance_after))} (drift ${drift})`;
      balanceWarnings.push(warning);
      recordException(db, {
        type: "BANK_BALANCE_GAP",
        severity: "medium",
        relatedBankTransactionId: curr.id,
        message: warning,
        requiredAction: "Check the statement for missing, duplicated or misordered rows around this transaction; re-import the complete export if needed.",
        sourceEvidence: {
          ruleId: BALANCE_CONTINUITY_RULE_ID,
          bankTransactionId: curr.id,
          transactionDate: curr.transaction_date,
          previousBalance: roundDkk(Number(prev.balance_after)),
          amount: roundDkk(Number(curr.amount)),
          expectedBalance: expected,
          statementBalance: roundDkk(Number(curr.balance_after)),
          drift,
        },
      });
    }
  }

  return {
    balanceWarnings,
    firstBalance: roundDkk(Number(withBalance[0].balance_after)),
    lastBalance: roundDkk(Number(withBalance[withBalance.length - 1].balance_after)),
  };
}
// ===== END BANK CLUSTER (#189) =====

// ===== BANK CLUSTER (#186-189) =====
export type ImportBankCsvOptions = {
  /** Numeric id or slug of the target bank account (#187). */
  account?: string | number;
  /** Named CSV import profile, e.g. "danske-bank" (#186). */
  profile?: string;
};
// ===== END BANK CLUSTER (#186-189) =====

export function importBankCsv(
  db: Database,
  companyRoot: string,
  csvPath: string,
  options: ImportBankCsvOptions = {},
): BankImportResult {
  if (!existsSync(csvPath)) return { ok: false, errors: [`file does not exist: ${csvPath}`] };

  // ===== BANK CLUSTER (#187) =====
  // Resolve the target account up front: new imports couple their rows to a
  // bank account. A given-but-unknown account aborts before any parsing.
  let bankAccount: BankAccount | null = null;
  if (options.account !== undefined && String(options.account).trim() !== "") {
    bankAccount = resolveBankAccount(db, options.account);
    if (!bankAccount) return { ok: false, errors: [`bank account '${options.account}' does not exist`] };
  }
  const bankAccountId = bankAccount?.id ?? null;
  // ===== END BANK CLUSTER (#187) =====

  // ===== BANK CLUSTER (#186) =====
  // A named profile pins delimiter / encoding / date order and an explicit
  // column->field map. A given-but-unknown profile aborts before parsing.
  let profile: BankImportProfile | null = null;
  if (options.profile !== undefined && String(options.profile).trim() !== "") {
    profile = getBankProfile(String(options.profile));
    if (!profile) {
      return { ok: false, errors: [`unknown bank import profile '${options.profile}' (known: ${listBankProfileNames().join(", ")})`] };
    }
  }
  // ===== END BANK CLUSTER (#186) =====

  const fileBytes = readFileSync(csvPath);
  const sourceFileHash = sha256Bytes(fileBytes);
  const importBatchId = `BANK-${new Date().toISOString().replace(/[-:TZ.]/g, "").slice(0, 14)}-${sourceFileHash.slice(0, 8)}`;

  // ===== BANK CLUSTER (#186) =====
  // With a profile the file is parsed under the pinned format; without one the
  // generic alias-guessing parser is used unchanged.
  let rows: BankImportRow[];
  if (profile) {
    const parsed = parseCsvWithProfile(fileBytes, profile);
    if (parsed.errors.length > 0) return { ok: false, errors: parsed.errors };
    rows = parsed.rows;
  } else {
    const parsedCsv = parseCsv(fileBytes.toString("utf8"));
    if (parsedCsv.errors.length > 0) return { ok: false, errors: parsedCsv.errors };
    rows = parsedCsv.rows.map(toRow);
  }
  // ===== END BANK CLUSTER (#186) =====
  const validation = validateBankImportRows(rows);
  if (!validation.ok) return { ok: false, errors: validation.errors };

  const p = companyPaths(companyRoot);
  mkdirSync(p.bankProcessed, { recursive: true });
  copyFileSync(csvPath, join(p.bankProcessed, `${importBatchId}-${basename(csvPath)}`));

  let imported = 0;
  let skippedDuplicates = 0;
  const skippedDuplicateRows: string[] = [];
  const importedRowIds: number[] = [];
  // Per-content-fingerprint counter: distinguishes repeated identical rows
  // within this file so each gets a unique transaction_hash.
  const occurrenceByContent = new Map<string, number>();
  const insert = db.prepare(
    `INSERT INTO bank_transactions (
      transaction_date, booking_date, text, amount, currency, reference, amount_dkk, fx_rate_to_dkk,
      source_file_hash, import_batch_id, transaction_hash, status, retain_until,
      bank_account_id, counterparty_name, counterparty_account, message, archive_reference, customer_reference, balance_after, raw_json
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'imported', ?, ?, ?, ?, ?, ?, ?, ?, ?)`
  );

  db.transaction(() => {
    for (const row of rows) {
      const contentHash = transactionFingerprint(row, 0, bankAccountId);
      const occurrence = occurrenceByContent.get(contentHash) ?? 0;
      occurrenceByContent.set(contentHash, occurrence + 1);
      const hash = transactionFingerprint(row, occurrence, bankAccountId);
      const existing = db.query("SELECT id FROM bank_transactions WHERE transaction_hash = ?").get(hash) as { id: number } | null;
      if (existing) {
        skippedDuplicates += 1;
        skippedDuplicateRows.push(`${row.transactionDate} ${row.text.trim()} ${normalizeAmount(row.amount)} ${normalizeCurrency(row.currency)}`);
        continue;
      }
      const result = insert.run(
        row.transactionDate,
        row.bookingDate ?? null,
        row.text.trim(),
        normalizeAmount(row.amount),
        normalizeCurrency(row.currency),
        row.reference ?? null,
        normalizeAmountOrNull(row.amountDkk),
        row.fxRateToDkk == null ? null : roundRate6(row.fxRateToDkk),
        sourceFileHash,
        importBatchId,
        hash,
        retainUntilForDate(db, row.bookingDate ?? row.transactionDate),
        bankAccountId,
        row.counterpartyName?.trim() || null,
        row.counterpartyAccount?.trim() || null,
        row.message?.trim() || null,
        row.archiveReference?.trim() || null,
        row.customerReference?.trim() || null,
        row.balanceAfter == null ? null : normalizeAmount(row.balanceAfter),
        row.raw ? JSON.stringify(row.raw) : null,
      );
      importedRowIds.push(Number(result.lastInsertRowid));
      imported += 1;
    }
    insertAuditLog(db, {
      eventType: "bank_import",
      entityType: "bank_transaction_batch",
      entityId: importBatchId,
      message: `Imported ${imported} bank transactions from ${basename(csvPath)}; skipped ${skippedDuplicates} duplicates`,
    });
  })();

  // ===== BANK CLUSTER (#189) =====
  // When a profile supplied a running balance, verify continuity across the
  // freshly-imported batch. A break is surfaced (warning + exception) but never
  // fails the import — a partial export is a legitimate input.
  const balanceCheck = verifyBatchBalanceContinuity(db, importedRowIds);
  // ===== END BANK CLUSTER (#189) =====

  return {
    ok: true,
    importBatchId,
    imported,
    skippedDuplicates,
    skippedDuplicateRows,
    sourceFileHash,
    bankAccountId: bankAccountId ?? undefined,
    bankAccountSlug: bankAccount?.slug,
    profile: profile?.name,
    ...balanceCheck,
    errors: [],
  };
}
