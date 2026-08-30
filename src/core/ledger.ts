import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { seedNativeAccountRoles } from "./account-roles";
import { insertAuditLog, resolveActor } from "./actor";
import { verifyAuditLogIntegrity } from "./audit-log";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { DocumentEvidenceError, isIssuedDocumentEvidence, snapshotRegisteredDocumentEvidence } from "./document-storage";
import { resolveOpenExceptionsForBankTransaction } from "./exceptions";
import { asJournalEntryId, type JournalEntryId } from "./ids";
import {
  HISTORICAL_IMPORT_PROGRAM,
  isPersistedHistoricalImportProgram,
} from "./import-provenance";
import { validateInvoiceJournalEvidence } from "./invoice-journal-evidence";
import { validateLegacyInvoiceRepairEvidence } from "./invoice-legacy-repair-evidence";
import { getInvoiceStatus } from "./invoice-payments";
import { compareDkk, fromOre, roundDkk, roundRate6, toOre } from "./money";
import { companyPaths } from "./paths";
import { validateJournalTransactionDate } from "./periods";
import { retainUntilForDate } from "./retention";
import { currentRuleBundleVersion } from "./rules-metadata";
import { companySequenceScope, fiscalYearLabelFromDate, nextSequenceValue } from "./sequences";
import {
  loadVatAccountSemantics,
  VAT_LINE_CODES,
} from "./vat-account-semantics";

export type JournalLineInput = {
  accountNo: string;
  debitAmount?: number;
  creditAmount?: number;
  vatCode?: string;
  text?: string;
};

export type JournalEntryInput = {
  transactionDate: string;
  text: string;
  documentId?: number;
  sourceBankTransactionId?: number;
  currency?: string;
  amountForeign?: number;
  amountDkk?: number;
  fxRateToDkk?: number;
  createdBy?: string;
  createdByProgram?: string;
  lines: JournalLineInput[];
};

// `created_by_program` values flagging a journal entry as a replayed import
// posting (#195). Such entries are exempt from the income/expense
// document-evidence requirement until #196 attaches the original bilag.
const IMPORTED_HISTORICAL_PROGRAMS = new Set([HISTORICAL_IMPORT_PROGRAM]);

type JournalPostingPolicy = {
  historicalImport: boolean;
  /** A reversal may faithfully copy an older uncoded VAT correction. */
  allowUncodedVatControl: boolean;
};

const MANUAL_POSTING_POLICY: JournalPostingPolicy = {
  historicalImport: false,
  allowUncodedVatControl: false,
};

const HISTORICAL_IMPORT_POSTING_POLICY: JournalPostingPolicy = {
  historicalImport: true,
  allowUncodedVatControl: true,
};

export type JournalPostResult = {
  ok: boolean;
  entryId?: JournalEntryId;
  entryNo?: string;
  entryHash?: string;
  appliedRules: string[];
  errors: string[];
};

export type JournalReverseResult = JournalPostResult & {
  originalEntryId?: JournalEntryId;
};

// The signed effect a dry-run posting would have on one account. `balanceBefore`
// and `balanceAfter` are the running debit-minus-credit net (the saldobalance
// convention) — credit-normal accounts therefore read negative.
export type JournalDryRunAccountEffect = {
  accountNo: string;
  accountName: string;
  balanceBefore: number;
  balanceAfter: number;
  delta: number;
};

// A non-binding preview of postJournalEntry. The entry/hash/effects describe
// what *would* be written if the same payload were posted right now — they are
// not a reservation: the entry number, previous hash and balances can all move
// if another entry is posted between the dry run and the real post.
export type JournalDryRunResult = {
  ok: boolean;
  appliedRules: string[];
  errors: string[];
  entryId?: JournalEntryId;
  entryNo?: string;
  previousHash?: string;
  entryHash?: string;
  accountEffects?: JournalDryRunAccountEffect[];
};

const LEDGER_RULES = {
  BALANCED: "DK-BOOKKEEPING-BALANCED-001",
  APPEND_ONLY: "DK-BOOKKEEPING-APPEND-ONLY-001",
  REVERSAL: "DK-BOOKKEEPING-REVERSAL-001",
  DOCUMENT: "DK-BOOKKEEPING-DOCUMENT-001",
  FX: "DK-BOOKKEEPING-FX-001",
  PERIOD_LOCK: "DK-BOOKKEEPING-PERIOD-LOCK-001",
} as const;

const RULE_VERSION = currentRuleBundleVersion();

export function seedAccounts(db: Database) {
  // The seeded chart of accounts covers what an ordinary Danish small business
  // (incl. a one-person ApS) actually needs (#226). Numbering keeps the Danish
  // kontoplan ranges already in use:
  //   1xxx  income + debtors + salgsmoms
  //   2xxx  bank
  //   3xxx  external operating expenses (driftsomkostninger) + staff costs
  //   4xxx  VAT settlement accounts (4000 Købsmoms, 4500 Momsafregning)
  //   5xxx  equity (incl. owner's draw/contribution) + fixed assets
  //   7xxx  short-term liabilities (creditors + payroll-related gæld + tax)
  const rows = [
    // --- Income (1xxx) ---
    ["1000", "Omsætning, ydelser", "income", "credit", null],
    ["1010", "Gebyr- og kompensationsindtægter", "income", "credit", null],
    // Realiseret valutakursgevinst på debitorer/kreditorer i fremmed valuta:
    // når en fordring/gæld indfries til en anden kurs end den, den blev bogført
    // til, er forskellen en finansiel gevinst (kredit) — modposten til 3320.
    ["1020", "Valutakursgevinst (realiseret)", "income", "credit", null],
    ["1100", "Debitorer", "asset", "debit", null],
    ["1200", "Salgsmoms", "vat", "credit", null],
    // Periodeafgrænsningspost: forudbetalte omkostninger (asset). A prepaid
    // expense already paid that belongs to a later period is parked here until
    // it is recognised period by period.
    ["1300", "Forudbetalte omkostninger", "asset", "debit", null],
    // --- Bank (2xxx) ---
    ["2000", "Bank", "asset", "debit", null],
    // --- External operating expenses (3xxx, 3000-3399) ---
    ["3000", "Software og SaaS", "expense", "debit", "DK_PURCHASE_25"],
    ["3010", "AI-værktøjer", "expense", "debit", "EU_SERVICE_REVERSE_CHARGE"],
    ["3020", "Hosting og cloud", "expense", "debit", "EU_SERVICE_REVERSE_CHARGE"],
    ["3050", "Rejse og transport", "expense", "debit", "DK_PURCHASE_25"],
    ["3055", "Kørselsgodtgørelse", "expense", "debit", null],
    ["3070", "Repræsentation", "expense", "debit", "REPRESENTATION_SPECIAL"],
    ["3080", "Tab på debitorer", "expense", "debit", "DK_BAD_DEBT_25"],
    ["3100", "Husleje", "expense", "debit", "DK_PURCHASE_25"],
    ["3110", "El, vand og varme", "expense", "debit", "DK_PURCHASE_25"],
    ["3120", "Hardware og udstyr", "expense", "debit", "DK_PURCHASE_25"],
    ["3130", "Kontorartikler og småanskaffelser", "expense", "debit", "DK_PURCHASE_25"],
    ["3140", "Telefon og internet", "expense", "debit", "DK_PURCHASE_25"],
    ["3150", "Forsikringer", "expense", "debit", null],
    ["3160", "Revisor og bogføring", "expense", "debit", "DK_PURCHASE_25"],
    ["3170", "Advokat og rådgivning", "expense", "debit", "DK_PURCHASE_25"],
    ["3180", "Markedsføring og annoncering", "expense", "debit", "DK_PURCHASE_25"],
    ["3190", "Kontingenter og abonnementer", "expense", "debit", "DK_PURCHASE_25"],
    ["3200", "Porto og fragt", "expense", "debit", "DK_PURCHASE_25"],
    ["3300", "Gebyrer, bank og betalingskort", "expense", "debit", null],
    ["3310", "Renteudgifter", "expense", "debit", null],
    // Realiseret valutakurstab på debitorer/kreditorer i fremmed valuta — det
    // finansielle modstykke til 1020. Bogføres når en fordring/gæld indfries
    // til en ringere kurs end bogføringskursen.
    ["3320", "Valutakurstab (realiseret)", "expense", "debit", null],
    // --- Staff costs (3xxx, 3500-3599) ---
    ["3500", "Lønninger", "expense", "debit", null],
    ["3510", "Pension", "expense", "debit", null],
    ["3520", "ATP og lovpligtige bidrag", "expense", "debit", null],
    ["3530", "Personaleomkostninger", "expense", "debit", "DK_PURCHASE_25"],
    // --- VAT settlement accounts (4xxx) ---
    ["4000", "Købsmoms", "vat", "debit", null],
    ["4500", "Momsafregning", "liability", "credit", null],
    // --- Equity + fixed assets (5xxx) ---
    ["5000", "Egenkapital", "equity", "credit", null],
    // Owner's draw / contribution (#249): for an enkeltmandsvirksomhed money
    // moved between the owner and the business is an equity movement, not an
    // expense or income. 5010 records hævninger (debit reduces equity), 5020
    // records indskud (credit increases equity).
    ["5010", "Privat hævning", "equity", "debit", null],
    ["5020", "Privat indskud", "equity", "credit", null],
    // Fixed-asset accounts (#124, #125), 5800-5899 range. 5800 capitalises
    // driftsmidler; 5810 is the contra-asset accumulated-depreciation account
    // (credit-normal); 5820 carries the period depreciation expense.
    ["5800", "Driftsmidler og inventar", "asset", "debit", null],
    ["5810", "Akkumulerede afskrivninger", "asset", "credit", null],
    ["5820", "Afskrivninger", "expense", "debit", null],
    // --- Short-term liabilities (7xxx): trade creditors + payroll gæld ---
    ["7000", "Leverandørgæld (kreditorer)", "liability", "credit", null],
    ["7100", "Skyldig A-skat", "liability", "credit", null],
    ["7110", "Skyldigt AM-bidrag", "liability", "credit", null],
    ["7120", "Skyldig ATP", "liability", "credit", null],
    ["7130", "Skyldig løn", "liability", "credit", null],
    // Tax payable / skattekonto (#249): the everyday account for tax owed to
    // SKAT — B-skat / restskat for an enkeltmandsvirksomhed, selskabsskat for
    // an ApS. Credit-normal short-term liability, alongside the payroll gæld.
    ["7200", "Skyldig skat (skattekonto)", "liability", "credit", null],
    // Periodeafgrænsningsposter på passivsiden. 7300 holds accrued expenses
    // (skyldige omkostninger): a cost that belongs to the period but is not yet
    // paid. 7310 holds deferred revenue (forudbetalt indtægt): cash received
    // for a service not yet delivered. Both are credit-normal liabilities that
    // unwind period by period as the cost/revenue is recognised.
    ["7300", "Skyldige omkostninger", "liability", "credit", null],
    ["7310", "Forudbetalt indtægt (udskudt omsætning)", "liability", "credit", null]
  ];
  const insert = db.prepare("INSERT OR IGNORE INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES (?,?,?,?,?)");
  db.transaction(() => rows.forEach((r) => {
    insert.run(...r);
  })).immediate();
  seedNativeAccountRoles(db);
}

// Minimum entry-number suffix width. Numbers below 100 000 are zero-padded to
// five digits (`2026-00001`); once a fiscal scope exceeds 99 999 entries the
// suffix simply grows wider (`2026-100000`). The matcher below must therefore
// be robust to a VARIABLE suffix width (KODE-12), not assume exactly five.
const ENTRY_NO_MIN_DIGITS = 5;

// Width-robust max-suffix read for one fiscal scope. The suffix is whatever
// follows the literal `${scope}-` prefix, so it is extracted by length —
// `substr(entry_no, length(prefix) + 1)` — instead of a fixed `substr(-5)`
// that silently truncates a six-digit number to its last five digits. The
// match anchors the exact prefix and requires the suffix to be all digits
// (`NOT GLOB '*[^0-9]*'`), so a wider number still matches and still parses.
function maxEntryNoSuffix(db: Database, scope: string): number {
  const prefix = `${scope}-`;
  const row = db
    .query(
      `SELECT COALESCE(MAX(CAST(substr(entry_no, ? + 1) AS INTEGER)), 0) AS n
         FROM journal_entries
        WHERE substr(entry_no, 1, ?) = ?
          AND length(entry_no) > ?
          AND substr(entry_no, ? + 1) NOT GLOB '*[^0-9]*'`,
    )
    .get(prefix.length, prefix.length, prefix, prefix.length, prefix.length) as { n: number };
  return Number(row.n ?? 0);
}

export function nextEntryNo(db: Database, transactionDate: string) {
  const scope = fiscalYearLabelFromDate(db, transactionDate);
  const floor = maxEntryNoSuffix(db, scope);
  const nextValue = nextSequenceValue(db, "journal_entry", companySequenceScope(db, scope), floor);
  return `${scope}-${String(nextValue).padStart(ENTRY_NO_MIN_DIGITS, "0")}`;
}

export function previousHash(db: Database) {
  const row = db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as { entry_hash: string } | null;
  return row?.entry_hash ?? "GENESIS";
}

// journal_entries is append-only (delete is blocked by trigger), so the id the
// next inserted row receives is deterministically MAX(id) + 1. The id is bound
// into the entry hash, so it must be predicted before the row is written.
function nextEntryId(db: Database) {
  const row = db.query("SELECT COALESCE(MAX(id), 0) AS n FROM journal_entries").get() as { n: number };
  return Number(row.n) + 1;
}

export function hashEntry(data: unknown, prev: string) {
  return createHash("sha256").update(JSON.stringify(data)).update(prev).digest("hex");
}


function normalizeAmount(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? roundDkk(value) : 0;
}

// The canonical hash binds the row id and an explicit per-line ordinal so each
// entry_hash attests its row identity and the insertion order of its lines.
// The chain therefore attests the id-ordered insertion sequence: two rows
// cannot be swapped in id order without invalidating their hashes.
function canonicalEntryData(entry: any, lines: any[]) {
  return {
    id: entry.id ?? null,
    entry_no: entry.entry_no,
    transaction_date: entry.transaction_date,
    text: entry.text,
    source_bank_transaction_id: entry.source_bank_transaction_id ?? null,
    document_id: entry.document_id ?? null,
    currency: entry.currency ?? 'DKK',
    amount_foreign: entry.amount_foreign ?? null,
    amount_dkk: entry.amount_dkk ?? null,
    fx_rate_to_dkk: entry.fx_rate_to_dkk ?? null,
    rule_version: entry.rule_version,
    created_by: entry.created_by,
    created_by_program: entry.created_by_program,
    status: entry.status,
    reversal_of_entry_id: entry.reversal_of_entry_id ?? null,
    lines: lines.map((line, ordinal) => ({
      ordinal,
      account_no: line.account_no,
      debit_amount: normalizeAmount(line.debit_amount),
      credit_amount: normalizeAmount(line.credit_amount),
      vat_code: line.vat_code ?? null,
      text: line.text ?? null,
    })),
  };
}

function accountMap(db: Database) {
  const rows = db.query("SELECT id, account_no, type, active, default_vat_code FROM accounts").all() as Array<{ id: number; account_no: string; type: string; active: number; default_vat_code: string | null }>;
  return new Map(rows.map((row) => [row.account_no, row]));
}

function existingCreditNoteJournal(db: Database, documentId: number | undefined) {
  if (documentId == null) return null;
  return db.query(
    `SELECT existing.id, existing.entry_no
       FROM documents credit
       JOIN journal_entries existing ON existing.document_id = credit.id
      WHERE credit.id = ?
        AND credit.document_type = 'credit_note'
      ORDER BY existing.id ASC
      LIMIT 1`,
  ).get(documentId) as { id: number; entry_no: string } | null;
}

function validateJournalEntryWithPolicy(
  db: Database,
  payload: JournalEntryInput,
  policy: JournalPostingPolicy,
) {
  const errors: string[] = [];
  const appliedRules: string[] = [LEDGER_RULES.BALANCED, LEDGER_RULES.APPEND_ONLY];
  const lines = payload.lines ?? [];
  const currency = (payload.currency ?? 'DKK').trim().toUpperCase();

  if (!looksLikeIsoDate(payload.transactionDate)) errors.push("transactionDate must be present in YYYY-MM-DD format");
  if (typeof payload.text !== "string" || payload.text.trim().length === 0) errors.push("text is required");
  if (looksLikeIsoDate(payload.transactionDate)) {
    appliedRules.push(LEDGER_RULES.PERIOD_LOCK);
    errors.push(...validateJournalTransactionDate(db, payload.transactionDate));
  }
  if (!Array.isArray(lines) || lines.length < 2) errors.push("at least two journal lines are required");
  if (currency.length !== 3) errors.push("currency must be a 3-letter ISO code when present");

  const accounts = accountMap(db);
  const vatSemantics = loadVatAccountSemantics(db);
  const vatAccounts = vatSemantics.amountSideByAccountNo;
  let debitSum = 0n;
  let creditSum = 0n;
  let requiresDocument = false;
  let touchesVatControl = false;
  let hasExplicitVatBaseCode = false;
  let touchesVatSettlement = false;
  let everyLineIsVatControlOrSettlement = true;

  if (
    !policy.historicalImport &&
    isPersistedHistoricalImportProgram(payload.createdByProgram)
  ) {
    errors.push(
      `createdByProgram '${HISTORICAL_IMPORT_PROGRAM}' is reserved for the verified historical-import adapter`,
    );
  }

  lines.forEach((line, idx) => {
    const debit = normalizeAmount(line.debitAmount);
    const credit = normalizeAmount(line.creditAmount);
    if (!line.accountNo || !accounts.has(line.accountNo)) {
      errors.push(`lines[${idx}].accountNo must reference an existing account`);
      return;
    }
    const account = accounts.get(line.accountNo)!;
    const isVatControl = vatAccounts.has(line.accountNo);
    const isVatSettlement = vatSemantics.settlementAccountNos.has(line.accountNo);
    if (isVatControl) touchesVatControl = true;
    if (isVatSettlement) touchesVatSettlement = true;
    if (!isVatControl && !isVatSettlement) everyLineIsVatControlOrSettlement = false;
    if (line.vatCode != null && typeof line.vatCode !== "string") {
      errors.push(`lines[${idx}].vatCode must be a string when present`);
    }
    if (typeof line.vatCode === "string" && line.vatCode.trim().length > 0) {
      const vatCode = line.vatCode.trim();
      if (!VAT_LINE_CODES.has(vatCode)) {
        errors.push(`lines[${idx}].vatCode '${vatCode}' is not a supported VAT code`);
      } else if (account.type === "income" || account.type === "expense") {
        hasExplicitVatBaseCode = true;
      }
    }
    if (!account.active) errors.push(`lines[${idx}].accountNo refers to an inactive account`);
    if (debit < 0 || credit < 0) {
      errors.push(`lines[${idx}] debit and credit amounts must not be negative`);
    } else if ((debit > 0 && credit > 0) || (debit === 0 && credit === 0)) {
      errors.push(`lines[${idx}] must have either debitAmount or creditAmount`);
    }
    debitSum += toOre(debit);
    creditSum += toOre(credit);
    if (account.type === "expense" || account.type === "income") requiresDocument = true;
  });

  if (debitSum !== creditSum) {
    errors.push(`journal entry must balance: debit ${fromOre(debitSum)} != credit ${fromOre(creditSum)}`);
  }

  // #533: a new manual entry that changes a VAT control account without any
  // explicit base classification can produce the right payable but zero
  // rubrik bases. Reject it at the write boundary. Verified historical imports
  // and exact reversals use separate policies because old source systems may
  // legitimately carry amount-only VAT corrections.
  if (
    !policy.allowUncodedVatControl &&
    touchesVatControl &&
    !hasExplicitVatBaseCode &&
    !(touchesVatSettlement && everyLineIsVatControlOrSettlement)
  ) {
    errors.push(
      "manual journal entries that affect a VAT amount account require an explicit vatCode on the VAT base line (for example DK_SALE_25, DK_PURCHASE_25, EU_SERVICE_REVERSE_CHARGE, or REPRESENTATION_SPECIAL)",
    );
  }

  if (currency !== 'DKK') {
    appliedRules.push(LEDGER_RULES.FX);
    const amountForeign = normalizeAmount(payload.amountForeign);
    const amountDkk = normalizeAmount(payload.amountDkk);
    const fxRateToDkk = typeof payload.fxRateToDkk === 'number' && Number.isFinite(payload.fxRateToDkk) ? roundRate6(payload.fxRateToDkk) : NaN;
    if (!(amountForeign > 0)) errors.push("amountForeign must be positive for non-DKK journal entries");
    if (!(amountDkk > 0)) errors.push("amountDkk must be positive for non-DKK journal entries");
    if (!(fxRateToDkk > 0)) errors.push("fxRateToDkk must be positive for non-DKK journal entries");
    if (amountForeign > 0 && fxRateToDkk > 0) {
      const expectedAmountDkk = roundDkk(amountForeign * fxRateToDkk);
      if (compareDkk(amountDkk, expectedAmountDkk) !== 0) errors.push(`amountDkk must equal amountForeign * fxRateToDkk (${expectedAmountDkk})`);
    }
  }

  if (requiresDocument) {
    appliedRules.push(LEDGER_RULES.DOCUMENT);
    if (policy.historicalImport && !payload.documentId) {
      // A replayed migration posting (#195): the original receipt is not yet
      // ingested into Rentemester. The entry is still recorded as needing
      // document evidence — #196 attaches the bilag — but is not rejected.
    } else if (!payload.documentId) {
      errors.push("documentId is required when posting expense or income lines");
    } else {
      const doc = db.query("SELECT id FROM documents WHERE id = ?").get(payload.documentId) as { id: number } | null;
      if (!doc) errors.push(`documentId ${payload.documentId} does not exist`);
    }
  }

  const existingCreditJournal = existingCreditNoteJournal(db, payload.documentId);
  if (existingCreditJournal) {
    errors.push(
      `credit note document ${payload.documentId} already has accounting journal ${existingCreditJournal.entry_no}; issue a new credit note document for another correction`,
    );
  }

  if (payload.sourceBankTransactionId) {
    const bank = db.query("SELECT id FROM bank_transactions WHERE id = ?").get(payload.sourceBankTransactionId) as { id: number } | null;
    if (!bank) errors.push(`sourceBankTransactionId ${payload.sourceBankTransactionId} does not exist`);
    const historicalLink = db.query(
      "SELECT journal_entry_id FROM bank_journal_reconciliation_links WHERE bank_transaction_id = ?",
    ).get(payload.sourceBankTransactionId) as { journal_entry_id: number } | null;
    if (historicalLink) errors.push(`sourceBankTransactionId ${payload.sourceBankTransactionId} is already reconciled to journal entry ${historicalLink.journal_entry_id}`);
  }

  return { ok: errors.length === 0, appliedRules, errors };
}

/** Validate the public/manual journal contract. Import privileges are absent. */
export function validateJournalEntry(db: Database, payload: JournalEntryInput) {
  return validateJournalEntryWithPolicy(db, payload, MANUAL_POSTING_POLICY);
}

// Thrown when the period-lock re-check inside the write transaction fails: a
// period covering the transaction date was closed/reported between the initial
// validateJournalEntry and the insert (KODE-4 TOCTOU). postJournalEntry and
// reverseJournalEntry translate it back into a normal {ok:false} result.
class PeriodLockRaceError extends Error {
  constructor(public readonly periodErrors: string[]) {
    super(periodErrors.join("; "));
    this.name = "PeriodLockRaceError";
  }
}

class CreditNoteJournalRaceError extends Error {
  constructor(public readonly documentId: number, public readonly entryNo: string) {
    super(`credit note document ${documentId} already has accounting journal ${entryNo}; issue a new credit note document for another correction`);
    this.name = "CreditNoteJournalRaceError";
  }
}

// Inserts a validated entry into the append-only chain: journal row, lines,
// audit log and any bank-exception resolution. MUST run inside a db transaction
// — postJournalEntry commits it, dryRunJournalEntry rolls it back. The caller is
// responsible for having run validateJournalEntry first.
function applyJournalEntry(
  db: Database,
  payload: JournalEntryInput,
  accounts: ReturnType<typeof accountMap>,
  policy: JournalPostingPolicy,
): { entryId: JournalEntryId; entryNo: string; previousHash: string; entryHash: string } {
  // KODE-4: re-validate the period lock HERE, inside the BEGIN IMMEDIATE write
  // transaction, not only in the caller's earlier validateJournalEntry. The
  // earlier check runs before the write lock is held, so a period covering this
  // date could be closed in the gap between validation and insert. Repeating the
  // check under the held write lock closes that TOCTOU window: a date that has
  // since fallen into a closed/reported period aborts the post (and, via the
  // surrounding transaction, rolls back) instead of slipping into a locked
  // period. dryRunJournalEntry rolls back regardless, so the extra check is a
  // harmless no-op there.
  const periodErrors = validateJournalTransactionDate(db, payload.transactionDate);
  if (periodErrors.length > 0) throw new PeriodLockRaceError(periodErrors);

  const existingCreditJournal = existingCreditNoteJournal(db, payload.documentId);
  if (existingCreditJournal && payload.documentId != null) {
    throw new CreditNoteJournalRaceError(payload.documentId, existingCreditJournal.entry_no);
  }

  const entryId = nextEntryId(db);
  const entryNo = nextEntryNo(db, payload.transactionDate);
  const prevHash = previousHash(db);
  const actor = resolveActor({ createdBy: payload.createdBy, createdByProgram: payload.createdByProgram });
  // Explicit line classification is authoritative. A chart default is only a
  // historical-import convenience, selected only by an internal posting
  // policy; it is never a shortcut for manual/API journals.
  const canonicalLines = payload.lines.map((line) => {
    const explicitVatCode =
      typeof line.vatCode === "string" ? line.vatCode.trim() : "";
    return {
      account_no: line.accountNo,
      debit_amount: normalizeAmount(line.debitAmount),
      credit_amount: normalizeAmount(line.creditAmount),
      vat_code:
        explicitVatCode.length > 0
          ? explicitVatCode
          : policy.historicalImport
            ? accounts.get(line.accountNo)?.default_vat_code ?? null
            : null,
      text: line.text ?? null,
    };
  });
  const entryDraft = {
    id: entryId,
    entry_no: entryNo,
    transaction_date: payload.transactionDate,
    text: payload.text,
    source_bank_transaction_id: payload.sourceBankTransactionId ?? null,
    document_id: payload.documentId ?? null,
    currency: (payload.currency ?? 'DKK').trim().toUpperCase(),
    amount_foreign: payload.amountForeign == null ? null : roundDkk(payload.amountForeign),
    amount_dkk: payload.amountDkk == null ? null : roundDkk(payload.amountDkk),
    fx_rate_to_dkk: payload.fxRateToDkk == null ? null : roundRate6(payload.fxRateToDkk),
    rule_version: RULE_VERSION,
    created_by: actor.createdBy,
    created_by_program: actor.createdByProgram,
    status: 'posted',
    reversal_of_entry_id: null,
  };
  const entryHash = hashEntry(canonicalEntryData(entryDraft, canonicalLines), prevHash);

  const insertEntry = db.query(
    `INSERT INTO journal_entries (
      id, entry_no, transaction_date, text, source_bank_transaction_id, document_id,
      currency, amount_foreign, amount_dkk, fx_rate_to_dkk,
      rule_version, created_by, created_by_program, status, previous_hash, entry_hash, retain_until
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'posted', ?, ?, ?)
    RETURNING id, entry_no`
  );

  const entry = insertEntry.get(
    entryId,
    entryNo,
    payload.transactionDate,
    payload.text,
    payload.sourceBankTransactionId ?? null,
    payload.documentId ?? null,
    (payload.currency ?? 'DKK').trim().toUpperCase(),
    payload.amountForeign == null ? null : roundDkk(payload.amountForeign),
    payload.amountDkk == null ? null : roundDkk(payload.amountDkk),
    payload.fxRateToDkk == null ? null : roundRate6(payload.fxRateToDkk),
    RULE_VERSION,
    actor.createdBy,
    actor.createdByProgram,
    prevHash,
    entryHash,
    retainUntilForDate(db, payload.transactionDate),
  ) as any;

  const insertLine = db.prepare(
    `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, vat_code, currency, text)
     VALUES (?, ?, ?, ?, ?, 'DKK', ?)`
  );

  canonicalLines.forEach((line) => {
    const account = accounts.get(line.account_no)!;
    insertLine.run(entry.id, account.id, line.debit_amount, line.credit_amount, line.vat_code, line.text);
  });

  insertAuditLog(db, {
    eventType: "journal_post",
    entityType: "journal_entry",
    entityId: entry.id,
    message: `Posted journal entry ${entry.entry_no}`,
    createdBy: actor.createdBy,
    createdByProgram: actor.createdByProgram,
  });

  if (payload.sourceBankTransactionId) {
    resolveOpenExceptionsForBankTransaction(
      db,
      payload.sourceBankTransactionId,
      `Resolved automatically by posted journal entry ${entry.entry_no}`,
      actor.createdBy,
    );
  }

  return { entryId: asJournalEntryId(entry.id), entryNo: entry.entry_no, previousHash: prevHash, entryHash };
}

function postJournalEntryWithPolicy(
  db: Database,
  payload: JournalEntryInput,
  policy: JournalPostingPolicy,
): JournalPostResult {
  return db.transaction(() => postJournalEntryInCurrentTransactionWithPolicy(db, payload, policy)).immediate();
}

function postJournalEntryInCurrentTransactionWithPolicy(
  db: Database,
  payload: JournalEntryInput,
  policy: JournalPostingPolicy,
): JournalPostResult {
  const validation = validateJournalEntryWithPolicy(db, payload, policy);
  if (!validation.ok) return { ok: false, appliedRules: validation.appliedRules, errors: validation.errors };

  const accounts = accountMap(db);

  let applied: ReturnType<typeof applyJournalEntry>;
  try {
    applied = applyJournalEntry(db, payload, accounts, policy);
  } catch (error) {
    // KODE-4: the period-lock re-check inside the transaction lost the race —
    // the period was closed after validation. Surface it as a normal error
    // result; the transaction has already rolled back, so nothing was written.
    if (error instanceof PeriodLockRaceError) {
      return { ok: false, appliedRules: validation.appliedRules, errors: error.periodErrors };
    }
    if (error instanceof CreditNoteJournalRaceError) {
      return { ok: false, appliedRules: validation.appliedRules, errors: [error.message] };
    }
    throw error;
  }

  const { previousHash: _previousHash, ...result } = applied;
  return { ok: true, appliedRules: validation.appliedRules, errors: [], ...result };
}

/** Public/manual journal posting. Import privileges cannot be supplied in data. */
export function postJournalEntry(db: Database, payload: JournalEntryInput): JournalPostResult {
  return postJournalEntryWithPolicy(db, payload, MANUAL_POSTING_POLICY);
}

/**
 * Internal transaction-aware adapter for a larger atomic domain workflow.
 * The caller MUST invoke this inside its own immediate SQLite transaction.
 * It exists so reviewed draft evidence and the journal post can commit or
 * roll back together without relying on nested transaction semantics.
 */
export function postJournalEntryInCurrentTransaction(db: Database, payload: JournalEntryInput): JournalPostResult {
  return postJournalEntryInCurrentTransactionWithPolicy(db, payload, MANUAL_POSTING_POLICY);
}

/**
 * Internal adapter for verified source vouchers. It hardcodes the only
 * persisted import marker, waives missing Rentemester documents for the replay,
 * and may persist a reviewed account default when the source line lacks a code.
 * No equivalent flag exists on the public JournalEntryInput/CLI/MCP contract.
 *
 * @internal Used only by `core/import/dinero-postings.ts`.
 */
export function postVerifiedHistoricalImportEntry(
  db: Database,
  payload: Omit<JournalEntryInput, "createdByProgram">,
): JournalPostResult {
  return postJournalEntryWithPolicy(
    db,
    { ...payload, createdByProgram: HISTORICAL_IMPORT_PROGRAM },
    HISTORICAL_IMPORT_POSTING_POLICY,
  );
}

// Thrown to unwind a dry-run transaction once its effects have been captured.
// Identity is checked so a genuine failure inside applyJournalEntry still
// propagates instead of being swallowed as an intentional rollback.
class DryRunRollback extends Error {
  constructor() {
    super("dry-run rollback");
    this.name = "DryRunRollback";
  }
}

// Running debit-minus-credit balance, in øre, for each given account, summed
// per line in bigint to stay exact (no float drift from a SQL SUM).
function accountBalanceSnapshot(
  db: Database,
  accountNos: string[],
): Map<string, { name: string; balanceOre: bigint }> {
  const nameStmt = db.query("SELECT name FROM accounts WHERE account_no = ?");
  const lineStmt = db.query(
    `SELECT jl.debit_amount AS debit, jl.credit_amount AS credit
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
      WHERE a.account_no = ?`,
  );
  const out = new Map<string, { name: string; balanceOre: bigint }>();
  for (const accountNo of accountNos) {
    // accountNos come from a payload validateJournalEntry has already accepted,
    // so every account is guaranteed to exist.
    const nameRow = nameStmt.get(accountNo) as { name: string };
    let balanceOre = 0n;
    for (const line of lineStmt.all(accountNo) as Array<{ debit: number; credit: number }>) {
      balanceOre += toOre(Number(line.debit ?? 0)) - toOre(Number(line.credit ?? 0));
    }
    out.set(accountNo, { name: nameRow.name, balanceOre });
  }
  return out;
}

// Non-binding preview of postJournalEntry. Validates the payload, then runs the
// exact same insert path inside a transaction that is always rolled back, so it
// can report the entry number, hash-chain continuation and per-account balance
// effect *without* writing anything — not even the allocated sequence number.
export function dryRunJournalEntry(db: Database, payload: JournalEntryInput): JournalDryRunResult {
  const validation = validateJournalEntryWithPolicy(db, payload, MANUAL_POSTING_POLICY);
  if (!validation.ok) return { ok: false, appliedRules: validation.appliedRules, errors: validation.errors };

  const accounts = accountMap(db);
  const accountNos = [...new Set(payload.lines.map((line) => line.accountNo))].sort();

  let preview:
    | { applied: ReturnType<typeof applyJournalEntry>; effects: JournalDryRunAccountEffect[] }
    | undefined;
  const rollback = new DryRunRollback();
  try {
    db.transaction(() => {
      const before = accountBalanceSnapshot(db, accountNos);
      const applied = applyJournalEntry(db, payload, accounts, MANUAL_POSTING_POLICY);
      const after = accountBalanceSnapshot(db, accountNos);
      const effects = accountNos.map((accountNo) => {
        const beforeOre = before.get(accountNo)!.balanceOre;
        const afterSnapshot = after.get(accountNo)!;
        return {
          accountNo,
          accountName: afterSnapshot.name,
          balanceBefore: fromOre(beforeOre),
          balanceAfter: fromOre(afterSnapshot.balanceOre),
          delta: fromOre(afterSnapshot.balanceOre - beforeOre),
        };
      });
      preview = { applied, effects };
      // Unwinds the transaction: the journal rows, audit log, any resolved bank
      // exceptions and the allocated journal number are all discarded.
      throw rollback;
    }).immediate();
  } catch (error) {
    if (error !== rollback) throw error;
  }

  // The transaction body always sets `preview` before throwing `rollback`; a
  // genuine failure is re-thrown above. Guard the invariant explicitly so a
  // future change cannot silently produce an undefined preview.
  if (!preview) throw new Error("dry run completed without capturing a preview");
  const { applied, effects } = preview;
  return {
    ok: true,
    appliedRules: validation.appliedRules,
    errors: [],
    entryId: applied.entryId,
    entryNo: applied.entryNo,
    previousHash: applied.previousHash,
    entryHash: applied.entryHash,
    accountEffects: effects,
  };
}

type ReverseJournalInput = {
  entryId: JournalEntryId;
  transactionDate: string;
  reason: string;
  createdBy?: string;
  createdByProgram?: string;
};

type IssuedInvoiceRepairReversalAuthorization = {
  invoiceDocumentId: number;
  replacementJournalEntryId: number;
};

function protectedInvoiceReversalError(
  db: Database,
  original: {
    id: number;
    entry_no: string;
    document_id: number | null;
    transaction_date: string;
  },
  input: ReverseJournalInput,
  authorization?: IssuedInvoiceRepairReversalAuthorization,
): string | null {
  if (original.document_id == null) return null;
  const document = db.query(
    `SELECT document_type FROM documents
      WHERE id = ? AND document_type IN ('issued_invoice', 'credit_note')`,
  ).get(original.document_id) as { document_type: string } | null;
  if (!document) return null;
  const genericError = `journal entry ${input.entryId} is protected invoice evidence (${document.document_type}) and cannot be reversed without an atomic invoice correction workflow`;
  if (
    document.document_type !== "issued_invoice" ||
    !authorization ||
    authorization.invoiceDocumentId !== original.document_id ||
    input.transactionDate !== original.transaction_date
  ) return genericError;

  const repair = validateLegacyInvoiceRepairEvidence(db, {
    invoiceDocumentId: authorization.invoiceDocumentId,
    legacyJournalEntryId: original.id,
    replacementJournalEntryId: authorization.replacementJournalEntryId,
  });
  return repair.ok ? null : `${genericError}: ${repair.errors.join("; ")}`;
}

function reverseJournalEntryInternal(
  db: Database,
  input: ReverseJournalInput,
  authorization?: IssuedInvoiceRepairReversalAuthorization,
  inCurrentTransaction = false,
): JournalReverseResult {
  const appliedRules = [LEDGER_RULES.APPEND_ONLY, LEDGER_RULES.REVERSAL];
  const errors: string[] = [];

  if (!Number.isInteger(input.entryId) || input.entryId <= 0) errors.push("entryId must be a positive integer");
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be present in YYYY-MM-DD format");
  if (typeof input.reason !== "string" || input.reason.trim().length === 0) errors.push("reason is required");
  if (errors.length > 0) return { ok: false, appliedRules, errors };

  const original = db.query(
    `SELECT id, entry_no, transaction_date, text, source_bank_transaction_id, document_id, currency, amount_foreign, amount_dkk, fx_rate_to_dkk, rule_version, created_by, created_by_program, status, reversal_of_entry_id
     FROM journal_entries WHERE id = ?`
  ).get(input.entryId) as any | null;
  if (!original) return { ok: false, appliedRules, errors: [`journal entry ${input.entryId} does not exist`] };
  if (original.reversal_of_entry_id != null) return { ok: false, appliedRules, errors: [`journal entry ${input.entryId} is already a reversal entry`] };

  // Invoice domain rows are append-only and currently have no atomic
  // superseding/relink workflow. Reversing their journal in isolation would
  // leave the invoice/payment/claim row pointing at dead evidence forever and
  // could strand a bank source or receivable balance. Refuse until the domain
  // can reverse both the journal and its application state atomically.
  const protectedError = protectedInvoiceReversalError(db, original, input, authorization);
  if (protectedError) {
    return {
      ok: false,
      appliedRules,
      errors: [protectedError],
    };
  }

  const existingReversal = db.query("SELECT id, entry_no FROM journal_entries WHERE reversal_of_entry_id = ? LIMIT 1").get(input.entryId) as { id: number; entry_no: string } | null;
  if (existingReversal) {
    return { ok: false, appliedRules, errors: [`journal entry ${input.entryId} already has reversal ${existingReversal.entry_no}`] };
  }

  const originalLines = db.query(
    `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
     FROM journal_lines jl
     JOIN accounts a ON a.id = jl.account_id
     WHERE jl.journal_entry_id = ?
     ORDER BY jl.id ASC`
  ).all(input.entryId) as any[];
  if (originalLines.length === 0) {
    return { ok: false, appliedRules, errors: [`journal entry ${input.entryId} has no lines to reverse`] };
  }

  // Reversing an imported-historical voucher (#195) that has no Rentemester
  // document yet (#196 attaches the bilag later): the reversal copies the same
  // document-less income/expense lines, so it must inherit the SAME exemption
  // the original carries — otherwise validateJournalEntry rejects it with
  // "documentId is required when posting expense or income lines" and a
  // legitimately posted imported voucher could never be corrected. The
  // reversal selects the internal historical policy and retains the original's
  // reserved import marker. When the original DID carry a document, the
  // reversal copies that document and needs no exemption.
  const fromImport =
    IMPORTED_HISTORICAL_PROGRAMS.has(original.created_by_program) &&
    original.document_id == null;

  const reversalPayload: JournalEntryInput = {
    transactionDate: input.transactionDate,
    text: `Reversal of ${original.entry_no}: ${input.reason.trim()}`,
    documentId: original.document_id ?? undefined,
    sourceBankTransactionId: original.source_bank_transaction_id ?? undefined,
    currency: original.currency ?? 'DKK',
    amountForeign: original.amount_foreign ?? undefined,
    amountDkk: original.amount_dkk ?? undefined,
    fxRateToDkk: original.fx_rate_to_dkk ?? undefined,
    createdBy: input.createdBy,
    createdByProgram: fromImport ? original.created_by_program : input.createdByProgram,
    lines: originalLines.map((line) => ({
      accountNo: line.account_no,
      debitAmount: normalizeAmount(line.credit_amount) || undefined,
      creditAmount: normalizeAmount(line.debit_amount) || undefined,
      vatCode: line.vat_code ?? undefined,
      text: line.text ? `REVERSAL: ${line.text}` : `REVERSAL of ${original.entry_no}`,
    })),
  };

  const reversalPolicy: JournalPostingPolicy = {
    historicalImport: fromImport,
    allowUncodedVatControl: true,
  };
  const validation = validateJournalEntryWithPolicy(db, reversalPayload, reversalPolicy);
  if (!validation.ok) return { ok: false, appliedRules: [...new Set([...appliedRules, ...validation.appliedRules])], errors: validation.errors };

  const accounts = accountMap(db);

  let result: { entryId: JournalEntryId; entryNo: string; entryHash: string };
  try {
    const apply = () => {
    const protectedInsideLock = protectedInvoiceReversalError(db, original, input, authorization);
    if (protectedInsideLock) throw new Error(protectedInsideLock);
    // KODE-4: re-check the period lock inside the write transaction, matching
    // applyJournalEntry — a reversal date that fell into a period closed after
    // the outer validation must abort here rather than slip into a locked period.
    const periodErrors = validateJournalTransactionDate(db, reversalPayload.transactionDate);
    if (periodErrors.length > 0) throw new PeriodLockRaceError(periodErrors);
    const entryId = nextEntryId(db);
    const entryNo = nextEntryNo(db, reversalPayload.transactionDate);
    const prevHash = previousHash(db);
    const actor = resolveActor({ createdBy: reversalPayload.createdBy, createdByProgram: reversalPayload.createdByProgram });
    const canonicalLines = reversalPayload.lines.map((line) => ({
      account_no: line.accountNo,
      debit_amount: normalizeAmount(line.debitAmount),
      credit_amount: normalizeAmount(line.creditAmount),
      vat_code: line.vatCode ?? null,
      text: line.text ?? null,
    }));
    const entryDraft = {
      id: entryId,
      entry_no: entryNo,
      transaction_date: reversalPayload.transactionDate,
      text: reversalPayload.text,
      source_bank_transaction_id: reversalPayload.sourceBankTransactionId ?? null,
      document_id: reversalPayload.documentId ?? null,
      currency: (reversalPayload.currency ?? 'DKK').trim().toUpperCase(),
      amount_foreign: reversalPayload.amountForeign == null ? null : roundDkk(reversalPayload.amountForeign),
      amount_dkk: reversalPayload.amountDkk == null ? null : roundDkk(reversalPayload.amountDkk),
      fx_rate_to_dkk: reversalPayload.fxRateToDkk == null ? null : roundRate6(reversalPayload.fxRateToDkk),
      rule_version: RULE_VERSION,
      created_by: actor.createdBy,
      created_by_program: actor.createdByProgram,
      status: 'reversed',
      reversal_of_entry_id: original.id,
    };
    const entryHash = hashEntry(canonicalEntryData(entryDraft, canonicalLines), prevHash);

    const entry = db.query(
      `INSERT INTO journal_entries (
        id, entry_no, transaction_date, text, source_bank_transaction_id, document_id,
        currency, amount_foreign, amount_dkk, fx_rate_to_dkk,
        rule_version, created_by, created_by_program, status, reversal_of_entry_id, previous_hash, entry_hash, retain_until
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'reversed', ?, ?, ?, ?)
      RETURNING id, entry_no`
    ).get(
      entryId,
      entryNo,
      reversalPayload.transactionDate,
      reversalPayload.text,
      reversalPayload.sourceBankTransactionId ?? null,
      reversalPayload.documentId ?? null,
      (reversalPayload.currency ?? 'DKK').trim().toUpperCase(),
      reversalPayload.amountForeign == null ? null : roundDkk(reversalPayload.amountForeign),
      reversalPayload.amountDkk == null ? null : roundDkk(reversalPayload.amountDkk),
      reversalPayload.fxRateToDkk == null ? null : roundRate6(reversalPayload.fxRateToDkk),
      RULE_VERSION,
      actor.createdBy,
      actor.createdByProgram,
      original.id,
      prevHash,
      entryHash,
      retainUntilForDate(db, reversalPayload.transactionDate),
    ) as any;

    const insertLine = db.prepare(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, vat_code, currency, text)
       VALUES (?, ?, ?, ?, ?, 'DKK', ?)`
    );
    canonicalLines.forEach((line) => {
      const account = accounts.get(line.account_no)!;
      insertLine.run(entry.id, account.id, line.debit_amount, line.credit_amount, line.vat_code, line.text);
    });

    insertAuditLog(db, {
      eventType: "journal_reverse",
      entityType: "journal_entry",
      entityId: entry.id,
      message: `Reversed journal entry ${original.entry_no} with ${entry.entry_no}`,
      createdBy: actor.createdBy,
      createdByProgram: actor.createdByProgram,
    });

    return { entryId: asJournalEntryId(entry.id), entryNo: entry.entry_no, entryHash };
    };
    result = inCurrentTransaction ? apply() : db.transaction(apply).immediate();
  } catch (error) {
    if (error instanceof PeriodLockRaceError) {
      return { ok: false, appliedRules: [...new Set([...appliedRules, ...validation.appliedRules])], errors: error.periodErrors };
    }
    throw error;
  }

  return { ok: true, originalEntryId: asJournalEntryId(original.id), appliedRules: [...new Set([...appliedRules, ...validation.appliedRules])], errors: [], ...result };
}

export function reverseJournalEntry(db: Database, input: ReverseJournalInput): JournalReverseResult {
  return reverseJournalEntryInternal(db, input);
}

/** Internal adapter for an already-held immediate transaction (#583). */
export function reverseJournalEntryInCurrentTransaction(db: Database, input: ReverseJournalInput): JournalReverseResult {
  return reverseJournalEntryInternal(db, input, undefined, true);
}

/**
 * Narrow adapter used only by the atomic issued-invoice legacy repair. It does
 * not expose a generic bypass: the original date is immutable, and the helper
 * revalidates the exact active replacement plus the dependency-free invoice
 * state both before and inside the ledger write lock.
 */
export function reverseUnlinkedIssuedInvoiceJournalAfterReplacement(
  db: Database,
  input: {
    invoiceDocumentId: number;
    legacyJournalEntryId: number;
    replacementJournalEntryId: number;
    reason: string;
    createdBy?: string;
    createdByProgram?: string;
  },
): JournalReverseResult {
  const original = db.query(
    "SELECT transaction_date FROM journal_entries WHERE id = ?",
  ).get(input.legacyJournalEntryId) as { transaction_date: string } | null;
  if (!original) {
    return {
      ok: false,
      appliedRules: [LEDGER_RULES.APPEND_ONLY, LEDGER_RULES.REVERSAL],
      errors: [`journal entry ${input.legacyJournalEntryId} does not exist`],
    };
  }
  return reverseJournalEntryInternal(
    db,
    {
      entryId: asJournalEntryId(input.legacyJournalEntryId),
      transactionDate: original.transaction_date,
      reason: input.reason,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
    },
    {
      invoiceDocumentId: input.invoiceDocumentId,
      replacementJournalEntryId: input.replacementJournalEntryId,
    },
  );
}

export type VerifyAuditChainOptions = {
  /**
   * Root whose on-disk evidence is being verified. Restore must pass its
   * staging root explicitly; normal live callers can rely on DB-path inference.
   */
  companyRoot?: string;
};

function inferredAuditCompanyRoot(db: Database, explicit?: string) {
  if (typeof explicit === "string" && explicit.trim().length > 0) {
    return resolve(explicit.trim());
  }
  const filename = (db as unknown as { filename?: string }).filename;
  if (typeof filename !== "string" || filename.length === 0 || filename === ":memory:") {
    return null;
  }
  return dirname(dirname(resolve(filename)));
}

type AuditEvidenceDocument = {
  id: number;
  document_no?: string | null;
  sha256_hash: string | null;
  stored_path: string | null;
  document_type: string;
};

/**
 * Validate one registered document without ever reading its historical
 * absolute `stored_path` directly. The canonical evidence resolver rebases a
 * known store suffix below the current company and verifies the exact bytes.
 * Returned errors deliberately contain no absolute paths.
 */
function verifyRegisteredDocumentEvidence(
  document: AuditEvidenceDocument,
  companyRoot: string | null,
): string | null {
  if (!document.stored_path || document.stored_path.trim().length === 0) {
    return "has no stored_path";
  }
  if (!document.sha256_hash || !/^[0-9a-f]{64}$/i.test(document.sha256_hash.trim())) {
    return "has an invalid sha256_hash";
  }
  if (!companyRoot) return "cannot resolve the current company root";

  const expectedRelativeStore = isIssuedDocumentEvidence(document.document_type)
    ? "invoices/issued"
    : "documents/originals";
  try {
    snapshotRegisteredDocumentEvidence(companyRoot, {
      storedPath: document.stored_path,
      expectedSha256: document.sha256_hash,
      documentType: document.document_type,
    });
    return null;
  } catch (error) {
    if (error instanceof DocumentEvidenceError) {
      if (error.reason === "hash_mismatch") {
        return "stored evidence sha256 does not match the document register";
      }
      if (error.reason === "invalid_path") {
        return `stored_path is invalid or outside the ${expectedRelativeStore} evidence store`;
      }
      if (error.reason === "unsafe_store") {
        return `${expectedRelativeStore} evidence store is not a safe regular directory`;
      }
      if (error.reason === "unsafe_file" || error.reason === "invalid_size") {
        return "stored evidence path is not a safe regular file";
      }
    }
    return "stored evidence file is missing or inaccessible";
  }
}

export function verifyAuditChain(db: Database, options: VerifyAuditChainOptions = {}) {
  const entries = db.query("SELECT id, entry_no, transaction_date, text, source_bank_transaction_id, document_id, currency, amount_foreign, amount_dkk, fx_rate_to_dkk, rule_version, created_by, created_by_program, status, reversal_of_entry_id, previous_hash, entry_hash FROM journal_entries ORDER BY id ASC").all() as any[];
  let prev = "GENESIS";
  const errors: string[] = [];
  const entriesById = new Map<number, any>(entries.map((entry) => [Number(entry.id), entry]));
  const linesByEntryId = new Map<number, any[]>();
  const reversalCounts = new Map<number, number>();
  for (const entry of entries) {
    if (entry.reversal_of_entry_id == null) continue;
    const originalId = Number(entry.reversal_of_entry_id);
    reversalCounts.set(originalId, (reversalCounts.get(originalId) ?? 0) + 1);
  }
  for (const [originalId, count] of reversalCounts) {
    if (count <= 1) continue;
    const original = entriesById.get(originalId);
    errors.push(`${original?.entry_no ?? `journal entry id ${originalId}`}: has ${count} reversal rows; exactly one is allowed`);
  }
  const companyRoot = inferredAuditCompanyRoot(db, options.companyRoot);
  const documentEvidenceCache = new Map<number, string | null>();
  const journalReferencedDocumentIds = new Set<number>();

  const foreignKeyErrors = db.query("PRAGMA foreign_key_check").all() as Array<{ table: string; rowid: number; parent: string; fkid: number }>;
  for (const fk of foreignKeyErrors) {
    errors.push(`foreign key violation: ${fk.table} row ${fk.rowid} references missing ${fk.parent}`);
  }

  const bankJournalLinks = db.query(
    `SELECT link.id, link.bank_transaction_id, link.journal_entry_id,
            bt.amount, bt.amount_dkk, bt.currency,
            ba.ledger_account_no,
            je.status, je.reversal_of_entry_id, je.source_bank_transaction_id,
            EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id = je.id) AS has_reversal,
            EXISTS(SELECT 1 FROM bank_reconciliation_correction_events correction
                    WHERE correction.supersedes_kind = 'append-only'
                      AND correction.supersedes_id = link.id
                      AND correction.bank_transaction_id = link.bank_transaction_id) AS is_superseded,
            COALESCE(SUM(CASE WHEN a.account_no = ba.ledger_account_no THEN jl.debit_amount - jl.credit_amount ELSE 0 END), 0) AS journal_bank_movement,
            EXISTS(SELECT 1 FROM journal_entries direct WHERE direct.source_bank_transaction_id = bt.id) AS has_direct_link
       FROM bank_journal_reconciliation_links link
       JOIN bank_transactions bt ON bt.id = link.bank_transaction_id
       LEFT JOIN bank_accounts ba ON ba.id = bt.bank_account_id
       JOIN journal_entries je ON je.id = link.journal_entry_id
       LEFT JOIN journal_lines jl ON jl.journal_entry_id = je.id
       LEFT JOIN accounts a ON a.id = jl.account_id
      GROUP BY link.id
      ORDER BY link.id`,
  ).all() as Array<{
    id: number; bank_transaction_id: number; journal_entry_id: number;
    amount: number; amount_dkk: number | null; currency: string;
    ledger_account_no: string | null; status: string; reversal_of_entry_id: number | null;
    source_bank_transaction_id: number | null; has_reversal: number;
    journal_bank_movement: number; has_direct_link: number; is_superseded: number;
  }>;
  for (const link of bankJournalLinks) {
    const label = `bank-journal reconciliation link ${link.id}`;
    if (!link.ledger_account_no) errors.push(`${label}: bank transaction has no mapped ledger account`);
    if (!Number(link.is_superseded) && (link.status !== "posted" || link.reversal_of_entry_id != null || Number(link.has_reversal) !== 0)) {
      errors.push(`${label}: journal entry ${link.journal_entry_id} is not an active original posted entry`);
    }
    if (link.source_bank_transaction_id != null || Number(link.has_direct_link) !== 0) {
      errors.push(`${label}: conflicts with a direct bank link`);
    }
    const bankDkk = String(link.currency).trim().toUpperCase() === "DKK" ? Number(link.amount) : Number(link.amount_dkk);
    if (!Number.isFinite(bankDkk) || compareDkk(Number(link.journal_bank_movement), bankDkk) !== 0) {
      errors.push(`${label}: journal bank movement does not reconcile exactly to the bank amount in DKK`);
    }
  }

  // v33 correction evidence is append-only but deliberately richer than an
  // audit-log message: verify the complete same-bank chain and the exact
  // replacement bytes so every consumer sees one deterministic current row.
  const correctionEvents = db.query(`SELECT event.*, bt.amount,bt.amount_dkk,bt.currency,COALESCE(ba.ledger_account_no,(SELECT m.account_no FROM account_role_mappings m WHERE m.role='bank' AND m.status='confirmed' ORDER BY m.version DESC LIMIT 1)) AS ledger_account_no,je.entry_hash,je.status,je.reversal_of_entry_id,je.source_bank_transaction_id,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=je.id) AS has_reversal,EXISTS(SELECT 1 FROM bank_reconciliation_correction_events child WHERE child.supersedes_kind='correction' AND child.supersedes_id=event.id AND child.bank_transaction_id=event.bank_transaction_id) AS is_superseded,COALESCE(SUM(CASE WHEN a.account_no=event.bank_account_no THEN jl.debit_amount-jl.credit_amount ELSE 0 END),0) AS bank_movement,EXISTS(SELECT 1 FROM audit_log audit WHERE audit.event_type='bank_reconciliation_corrected' AND audit.entity_type='bank_transaction' AND audit.entity_id=CAST(event.bank_transaction_id AS TEXT) AND (audit.actor=event.actor OR audit.actor LIKE event.actor || ' via %')) AS has_audit FROM bank_reconciliation_correction_events event JOIN bank_transactions bt ON bt.id=event.bank_transaction_id LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id JOIN journal_entries je ON je.id=event.replacement_journal_entry_id LEFT JOIN journal_lines jl ON jl.journal_entry_id=je.id LEFT JOIN accounts a ON a.id=jl.account_id GROUP BY event.id ORDER BY event.id`).all() as any[];
  for (const event of correctionEvents) {
    const label = `bank reconciliation correction ${event.id}`;
    const amount = String(event.currency).trim().toUpperCase() === "DKK" ? Number(event.amount) : Number(event.amount_dkk);
    if (event.ledger_account_no !== event.bank_account_no || compareDkk(amount, Number(event.bank_amount_dkk)) !== 0 || compareDkk(amount, Number(event.bank_movement)) !== 0) errors.push(`${label}: bank account or amount evidence is inconsistent`);
    if (event.entry_hash !== event.replacement_journal_hash || event.status !== "posted" || event.reversal_of_entry_id != null || event.source_bank_transaction_id != null || (!Number(event.is_superseded) && Number(event.has_reversal) !== 0)) errors.push(`${label}: replacement journal hash or lifecycle is invalid`);
    if (!event.has_audit) errors.push(`${label}: missing bank_reconciliation_corrected audit event`);
    const target = `${event.supersedes_kind}:${event.supersedes_id}`;
    if (event.supersedes_reconciliation_id !== target) errors.push(`${label}: typed supersession target is inconsistent`);
  }
  const hasPurchaseCorrections = db.query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='direct_bank_purchase_payable_corrections'").get();
  const purchaseCorrections = hasPurchaseCorrections ? db.query(`
    SELECT correction.*,
           document.sha256_hash AS current_document_hash,
           original.entry_hash AS current_original_hash,
           reversal.reversal_of_entry_id AS reversal_target,
           payable.document_id AS payable_document_id,
           payment.payable_id AS payment_payable_id,
           payment.bank_transaction_id AS payment_bank_id,
           payment.journal_entry_id AS payment_journal_id,
           EXISTS(SELECT 1 FROM bank_reconciliation_correction_events event
                    WHERE event.bank_transaction_id=correction.bank_transaction_id
                      AND event.replacement_journal_entry_id=correction.settlement_journal_entry_id) AS has_reconciliation_correction,
           EXISTS(SELECT 1 FROM audit_log audit
                    WHERE audit.event_type='direct_bank_purchase_payable_corrected'
                      AND audit.entity_type='bank_transaction'
                      AND audit.entity_id=CAST(correction.bank_transaction_id AS TEXT)
                      AND (audit.actor=correction.actor OR audit.actor LIKE correction.actor || ' via %')) AS has_audit
      FROM direct_bank_purchase_payable_corrections correction
      JOIN documents document ON document.id=correction.document_id
      JOIN journal_entries original ON original.id=correction.original_journal_entry_id
      JOIN journal_entries reversal ON reversal.id=correction.reversal_journal_entry_id
      JOIN payables payable ON payable.id=correction.payable_id
      JOIN payable_payments payment ON payment.id=correction.payment_id
     ORDER BY correction.id`).all() as any[] : [];
  for (const correction of purchaseCorrections) {
    const label = `direct-bank purchase payable correction ${correction.id}`;
    if (correction.current_document_hash !== correction.document_hash || correction.current_original_hash !== correction.original_journal_hash) errors.push(`${label}: immutable source hash mismatch`);
    if (Number(correction.reversal_target) !== Number(correction.original_journal_entry_id)) errors.push(`${label}: reversal linkage is invalid`);
    if (Number(correction.payable_document_id) !== Number(correction.document_id) || Number(correction.payment_payable_id) !== Number(correction.payable_id) || Number(correction.payment_bank_id) !== Number(correction.bank_transaction_id) || Number(correction.payment_journal_id) !== Number(correction.settlement_journal_entry_id)) errors.push(`${label}: payable/payment linkage is invalid`);
    if (!Number(correction.has_reconciliation_correction) || !Number(correction.has_audit)) errors.push(`${label}: reconciliation correction or audit evidence is missing`);
  }
  const badCurrent = db.query(`SELECT bank_transaction_id,COUNT(*) AS count FROM bank_journal_reconciliations GROUP BY bank_transaction_id HAVING COUNT(*)<>1`).all() as Array<{bank_transaction_id:number;count:number}>;
  for (const row of badCurrent) errors.push(`bank transaction ${row.bank_transaction_id}: has ${row.count} effective reconciliations; exactly one is required`);

  const orphanLines = db.query(
    `SELECT jl.id, jl.journal_entry_id
     FROM journal_lines jl
     LEFT JOIN journal_entries je ON je.id = jl.journal_entry_id
     WHERE je.id IS NULL
     ORDER BY jl.id ASC`
  ).all() as Array<{ id: number; journal_entry_id: number }>;
  for (const line of orphanLines) {
    errors.push(`journal_line ${line.id}: orphan journal_entry_id ${line.journal_entry_id}`);
  }

  const brokenAccountLines = db.query(
    `SELECT jl.id, jl.account_id
     FROM journal_lines jl
     LEFT JOIN accounts a ON a.id = jl.account_id
     WHERE a.id IS NULL
     ORDER BY jl.id ASC`
  ).all() as Array<{ id: number; account_id: number }>;
  for (const line of brokenAccountLines) {
    errors.push(`journal_line ${line.id}: missing account_id ${line.account_id}`);
  }

  for (const e of entries) {
    const lines = db.query(
      `SELECT a.account_no, a.type AS account_type, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ?
       ORDER BY jl.id ASC`
    ).all(e.id) as any[];
    linesByEntryId.set(Number(e.id), lines);
    if (lines.length === 0) errors.push(`${e.entry_no}: entry has no journal lines`);

    const canonicalLines = lines.map(({ account_type, ...line }) => line);
    const expected = hashEntry(canonicalEntryData(e, canonicalLines), prev);
    if (e.previous_hash !== prev) errors.push(`${e.entry_no}: previous_hash mismatch`);
    if (e.entry_hash !== expected) errors.push(`${e.entry_no}: entry_hash mismatch`);

    const debitSum = lines.reduce((sum, line) => sum + toOre(Number(line.debit_amount ?? 0)), 0n);
    const creditSum = lines.reduce((sum, line) => sum + toOre(Number(line.credit_amount ?? 0)), 0n);
    if (debitSum !== creditSum) errors.push(`${e.entry_no}: entry is unbalanced (${fromOre(debitSum)} != ${fromOre(creditSum)})`);
    if (
      (e.status === "reversed" && e.reversal_of_entry_id == null) ||
      (e.status === "posted" && e.reversal_of_entry_id != null)
    ) {
      errors.push(`${e.entry_no}: journal reversal status does not match reversal_of_entry_id`);
    }
    if (e.reversal_of_entry_id != null) {
      const originalId = Number(e.reversal_of_entry_id);
      const original = entriesById.get(originalId);
      if (!original) {
        errors.push(`${e.entry_no}: reversal target journal entry ${originalId} does not exist`);
      } else {
        if (original.status !== "posted" || original.reversal_of_entry_id != null) {
          errors.push(`${e.entry_no}: reversal target ${original.entry_no} is not an unreversed posted original`);
        }
        if (Number(original.id) >= Number(e.id)) {
          errors.push(`${e.entry_no}: reversal target ${original.entry_no} must precede the reversal row`);
        }

        const metadataMismatches: string[] = [];
        if ((e.document_id ?? null) !== (original.document_id ?? null)) metadataMismatches.push("document_id");
        if ((e.source_bank_transaction_id ?? null) !== (original.source_bank_transaction_id ?? null)) {
          metadataMismatches.push("source_bank_transaction_id");
        }
        if (String(e.currency ?? "DKK").trim().toUpperCase() !== String(original.currency ?? "DKK").trim().toUpperCase()) {
          metadataMismatches.push("currency");
        }
        const sameNullableAmount = (left: unknown, right: unknown) => {
          if (left == null || right == null) return left == null && right == null;
          return compareDkk(Number(left), Number(right)) === 0;
        };
        if (!sameNullableAmount(e.amount_foreign, original.amount_foreign)) metadataMismatches.push("amount_foreign");
        if (!sameNullableAmount(e.amount_dkk, original.amount_dkk)) metadataMismatches.push("amount_dkk");
        const sameNullableRate = (left: unknown, right: unknown) => {
          if (left == null || right == null) return left == null && right == null;
          return roundRate6(Number(left)) === roundRate6(Number(right));
        };
        if (!sameNullableRate(e.fx_rate_to_dkk, original.fx_rate_to_dkk)) metadataMismatches.push("fx_rate_to_dkk");
        if (metadataMismatches.length > 0) {
          errors.push(`${e.entry_no}: reversal metadata differs from ${original.entry_no} (${metadataMismatches.join(", ")})`);
        }

        const originalLines = linesByEntryId.get(originalId) ?? db.query(
          `SELECT a.account_no, a.type AS account_type, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
             FROM journal_lines jl
             JOIN accounts a ON a.id = jl.account_id
            WHERE jl.journal_entry_id = ?
            ORDER BY jl.id ASC`,
        ).all(originalId) as any[];
        let exactInverse = originalLines.length === lines.length;
        if (exactInverse) {
          for (let index = 0; index < lines.length; index += 1) {
            const reversalLine = lines[index];
            const originalLine = originalLines[index];
            if (
              reversalLine.account_no !== originalLine.account_no ||
              (reversalLine.vat_code ?? null) !== (originalLine.vat_code ?? null) ||
              toOre(Number(reversalLine.debit_amount ?? 0)) !== toOre(Number(originalLine.credit_amount ?? 0)) ||
              toOre(Number(reversalLine.credit_amount ?? 0)) !== toOre(Number(originalLine.debit_amount ?? 0))
            ) {
              exactInverse = false;
              break;
            }
          }
        }
        if (!exactInverse) {
          errors.push(`${e.entry_no}: reversal lines do not exactly invert ${original.entry_no}`);
        }
      }
    }

    const requiresDocument = lines.some((line) => line.account_type === "expense" || line.account_type === "income");
    // A replayed migration posting (#195) carries no Rentemester document yet —
    // #196 attaches the original bilag. It is identified by its import
    // `created_by_program` and exempted from the document-evidence check.
    const isImportedHistorical = IMPORTED_HISTORICAL_PROGRAMS.has(e.created_by_program);
    if (requiresDocument && !isImportedHistorical && e.document_id == null) {
      errors.push(`${e.entry_no}: income/expense entry is missing document evidence`);
    }
    if (e.document_id != null) {
      const document = db.query(
        "SELECT id, document_no, sha256_hash, stored_path, document_type FROM documents WHERE id = ?",
      ).get(e.document_id) as AuditEvidenceDocument | null;
      if (!document) {
        errors.push(`${e.entry_no}: document_id ${e.document_id} is missing`);
      } else {
        journalReferencedDocumentIds.add(document.id);
        let evidenceError = documentEvidenceCache.get(document.id);
        if (evidenceError === undefined && !documentEvidenceCache.has(document.id)) {
          evidenceError = verifyRegisteredDocumentEvidence(document, companyRoot);
          documentEvidenceCache.set(document.id, evidenceError);
        }
        if (evidenceError) {
          errors.push(`${e.entry_no}: document_id ${document.id} ${evidenceError}`);
        }
      }
    }

    prev = e.entry_hash;
  }

  // The document register is itself retained bookkeeping evidence. Verify all
  // rows, including invoice PDFs, balance-only journal attachments, and
  // ingested documents that have not yet been posted. A signed backup manifest
  // authenticates the backup package, but it must not be able to replace the
  // independent hash committed in this register.
  const registeredDocuments = db.query(
    `SELECT id, document_no, sha256_hash, stored_path, document_type
       FROM documents
      ORDER BY id ASC`,
  ).all() as AuditEvidenceDocument[];
  for (const document of registeredDocuments) {
    if (journalReferencedDocumentIds.has(document.id)) continue;
    let evidenceError = documentEvidenceCache.get(document.id);
    if (evidenceError === undefined && !documentEvidenceCache.has(document.id)) {
      evidenceError = verifyRegisteredDocumentEvidence(document, companyRoot);
      documentEvidenceCache.set(document.id, evidenceError);
    }
    if (evidenceError) {
      errors.push(`document ${document.document_no ?? document.id}: ${evidenceError}`);
    }
  }

  // Tail-truncation guard: the journal_entry sequence is monotonic and protected
  // against rollback, so it records the highest entry number ever allocated for
  // a fiscal scope. Deleting the most recent entries leaves a valid but shorter
  // chain; cross-checking the count and highest entry_no against the issued
  // sequence value detects those missing entries.
  const journalSequences = db.query(
    `SELECT scope, value FROM sequences WHERE kind = 'journal_entry'`
  ).all() as Array<{ scope: string; value: number }>;
  for (const sequence of journalSequences) {
    const fiscalScope = sequence.scope.slice(sequence.scope.lastIndexOf(":") + 1);
    if (!fiscalScope) continue;
    // KODE-12: width-robust suffix match. The old fixed `[0-9]{5}` GLOB +
    // `substr(-5)` silently dropped every six-digit (>99 999) entry, so a scope
    // that grew past the boundary read MAX=0 and falsely failed "entries are
    // missing". Anchor the `${scope}-` prefix by length and require the suffix
    // to be all digits, so any width matches and parses.
    const prefix = `${fiscalScope}-`;
    const stats = db.query(
      `SELECT COUNT(*) AS n, COALESCE(MAX(CAST(substr(entry_no, ? + 1) AS INTEGER)), 0) AS max_no
         FROM journal_entries
        WHERE substr(entry_no, 1, ?) = ?
          AND length(entry_no) > ?
          AND substr(entry_no, ? + 1) NOT GLOB '*[^0-9]*'`
    ).get(prefix.length, prefix.length, prefix, prefix.length, prefix.length) as { n: number; max_no: number };
    const expected = Number(sequence.value);
    if (Number(stats.max_no) < expected) {
      errors.push(`fiscal scope ${fiscalScope}: highest journal entry ${fiscalScope}-${String(stats.max_no).padStart(ENTRY_NO_MIN_DIGITS, "0")} is below issued sequence value ${expected} — entries are missing`);
    } else if (Number(stats.n) < expected) {
      errors.push(`fiscal scope ${fiscalScope}: ${stats.n} journal entries present but sequence issued ${expected} — entries are missing`);
    }
  }

  const bankLinkedEntries = db.query(
    `SELECT id, entry_no, source_bank_transaction_id, status, reversal_of_entry_id
     FROM journal_entries
     WHERE source_bank_transaction_id IS NOT NULL
     ORDER BY id ASC`
  ).all() as { id: number; entry_no: string; source_bank_transaction_id: number; status: string; reversal_of_entry_id: number | null }[];
  const byBankTransaction = new Map<number, typeof bankLinkedEntries>();
  for (const entry of bankLinkedEntries) {
    const existing = byBankTransaction.get(entry.source_bank_transaction_id) ?? [];
    existing.push(entry);
    byBankTransaction.set(entry.source_bank_transaction_id, existing);
  }
  for (const [bankTransactionId, group] of byBankTransaction.entries()) {
    if (group.length <= 1) continue;
    const cancelled = new Set<number>();
    const idsInGroup = new Set(group.map((entry) => entry.id));
    for (const entry of group) {
      if (entry.reversal_of_entry_id != null && idsInGroup.has(entry.reversal_of_entry_id)) {
        cancelled.add(entry.id);
        cancelled.add(entry.reversal_of_entry_id);
      }
    }
    const remaining = group.filter((entry) => !cancelled.has(entry.id));
    if (remaining.length > 1) {
      errors.push(`duplicate source_bank_transaction_id ${bankTransactionId}: used by ${remaining.length} active entries (${remaining.map((entry) => entry.entry_no).join(', ')})`);
    }
  }

  const invoiceEvidence = validateInvoiceJournalEvidence(db);
  errors.push(...invoiceEvidence.errors);

  const issuedInvoices = db.query("SELECT id, invoice_no, status FROM documents WHERE document_type = 'issued_invoice' ORDER BY id ASC").all() as Array<{ id: number; invoice_no: string | null; status: string | null }>;
  const allowedStoredStatus: Record<string, string[]> = {
    open: ["issued", "open"],
    paid: ["paid", "settled"],
    credited: ["credited"],
    refunded: ["refunded"],
    overpaid: ["overpaid"],
    written_off: ["written_off"],
  };
  for (const invoice of issuedInvoices) {
    const status = getInvoiceStatus(db, invoice.id);
    if (!status.ok) {
      errors.push(`invoice ${invoice.invoice_no ?? invoice.id}: status cross-check failed (${status.errors.join('; ')})`);
      continue;
    }
    const stored = invoice.status ?? "";
    if (stored === "issued") continue;
    const allowed = allowedStoredStatus[status.status ?? ""] ?? [status.status ?? ""];
    if (!allowed.includes(stored)) {
      errors.push(`invoice ${invoice.invoice_no ?? invoice.id}: stored status ${stored} does not match ledger status ${status.status} (open balance ${status.openBalance})`);
    }
  }

  // KODE-14: defence-in-depth audit_log tamper-evidence. The audit_log has no
  // hash chain of its own, so a removed row would otherwise be invisible here.
  // Fold in the id-gap detection (a hole means a middle row was deleted). The
  // stronger journal cross-check is left to the standalone verifyAuditLog-
  // Integrity caller: it assumes every entry was posted through the normal
  // path, which manually-inserted entries (migrations, fixtures) need not be.
  const auditLogIntegrity = verifyAuditLogIntegrity(db, { journalCrossCheck: false });
  errors.push(...auditLogIntegrity.errors);

  return { ok: errors.length === 0, entries: entries.length, errors };
}
