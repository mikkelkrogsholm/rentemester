// Import framework — the Dinero export parser. Issues #193 / #194 (epic #173).
//
// A Dinero data export is a directory tree. This parser reads the files that
// carry the company's chart of accounts, master data and opening balance:
//
//  - `Firmaoplysninger.csv` — one row of company master data.
//  - `<year>/Kontoplan.csv`  — the chart of accounts, one row per account.
//  - `<year>/Posteringer.csv` — the year's postings; its leading rows are the
//    fiscal-year opening balance (the primobalance, #194).
//
// All are semicolon-delimited UTF-8 CSV (a real export may carry a BOM, which
// `resolveSource` strips). The parser produces a normalised `ImportSource`:
// the chart classified onto Rentemester account types, every account's Dinero
// `Momstype` mapped onto a Rentemester VAT code, the company master data, and —
// when a `Posteringer.csv` is present — the cut-over year's opening balance.
//
// The opening balance is the set of `Posteringer.csv` rows with `Bilag = 0`
// and `Tekst = Primobeholdning`, all dated the fiscal-year's first day. `Beløb`
// is a signed amount (comma decimal): positive = debit, negative = credit. The
// balance-sheet primobeholdning rows sum to zero. Historical postings AFTER the
// cut-over date are still out of scope (#195).
//
// The parser is PURE and DETERMINISTIC: the same export always yields the same
// `ImportSource`, including the order of `chartOfAccounts`, `openingBalances`
// and `unmappedVatCodes`.

import { requireFile } from "./source";
import { isValidIsoDate } from "../dates";
import { parseDineroPostings } from "./dinero-postings";
import { validateImportedReceivableSchedule, type ImportedReceivableSchedule } from "../imported-receivables";
import type {
  ImportAccount,
  ImportAccountType,
  ImportCompanyMasterData,
  ImportHistoricalEntry,
  ImportNormalBalance,
  ImportOpenItemControlBalance,
  ImportOpeningBalanceLine,
  MultiArtifactSource,
  ParseResult,
  SourceParser,
} from "./types";
import { DINERO_VAT_CONTROL_ACCOUNTS } from "../vat-account-semantics";
import type { AccountRole } from "../account-roles";

const SYSTEM = "dinero";
const LABEL = "Dinero (data export — chart of accounts, master data & opening balance)";

const FIRMAOPLYSNINGER = "Firmaoplysninger.csv";
/** Optional companion export authored by the versioned Rentemester adapter. */
const RECEIVABLE_SCHEDULE = "Rentemester-modtagerposter-v1.json";

// The Dinero marker for an opening-balance row: voucher number 0, voucher text
// `Primobeholdning`. Such rows carry the fiscal year's opening balance.
const PRIMOBEHOLDNING_TEXT = "primobeholdning";

// --- Dinero Momstype -> Rentemester VAT code -------------------------------
//
// Dinero `Momstype` cells occur both as coded labels (`U25 - Dansk salgsmoms`)
// and as the bare Danish display label (`Dansk salgsmoms`) in Posteringer.csv.
// Both forms are source-defined identities and map to the same canonical code.
//
// Rentemester's VAT codes (rules/dk/vat.yaml) are deliberately few:
//   DK_SALE_25, DK_PURCHASE_25, EU_SERVICE_REVERSE_CHARGE,
//   NON_EU_SERVICE_REVERSE_CHARGE, REPRESENTATION_SPECIAL.
//
// A Dinero code with NO Rentemester equivalent (EU/world goods, reverse-charge
// purchase, the unindberettede EU sales code, ...) is intentionally left
// UNMAPPED: the account gets no `default_vat_code` and the code is surfaced in
// `unmappedVatCodes` for human review. Inventing a VAT code Rentemester does
// not have is explicitly out of scope (#193).
const VAT_CODE_MAP: Record<string, string> = {
  // Danish standard-rated sales / purchases.
  U25: "DK_SALE_25",
  "Dansk salgsmoms": "DK_SALE_25",
  I25: "DK_PURCHASE_25",
  "Dansk købsmoms": "DK_PURCHASE_25",
  // EU service purchases settle as a reverse charge in Rentemester.
  IEUY: "EU_SERVICE_REVERSE_CHARGE",
  "Ydelseskøb EU (rubrik A - ydelser)": "EU_SERVICE_REVERSE_CHARGE",
  // Services bought outside the EU use the distinct non-EU reverse-charge
  // code and therefore never feed the EU rubrik-A purchase base.
  IVY: "NON_EU_SERVICE_REVERSE_CHARGE",
  "Ydelseskøb fra verden": "NON_EU_SERVICE_REVERSE_CHARGE",
  // Representation has a special limited-deduction code.
  REP: "REPRESENTATION_SPECIAL",
  "Repræsentation (kvartmoms)": "REPRESENTATION_SPECIAL",
};

// Dinero codes Rentemester has no equivalent for. Listed explicitly so the
// parser can tell a deliberately-unmapped code apart from an unrecognised one
// (both are surfaced, but the documentation is clearer this way):
//   IEUV  - Varekøb EU (rubrik A - varer)
//   IVV   - Varekøb fra verden
//   OBPK  - Dansk køb med omvendt betalingspligt
//   UEUV  - Varesalg EU - Indberettes (rubrik B - varer)
//   UEUV2 - Varesalg EU - Indberettes ikke (rubrik B - varer)
//   UEUY  - Ydelsessalg EU (rubrik B - ydelser)
//   UVV   - Salg af varer til verden (rubrik C)
//   UVY   - Salg af ydelser til verden (rubrik C)

/** Splits a Dinero semicolon-delimited CSV record. The format has no quoting. */
function splitRecord(line: string): string[] {
  return line.split(";").map((cell) => cell.trim());
}

/** Extracts the leading code from a Dinero `Momstype` cell, e.g. `U25 - ...`. */
function vatCodeKey(momstype: string): string {
  const trimmed = momstype.trim();
  if (trimmed.length === 0) return "";
  const dash = trimmed.indexOf("-");
  return (dash > 0 ? trimmed.slice(0, dash) : trimmed).trim();
}

function canonicalVatCode(momstype: string): string | undefined {
  const trimmed = momstype.trim();
  return VAT_CODE_MAP[trimmed] ?? VAT_CODE_MAP[vatCodeKey(trimmed)];
}

/**
 * Classifies a Dinero account `Type` onto a Rentemester account type. Dinero
 * has no separate `Egenkapital` type — equity accounts sit under `Passiv` and
 * are identified by their account-number range (60000-60040, the registered
 * capital / retained-earnings / dividend block).
 */
function classifyAccount(
  dineroType: string,
  accountNo: string,
): { type: ImportAccountType; normalBalance: ImportNormalBalance } | null {
  const vatControl = DINERO_VAT_CONTROL_ACCOUNTS[accountNo as keyof typeof DINERO_VAT_CONTROL_ACCOUNTS];
  if (vatControl) return { type: "vat", normalBalance: vatControl.normalBalance };
  const t = dineroType.trim().toLowerCase();
  const no = Number(accountNo);
  if (t === "aktiv") return { type: "asset", normalBalance: "debit" };
  if (t === "indtægt" || t === "indtaegt") return { type: "income", normalBalance: "credit" };
  if (t === "udgift") return { type: "expense", normalBalance: "debit" };
  if (t === "passiv") {
    // Equity is a Passiv sub-range in a Dinero chart.
    if (Number.isInteger(no) && no >= 60000 && no <= 60040) {
      return { type: "equity", normalBalance: "credit" };
    }
    return { type: "liability", normalBalance: "credit" };
  }
  return null;
}

/** Parses the single data row of `Firmaoplysninger.csv` into master data. */
function parseFirmaoplysninger(text: string, errors: string[]): ImportCompanyMasterData | undefined {
  const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0);
  if (lines.length < 2) {
    errors.push(`${FIRMAOPLYSNINGER} has no company data row`);
    return undefined;
  }
  const header = splitRecord(lines[0]!);
  const row = splitRecord(lines[1]!);
  const col = (name: string): string | undefined => {
    const idx = header.findIndex((h) => h.toLowerCase() === name.toLowerCase());
    if (idx < 0) return undefined;
    const value = (row[idx] ?? "").trim();
    return value.length > 0 ? value : undefined;
  };
  const md: ImportCompanyMasterData = {};
  const name = col("Firmanavn");
  if (name) md.name = name;
  const cvr = col("CvrNr");
  if (cvr) md.cvr = cvr;
  const address = col("Adresse");
  if (address) md.address = address;
  const postalCode = col("Postnr");
  if (postalCode) md.postalCode = postalCode;
  const city = col("By");
  if (city) md.city = city;
  const country = col("Land");
  if (country) md.country = country;
  const email = col("Email");
  if (email) md.email = email;
  const phone = col("Telefonnummer");
  if (phone) md.phone = phone;
  const website = col("Hjemmeside");
  if (website) md.website = website;
  return md;
}

/**
 * Resolves the single `<year>/Kontoplan.csv` artifact. A Dinero export holds
 * one per fiscal year; the chart of accounts is stable across years, so the
 * LAST year (highest folder name) is used deterministically.
 */
function findKontoplan(input: MultiArtifactSource): { name: string; text: string } | null {
  const matches = Object.keys(input.files)
    .filter((n) => /(^|\/)Kontoplan\.csv$/i.test(n))
    .sort();
  if (matches.length === 0) return null;
  const name = matches[matches.length - 1]!;
  return { name, text: input.files[name]!.text };
}

/** Parses `Kontoplan.csv` into classified accounts plus the unmapped VAT codes. */
function parseKontoplan(
  text: string,
  sourceName: string,
  errors: string[],
): { accounts: ImportAccount[]; unmappedVatCodes: string[] } {
  const accounts: ImportAccount[] = [];
  const unmapped = new Set<string>();
  const lines = text.split(/\r?\n/);
  let sawHeader = false;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line.length === 0) continue;
    const cells = splitRecord(line);
    // The column-header row: `Nummer;Navn;Type;Momstype;Total`.
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "nummer") {
        sawHeader = true;
        continue;
      }
      errors.push(`${sourceName}: missing the 'Nummer;Navn;Type;Momstype;Total' header row`);
      return { accounts, unmappedVatCodes: [] };
    }
    const accountNo = cells[0] ?? "";
    const name = cells[1] ?? "";
    const dineroType = cells[2] ?? "";
    const momstype = cells[3] ?? "";
    if (!accountNo) {
      errors.push(`${sourceName} line ${i + 1}: account row is missing a Nummer`);
      continue;
    }
    const classified = classifyAccount(dineroType, accountNo);
    if (!classified) {
      errors.push(
        `${sourceName} line ${i + 1}: account '${accountNo}' has an unrecognised Type '${dineroType}'`,
      );
      continue;
    }
    const account: ImportAccount = {
      accountNo,
      name,
      type: dineroType.trim(),
      normalizedType: classified.type,
      normalBalance: classified.normalBalance,
      defaultVatCode: null,
    };
    const key = vatCodeKey(momstype);
    if (key.length > 0) {
      const mapped = VAT_CODE_MAP[key];
      if (mapped) {
        account.defaultVatCode = mapped;
      } else {
        // Unmapped: keep the account VAT-code-free and surface the label.
        unmapped.add(momstype.trim());
      }
    }
    accounts.push(account);
  }
  if (!sawHeader) {
    errors.push(`${sourceName}: missing the 'Nummer;Navn;Type;Momstype;Total' header row`);
  }
  return { accounts, unmappedVatCodes: [...unmapped].sort() };
}

/**
 * Resolves the cut-over year's `<year>/Posteringer.csv` artifact. A Dinero
 * export holds one per fiscal year; the opening balance is imported for the
 * LATEST year present (the same deterministic rule `findKontoplan` uses for
 * the chart), so a Rentemester migration continues from the most recent year.
 */
function findPosteringer(input: MultiArtifactSource): { name: string; text: string } | null {
  const matches = Object.keys(input.files)
    .filter((n) => /(^|\/)Posteringer\.csv$/i.test(n))
    .sort();
  if (matches.length === 0) return null;
  const name = matches[matches.length - 1]!;
  return { name, text: input.files[name]!.text };
}

function parseOpenItemControlBalances(
  input: MultiArtifactSource,
  posteringerName: string | undefined,
  proposals: Array<{ role: AccountRole; accountNo: string; source: string }>,
  errors: string[],
): ImportOpenItemControlBalance[] {
  if (!posteringerName) return [];
  const sourceReference = posteringerName.replace(/Posteringer\.csv$/i, "SaldoBalance.csv");
  const artifact = input.files[sourceReference];
  if (!artifact) return [];

  const balances = new Map<string, number>();
  const lines = artifact.text.split(/\r?\n/);
  let sawHeader = false;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (!line) continue;
    const cells = splitRecord(line);
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "konto") {
        sawHeader = true;
        continue;
      }
      errors.push(`${sourceReference}: missing the 'Konto;Kontonavn;Beløb' header row`);
      return [];
    }
    const accountNo = cells[0] ?? "";
    const amount = parseBelob(cells[2] ?? "");
    if (!accountNo || amount == null) {
      errors.push(`${sourceReference} line ${i + 1}: invalid account or Beløb`);
      continue;
    }
    if (balances.has(accountNo)) {
      errors.push(`${sourceReference} repeats account '${accountNo}'`);
      continue;
    }
    balances.set(accountNo, amount);
  }
  if (!sawHeader) errors.push(`${sourceReference}: missing the 'Konto;Kontonavn;Beløb' header row`);

  const out: ImportOpenItemControlBalance[] = [];
  for (const proposal of proposals) {
    if (proposal.role !== "debtors" && proposal.role !== "creditors") continue;
    const signedAmount = balances.get(proposal.accountNo);
    if (signedAmount == null || signedAmount === 0) continue;
    if (proposal.role === "debtors" && signedAmount < 0) {
      errors.push(`${sourceReference}: debtor control account '${proposal.accountNo}' has an unexpected credit balance ${signedAmount}`);
      continue;
    }
    if (proposal.role === "creditors" && signedAmount > 0) {
      errors.push(`${sourceReference}: creditor control account '${proposal.accountNo}' has an unexpected debit balance ${signedAmount}`);
      continue;
    }
    out.push({
      accountNo: proposal.accountNo,
      kind: proposal.role === "debtors" ? "receivable" : "payable",
      amount: Math.abs(signedAmount),
      sourceReference,
    });
  }
  return out.sort((a, b) => a.accountNo.localeCompare(b.accountNo, "en"));
}

/**
 * Parses a Dinero `Beløb` cell — a signed decimal with a comma decimal
 * separator and up to six decimal places, e.g. `30116,010000` or
 * `-40000,000000` — into a kroner Number. Returns `null` on a malformed cell.
 *
 * The result is a kroner amount: `ImportOpeningBalanceLine` debit/credit feed
 * straight into `postOpeningBalance` -> `postJournalEntry`, which stores the
 * kroner value (and converts to øre internally only for the balance check).
 */
function parseBelob(cell: string): number | null {
  const trimmed = cell.trim().replace(",", ".");
  if (trimmed.length === 0 || !/^-?\d+(\.\d+)?$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/**
 * Parses the cut-over year's `Posteringer.csv` opening balance. It extracts
 * the `Primobeholdning` rows (voucher `Bilag = 0`, `Tekst = Primobeholdning`):
 * each becomes an `ImportOpeningBalanceLine` with `Beløb > 0` -> `debitAmount`
 * and `Beløb < 0` -> `creditAmount` (absolute value), in kroner.
 *
 * `cutOverDate` is the (single) date the Primobeholdning rows carry — the
 * fiscal-year's first day. A file with NO Primobeholdning rows is not an error:
 * it yields an empty opening balance and an empty cut-over date, so the import
 * falls back to the chart-only behaviour (#193).
 */
function parsePosteringer(
  text: string,
  sourceName: string,
  errors: string[],
): { openingBalances: ImportOpeningBalanceLine[]; cutOverDate: string } {
  const openingBalances: ImportOpeningBalanceLine[] = [];
  const lines = text.split(/\r?\n/);
  let sawHeader = false;
  const cutOverDates = new Set<string>();
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line.length === 0) continue;
    const cells = splitRecord(line);
    // The column-header row:
    // `Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo`.
    if (!sawHeader) {
      if ((cells[0] ?? "").toLowerCase() === "konto") {
        sawHeader = true;
        continue;
      }
      errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Dato;...' header row`);
      return { openingBalances: [], cutOverDate: "" };
    }
    const accountNo = cells[0] ?? "";
    const date = (cells[2] ?? "").trim();
    const bilag = (cells[3] ?? "").trim();
    const tekst = (cells[5] ?? "").trim();
    const belob = cells[7] ?? "";
    // Only the opening-balance (Primobeholdning) rows are in scope for #194.
    if (bilag !== "0" || tekst.toLowerCase() !== PRIMOBEHOLDNING_TEXT) continue;
    if (!accountNo) {
      errors.push(`${sourceName} line ${i + 1}: Primobeholdning row is missing a Konto`);
      continue;
    }
    if (!isValidIsoDate(date)) {
      errors.push(
        `${sourceName} line ${i + 1}: Primobeholdning row for account '${accountNo}' has an invalid Dato '${date}'`,
      );
      continue;
    }
    const amount = parseBelob(belob);
    if (amount === null) {
      errors.push(
        `${sourceName} line ${i + 1}: Primobeholdning row for account '${accountNo}' has an invalid Beløb '${belob}'`,
      );
      continue;
    }
    cutOverDates.add(date);
    // Sign convention: positive Beløb is a debit, negative is a credit. A zero
    // Beløb carries no balance and is skipped.
    if (amount > 0) {
      openingBalances.push({ accountNo, debitAmount: amount });
    } else if (amount < 0) {
      openingBalances.push({ accountNo, creditAmount: -amount });
    }
  }
  if (!sawHeader) {
    errors.push(`${sourceName}: missing the 'Konto;Kontonavn;Dato;...' header row`);
    return { openingBalances: [], cutOverDate: "" };
  }
  if (cutOverDates.size > 1) {
    errors.push(
      `${sourceName}: Primobeholdning rows carry more than one date (${[...cutOverDates].sort().join(", ")}) — expected the fiscal-year start`,
    );
    return { openingBalances: [], cutOverDate: "" };
  }
  // No Primobeholdning rows: not an error — fall back to a chart-only import.
  const cutOverDate = openingBalances.length > 0 ? [...cutOverDates][0]! : "";
  return { openingBalances, cutOverDate };
}

function parseDineroSource(input: MultiArtifactSource): ParseResult {
  const errors: string[] = [];

  const firma = requireFile(input, FIRMAOPLYSNINGER, errors);
  const kontoplan = findKontoplan(input);
  if (!kontoplan) {
    errors.push("required export file '<year>/Kontoplan.csv' is missing");
  }
  if (!firma || !kontoplan) {
    return { ok: false, errors };
  }

  const companyMasterData = parseFirmaoplysninger(firma.text, errors);
  const { accounts, unmappedVatCodes } = parseKontoplan(kontoplan.text, kontoplan.name, errors);
  const accountRoleProposals = deriveAccountRoleProposals(accounts);

  if (accounts.length === 0) {
    errors.push(`${kontoplan.name}: no accounts parsed from the chart of accounts`);
  }

  // Opening balance (#194): the cut-over year's `Posteringer.csv` Primobeholdning
  // rows. An export with no `Posteringer.csv` — or one with no Primobeholdning
  // rows — keeps the chart-only behaviour (an empty cut-over date / opening
  // balance), exactly as before.
  const posteringer = findPosteringer(input);
  const { openingBalances, cutOverDate } = posteringer
    ? parsePosteringer(posteringer.text, posteringer.name, errors)
    : { openingBalances: [] as ImportOpeningBalanceLine[], cutOverDate: "" };
  const openItemControlBalances = parseOpenItemControlBalances(
    input,
    posteringer?.name,
    accountRoleProposals,
    errors,
  );

  // Year-to-date activity (#195): the cut-over year's `Posteringer.csv` rows
  // that are NOT Primobeholdning, grouped by `Bilag` into balanced vouchers.
  // The framework replays them as journal entries after the primobalance.
  const historicalEntries: ImportHistoricalEntry[] = posteringer
    ? parseDineroPostings(posteringer.text, posteringer.name, errors).map((voucher) => ({
        transactionDate: voucher.transactionDate,
        text: voucher.text,
        voucherRef: voucher.bilag,
        entryType: voucher.voucherType,
        lines: voucher.lines.map((line) => {
          const sourceVatCode = line.vatCode?.trim() ?? "";
          const normalizedVatCode =
            sourceVatCode.length > 0 ? canonicalVatCode(sourceVatCode) : undefined;
          if (sourceVatCode.length > 0 && normalizedVatCode === undefined) {
            errors.push(
              `${posteringer.name}: voucher ${voucher.bilag} account ${line.accountNo} has unsupported Dinero Momstype '${sourceVatCode}'`,
            );
          }
          return {
            accountNo: line.accountNo,
            ...(line.debitAmount !== undefined ? { debitAmount: line.debitAmount } : {}),
            ...(line.creditAmount !== undefined ? { creditAmount: line.creditAmount } : {}),
            text: line.text,
            // Persist Rentemester's canonical code, never Dinero's display
            // text (`I25 - ...`). Only a genuinely blank source field may use
            // the reviewed account default in the historical-import adapter.
            ...(normalizedVatCode ? { vatCode: normalizedVatCode } : {}),
          };
        }),
      }))
    : [];

  let importedReceivableSchedule: ImportedReceivableSchedule | undefined;
  const scheduleFile = input.files[RECEIVABLE_SCHEDULE];
  if (scheduleFile) {
    try {
      const checked = validateImportedReceivableSchedule(JSON.parse(scheduleFile.text));
      if (!checked.ok) errors.push(...checked.errors.map(error => `${RECEIVABLE_SCHEDULE}: ${error}`));
      else importedReceivableSchedule = checked.schedule;
    } catch { errors.push(`${RECEIVABLE_SCHEDULE}: invalid JSON`); }
  }

  if (errors.length > 0) {
    return { ok: false, errors };
  }

  return {
    ok: true,
    errors: [],
    source: {
      sourceSystem: SYSTEM,
      // A non-empty cut-over date marks this as a postable primobalance import
      // (#194); an empty one keeps the IR honest as a chart/master-data-only
      // import (#193). The framework dispatches on exactly this.
      cutOverDate,
      chartOfAccounts: accounts,
      ...(accountRoleProposals.length > 0 ? { accountRoleProposals } : {}),
      openingBalances,
      ...(historicalEntries.length > 0 ? { historicalEntries } : {}),
      ...(openItemControlBalances.length > 0 ? { openItemControlBalances } : {}),
      ...(importedReceivableSchedule ? { importedReceivableSchedule } : {}),
      ...(companyMasterData ? { companyMasterData } : {}),
      ...(unmappedVatCodes.length > 0 ? { unmappedVatCodes } : {}),
    },
  };
}

/** Conservative name/VAT-code evidence only. Number ranges are intentionally
 * excluded: imported charts must never inherit Rentemester's native numbers. */
export function deriveAccountRoleProposals(accounts: ImportAccount[]): Array<{ role: AccountRole; accountNo: string; source: string }> {
  const proposals: Array<{ role: AccountRole; accountNo: string; source: string }> = [];
  const add = (role: AccountRole, account: ImportAccount, evidence: string) => proposals.push({ role, accountNo: account.accountNo, source: `dinero:chart:${evidence}` });
  for (const account of accounts) {
    const name = account.name.toLowerCase();
    const vatControl = DINERO_VAT_CONTROL_ACCOUNTS[account.accountNo as keyof typeof DINERO_VAT_CONTROL_ACCOUNTS];
    if (vatControl && account.normalizedType === "vat" && account.normalBalance === vatControl.normalBalance) {
      add(vatControl.role, account, `control-account-${account.accountNo}`);
      continue;
    }
    if (account.normalizedType === "asset" && /\b(bank|bankkonto)\b/.test(name)) add("bank", account, "name-bank");
    if (account.normalizedType === "asset" && /debitor|tilgodehavende.*(kunde|salg)|kundetilgodehavende/.test(name)) add("debtors", account, "name-debtors");
    if (account.normalizedType === "liability" && /kreditor|leverandørgæld/.test(name)) add("creditors", account, "name-creditors");
    // U25/I25 live on revenue/expense base accounts and are never evidence for
    // the output/input VAT control accounts. Only an explicitly classified VAT
    // account may propose those roles (Dinero's exact mapping is centralised by
    // the #545 source adapter).
    if (account.normalizedType === "vat" && account.normalBalance === "credit" && /salgsmoms|udgående moms/.test(name)) add("output_vat", account, "control-name-output-vat");
    if (account.normalizedType === "vat" && account.normalBalance === "debit" && /købsmoms|indgående moms/.test(name)) add("input_vat", account, "control-name-input-vat");
    if (account.normalizedType === "vat" && account.normalBalance === "credit" && /omvendt.*moms|erhvervelsesmoms/.test(name)) add("reverse_charge_vat", account, "control-name-reverse-charge");
    if (account.normalizedType === "liability" && /skyldig moms|momsafregning/.test(name)) add("vat_settlement", account, "name-vat-settlement");
    if (account.normalizedType === "expense" && /drift|administration|kontor/.test(name)) add("operational_default", account, "name-operational");
  }
  return proposals;
}

/**
 * The Dinero export parser — a multi-file `SourceParser` (the `parseSource`
 * shape, #192). It declares the files it needs and converts a resolved Dinero
 * export into a normalised `ImportSource`.
 */
export const dineroParser: SourceParser = {
  system: SYSTEM,
  label: LABEL,
  requiredFiles: [FIRMAOPLYSNINGER],
  parseSource: parseDineroSource,
};

/** Exposed for documentation/tests — the Dinero `Momstype` -> VAT-code table. */
export { VAT_CODE_MAP };
