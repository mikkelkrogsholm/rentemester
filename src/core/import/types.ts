// Import framework — normalised intermediate representation + parser contract.
// Issue #185.
//
// A business adopting Rentemester from another accounting system (e-conomic,
// Billy, Dinero #173, ...) needs its history brought over. Rather than an
// ad-hoc parser per system, the import framework defines:
//
//  1. A NORMALISED INTERMEDIATE REPRESENTATION (`ImportSource`) — a
//     system-agnostic description of what was exported: the chart of accounts,
//     the per-account opening balances, and optionally historical entries.
//  2. A PARSER CONTRACT (`SourceParser`) — the small interface every
//     per-system parser implements. A parser's only job is to turn a raw
//     export (CSV/XML/JSON text) into an `ImportSource`. It performs NO
//     posting; the framework owns the mapping onto the ledger.
//
// The framework (`runImport`, see ./framework.ts) maps an `ImportSource` onto
// the #179 primobalance target by calling `postOpeningBalance` — it never
// reimplements opening-balance logic. Money is integer øre throughout, exactly
// as the rest of Rentemester.

/**
 * The Rentemester account types — mirrors the `accounts.type` CHECK constraint
 * in src/core/schema.sql. Importers may emit `vat` only from explicit
 * source-system control-account evidence, never from a sales/purchase base
 * account's default VAT code.
 */
export type ImportAccountType = "asset" | "liability" | "equity" | "income" | "expense" | "vat";

/** A Rentemester `normal_balance` value (mirrors the schema CHECK constraint). */
export type ImportNormalBalance = "debit" | "credit";

/**
 * One account in the source system's chart of accounts. Carried so the
 * framework can validate that every opening-balance line references a known
 * account and report unknown accounts clearly.
 *
 * A parser that has classified the account (the Dinero parser does) also fills
 * `normalizedType` / `normalBalance` / `defaultVatCode`, so the chart can be
 * reconciled into the live `accounts` table without re-deriving the mapping.
 */
export type ImportAccount = {
  accountNo: string;
  name: string;
  /** Optional source-system account type label, kept for the audit trail. */
  type?: string;
  /** Rentemester account type, when the parser has classified the account. */
  normalizedType?: ImportAccountType;
  /** Rentemester normal balance, when the parser has classified the account. */
  normalBalance?: ImportNormalBalance;
  /** Resolved Rentemester VAT code (`accounts.default_vat_code`), when mapped. */
  defaultVatCode?: string | null;
};

/**
 * The company master data carried by an export (Dinero's `Firmaoplysninger.csv`).
 * Reconciled into the `companies` row by the import flow; fields the schema
 * does not store (address, phone, ...) are kept for the audit trail.
 */
export type ImportCompanyMasterData = {
  name?: string;
  cvr?: string;
  address?: string;
  postalCode?: string;
  city?: string;
  country?: string;
  email?: string;
  phone?: string;
  website?: string;
};

/**
 * One per-account opening balance as exported by the source system. Exactly
 * one of `debitAmount` / `creditAmount` is expected (integer øre); a line
 * carrying both is rejected by the framework.
 */
export type ImportOpeningBalanceLine = {
  accountNo: string;
  debitAmount?: number;
  creditAmount?: number;
  /** Optional per-line text; defaults to the account name when omitted. */
  text?: string;
  /** Optional source-system VAT code/label carried for the audit trail. */
  vatCode?: string;
};

/**
 * A historical journal entry from the source system — one voucher's worth of
 * activity AFTER the cut-over date. Posted by `runImport` (#195) as a balanced
 * journal entry, marked as an imported migration posting.
 *
 * `voucherRef` is the source system's voucher (Dinero `Bilag`) number — the
 * stable handle #196 uses to link an ingested receipt to its journal entry.
 * `entryType` carries the source classification (Dinero `Bilagstype`).
 */
export type ImportHistoricalEntry = {
  transactionDate: string;
  text: string;
  lines: ImportOpeningBalanceLine[];
  /** Source-system voucher number (Dinero `Bilag`); links receipts in #196. */
  voucherRef?: string;
  /** Source-system entry classification (Dinero `Bilagstype`). */
  entryType?: string;
};

/**
 * An aggregate debtor/creditor control balance carried by the source without
 * a structured item schedule. It is evidence only: the framework must never
 * turn it into a native invoice, payable, customer, vendor, or journal.
 */
export type ImportOpenItemControlBalance = {
  accountNo: string;
  kind: "receivable" | "payable";
  /** Unsigned balance in kroner. */
  amount: number;
  /** Export-relative source reference, e.g. `2026/SaldoBalance.csv`. */
  sourceReference: string;
};

/**
 * The normalised intermediate representation. This is the single hand-off
 * point between a per-system parser and the framework: parsers produce it,
 * the framework consumes it.
 */
export type ImportSource = {
  /** Identifier of the source system, e.g. "synthetic-csv", "dinero". */
  sourceSystem: string;
  /** Cut-over date (YYYY-MM-DD) — the day the opening balances are as of. */
  cutOverDate: string;
  /** The source system's chart of accounts. */
  chartOfAccounts: ImportAccount[];
  /** Explainable, unconfirmed semantic role candidates from chart evidence. */
  accountRoleProposals?: Array<{ role: import("../account-roles").AccountRole; accountNo: string; source: string }>;
  /** Per-account opening balances (the primobalance to be posted). */
  openingBalances: ImportOpeningBalanceLine[];
  /** Optional historical entries — captured but not posted by `runImport`. */
  historicalEntries?: ImportHistoricalEntry[];
  /** Aggregate open-item controls when the source has no item-level schedule. */
  openItemControlBalances?: ImportOpenItemControlBalance[];
  /** Explicit, versioned item-level receivable schedule. Never inferred from postings. */
  importedReceivableSchedule?: import("../imported-receivables").ImportedReceivableSchedule;
  /** Optional company master data (name, CVR, ...) from the export. */
  companyMasterData?: ImportCompanyMasterData;
  /**
   * Optional list of source VAT-code labels the parser could NOT map onto a
   * Rentemester VAT code. Surfaced by the import flow — never silently dropped.
   */
  unmappedVatCodes?: string[];
  /** Optional free-text note carried into the primobalance entry. */
  note?: string;
};

/**
 * The result of a parser turning a raw export into an `ImportSource`.
 * `ok === false` carries the reason in `errors` and omits `source`.
 */
export type ParseResult = {
  ok: boolean;
  source?: ImportSource;
  errors: string[];
};

/**
 * One artifact (file) inside a multi-file export. CSV/text artifacts carry a
 * BOM-stripped UTF-8 `text` decode; binary artifacts (receipt images, ...) are
 * exposed as `path`/`bytes` for downstream ingest.
 */
export type ImportArtifact = {
  /** Export-root-relative, forward-slash file name, e.g. "2025/Kontoplan.csv". */
  name: string;
  /** Absolute path on disk. */
  path: string;
  /** Raw file bytes. */
  bytes: Uint8Array;
  /** BOM-stripped UTF-8 decode of `bytes`. */
  text: string;
};

/**
 * A resolved multi-file export: a root directory plus its artifacts keyed by
 * export-root-relative, forward-slash name. Produced by `resolveSource`
 * (./source.ts) and consumed by a `SourceParser.parseSource`.
 */
export type MultiArtifactSource = {
  rootDir: string;
  files: Record<string, ImportArtifact>;
  /** Immutable, canonical evidence for the exact resolved input. */
  sourceEvidence: SourceEvidence;
  /**
   * Present for a ZIP source after its complete archive listing and extraction
   * have been verified. The three counts must agree before parsing can begin.
   */
  archiveIntegrity?: ArchiveIntegrityEvidence;
};

/** Deterministic evidence that a ZIP export was completely extracted. */
export type ArchiveIntegrityEvidence = {
  archiveSha256: string;
  archiveEntryCount: number;
  extractedEntryCount: number;
  importedEntryCount: number;
  archiveListingSha256: string;
  extractedManifestSha256: string;
};

/** One canonical file record in a resolved import source. */
export type SourceEvidenceEntry = {
  path: string;
  size: number;
  sha256: string;
};

/**
 * Immutable evidence for a resolved import source. ZIP raw bytes are recorded
 * only for ZIP inputs; directory and file inputs deliberately expose only
 * their canonical file inventory.
 */
export type SourceEvidence = {
  sourceKind: "zip" | "directory" | "file";
  canonicalInventorySha256: string;
  entries: SourceEvidenceEntry[];
  importedEntryCount: number;
  totalUncompressedBytes: number;
  /** Present only for a ZIP's private immutable snapshot. */
  rawSha256?: string;
  /** Present only for a ZIP's private immutable snapshot. */
  rawSize?: number;
  /** Present only for ZIP file entries after strict path normalization. */
  canonicalListingSha256?: string;
  /** Present only for ZIP file entries after strict path normalization. */
  listingEntryCount?: number;
  /** Present only for ZIP sources. */
  extractedEntryCount?: number;
};

/**
 * THE PARSER CONTRACT.
 *
 * Every per-system importer implements this interface. A parser is
 * intentionally small: it converts a raw export into a normalised
 * `ImportSource` and nothing else. All ledger interaction, validation against
 * the live chart of accounts, and posting is owned by the import flow.
 *
 * Two shapes are supported, both optional so a parser implements exactly the
 * one its format needs:
 *  - `parse(raw)` — a single text file (the synthetic-CSV example).
 *  - `parseSource(input)` + `requiredFiles` — a multi-file export, e.g. a
 *    Dinero export directory. The framework resolves the directory/zip into a
 *    `MultiArtifactSource` and checks `requiredFiles` are present first.
 */
export type SourceParser = {
  /** Stable identifier, e.g. "synthetic-csv". Mirrors `ImportSource.sourceSystem`. */
  readonly system: string;
  /** Human-readable label for help text / audit trail. */
  readonly label: string;
  /**
   * Parse a raw export (file contents as a string) into an `ImportSource`.
   * Pure and deterministic: the same input always yields the same output.
   */
  parse?(raw: string): ParseResult;
  /**
   * Export-root-relative, forward-slash file names a multi-file parser needs.
   * Missing required files are a clear, named error before `parseSource` runs.
   */
  readonly requiredFiles?: readonly string[];
  /**
   * Parse a resolved multi-file export into an `ImportSource`. Pure and
   * deterministic with respect to `input`.
   */
  parseSource?(input: MultiArtifactSource): ParseResult;
};

/** Options accepted by `runImport`. */
export type ImportOptions = {
  createdBy?: string;
  createdByProgram?: string;
  /** Execute the complete DB path inside a transaction and always roll it back. */
  dryRun?: boolean;
  /**
   * The company root directory — where `ingestDocument` stores receipt
   * originals (#196). When omitted, `runImportFromSource` derives it from the
   * open database's path. Pass it explicitly for an in-memory ledger.
   */
  companyRoot?: string;
};

/**
 * The outcome of reconciling a source chart of accounts into the live
 * `accounts` table. Missing accounts are created; an existing account that
 * carries no journal lines yet is RECLASSIFIED to the source's definition
 * (the source chart is authoritative for a system migration); an existing
 * account that already has journal lines must NOT be reclassified and is
 * reported as a `conflict`. Deterministic and audited — see
 * `reconcileChartOfAccounts`.
 */
export type ChartReconciliationResult = {
  /** Account numbers created in the `accounts` table by this import. */
  created: string[];
  /** Account numbers that already existed and were left untouched. */
  existing: string[];
  /** Account numbers reclassified to the source definition (had no postings). */
  updated: string[];
  /**
   * Human-readable differences applied: each describes how an existing
   * postings-free account was reclassified to match the source.
   */
  differences: string[];
  /**
   * Account numbers that differ from the source but already carry journal
   * lines, so they could NOT be reclassified — human-readable conflict lines.
   */
  conflicts: string[];
  /** Source VAT-code labels that could not be mapped to a Rentemester code. */
  unmappedVatCodes: string[];
};

/**
 * The outcome of reconciling company master data into the `companies` row.
 * A non-empty field is never overwritten unless `overwrite` was requested.
 */
export type CompanyReconciliationResult = {
  /** `companies` column names that were populated by this import. */
  updatedFields: string[];
  /** Human-readable notes — e.g. a field skipped because it was already set. */
  notes: string[];
};

/**
 * The outcome of an import. On success it carries the primobalance entry
 * identifiers plus a deterministic, ordered audit trail describing every step
 * the framework took, so the migration is fully traceable.
 */
export type ImportResult = {
  ok: boolean;
  /** True when all database mutations were deliberately rolled back. */
  dryRun?: boolean;
  sourceSystem: string;
  cutOverDate?: string;
  entryId?: number;
  entryNo?: string;
  entryHash?: string;
  openingBalanceLineCount: number;
  /** Count of historical entries seen but intentionally not posted. */
  historicalEntriesSkipped: number;
  /**
   * Year-to-date vouchers (#195) replayed as journal entries after the
   * primobalance. Each carries the source voucher reference (Dinero `Bilag`)
   * and the journal entry it landed as — the handle #196 uses to link a
   * receipt to its voucher's journal entry.
   */
  historicalEntriesPosted?: Array<{
    voucherRef: string;
    entryId: number;
    entryNo: string;
    transactionDate: string;
  }>;
  /** Chart-of-accounts reconciliation outcome, when the source carried one. */
  chart?: ChartReconciliationResult;
  /** Company-master-data reconciliation outcome, when the source carried one. */
  company?: CompanyReconciliationResult;
  /** Pure importer suggestions shown for explicit review; never confirmations. */
  accountRoleProposals?: ImportSource["accountRoleProposals"];
  /**
   * Bilag (receipt) ingest outcome (#196), when the export shipped receipts:
   * how many were ingested and linked to their voucher's journal entry, and
   * how many unbooked receipts were flagged in the exception queue.
   */
  bilag?: {
    linkedCount: number;
    unmatchedCount: number;
    duplicateCount: number;
    unbookedCount: number;
  };
  /** Aggregate balances preserved without claiming item-level allocation. */
  migrationOpenItems?: {
    batchCount: number;
    receivableAmount: number;
    payableAmount: number;
  };
  /** ZIP archive evidence, when the source was a verified ZIP export. */
  archiveIntegrity?: ArchiveIntegrityEvidence;
  /** Ordered, deterministic human-readable description of what happened. */
  auditTrail: string[];
  appliedRules: string[];
  errors: string[];
};
