import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { basename, join, relative } from "node:path";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { effectiveRetainUntil } from "./retention";
import { writeFileAtomic } from "./atomic-file";

const RULE_ID = "DK-BOOKKEEPING-AUTHORITY-EXPORT-001";
const FOUR_WEEKS_MS = 28 * 24 * 60 * 60 * 1000;

type CanonicalJson = null | boolean | number | string | CanonicalJson[] | { [key: string]: CanonicalJson };

type ExportedFileMeta = {
  path: string;
  sha256: string;
  sizeBytes: number;
};

export type ExportAuthorityPackageInput = {
  periodStart: string;
  periodEnd: string;
  outputDir: string;
  requestedAt?: string;
  requester?: string;
  generatedAt?: string;
  packageProfile?: "authority" | "accountant_handoff";
};

export type ExportAuthorityPackageResult = {
  ok: boolean;
  exportDir?: string;
  manifestPath?: string;
  periodStart?: string;
  periodEnd?: string;
  generatedAt?: string;
  deadlineAt?: string | null;
  journalEntryCount?: number;
  bankTransactionCount?: number;
  documentCount?: number;
  appliedRules: string[];
  errors: string[];
};

type JournalLineRecord = {
  accountNo: string;
  accountName: string;
  debitAmount: number;
  creditAmount: number;
  vatCode: string | null;
  text: string | null;
};

type JournalEntryRecord = {
  id: number;
  entryNo: string;
  transactionDate: string;
  registrationDatetime: string;
  text: string;
  documentId: number | null;
  sourceBankTransactionId: number | null;
  currency: string;
  amountForeign: number | null;
  amountDkk: number | null;
  fxRateToDkk: number | null;
  status: string;
  reversalOfEntryId: number | null;
  createdBy: string;
  createdByProgram: string;
  retainUntil: string | null;
  lines: JournalLineRecord[];
};

type DocumentRecord = {
  id: number;
  documentNo: string | null;
  documentType: string;
  invoiceNo: string | null;
  invoiceDate: string | null;
  originalFilename: string | null;
  storedPath: string | null;
  mimeType: string | null;
  source: string;
  amountIncVat: number | null;
  vatAmount: number | null;
  status: string;
  retainUntil: string | null;
};

type BankTransactionRecord = {
  id: number;
  transactionDate: string;
  bookingDate: string | null;
  text: string;
  amount: number;
  currency: string;
  amountDkk: number | null;
  fxRateToDkk: number | null;
  reference: string | null;
  importBatchId: string | null;
  status: string;
  retainUntil: string | null;
};

type AuditLogRecord = {
  id: number;
  eventType: string;
  entityType: string;
  entityId: string | null;
  message: string;
  actor: string;
  createdAt: string;
};

type ExceptionRecord = {
  id: number;
  type: string;
  severity: string;
  status: string;
  relatedBankTransactionId: number | null;
  relatedDocumentId: number | null;
  message: string;
  requiredAction: string | null;
  createdAt: string;
  resolvedAt: string | null;
  resolvedBy: string | null;
  resolutionNote: string | null;
};

type AccountRecord = {
  id: number;
  accountNo: string;
  name: string;
  type: string;
  normalBalance: string;
  active: boolean;
  defaultVatCode: string | null;
  allowDirectPosting: boolean;
};

type CompanyRecord = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: string;
  createdAt: string;
};

type SchemaMigrationRecord = {
  id: number;
  name: string;
  appliedAt: string;
};

type ExportProfile = {
  packageType: "authority_export" | "accountant_handoff_export";
  auditEventType: "authority_export" | "accountant_handoff_export";
  readmeTitle: string;
  readmeScopeLines: string[];
  manifestExtras?: Record<string, unknown>;
};

function resolveExportProfile(profile: ExportAuthorityPackageInput["packageProfile"]): ExportProfile {
  if (profile === "accountant_handoff") {
    return {
      packageType: "accountant_handoff_export",
      auditEventType: "accountant_handoff_export",
      readmeTitle: "Rentemester — revisor-eksportpakke",
      readmeScopeLines: [
        "Primær overdragelsesmodel: lokal eksportpakke (local export package).",
        "Adgangsmodel: ingen direkte adgang til virksomhedens kørende data (no runtime access).",
        "Uden for omfang: hosted revisor-/bogholder-adgang, rolle-baseret skriveadgang og samarbejde i realtid (out of scope: hosted reviewer/accountant access, role-based write access, real-time collaboration).",
      ],
      manifestExtras: {
        handoffModel: "local_export_package",
        accessModel: "no_runtime_access",
        outOfScope: [
          "hosted_multi_user_access",
          "role_based_write_access",
          "real_time_collaboration",
        ],
      },
    };
  }
  return {
    packageType: "authority_export",
    auditEventType: "authority_export",
    readmeTitle: "Rentemester — myndighedseksportpakke",
    readmeScopeLines: [],
  };
}

function resolveIsoDateTime(value?: string) {
  if (!value) return null;
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return null;
  return new Date(parsed).toISOString();
}

function normalizeExportTimestamp(periodEnd: string) {
  return `${periodEnd}T23:59:59.000Z`;
}

function packageName(
  periodStart: string,
  periodEnd: string,
  generatedAt: string,
  profile?: ExportAuthorityPackageInput["packageProfile"],
) {
  // Per round-2 review: the accountant profile previously produced a
  // directory named `authority-export-…`, which a Danish revisor opening
  // the .tar saw as "myndighedseksport" — wrong recipient label. Use a
  // recipient-correct Danish prefix per profile so the directory name
  // alone tells you who the bundle is for.
  const prefix =
    profile === "accountant_handoff" ? "revisor-eksport" : "myndigheds-eksport";
  const stamp = generatedAt.replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  return `${prefix}-${periodStart}_${periodEnd}_${stamp}`;
}

function canonicalize(value: unknown): CanonicalJson {
  if (value === null || typeof value === "boolean" || typeof value === "number" || typeof value === "string") return value;
  if (Array.isArray(value)) return value.map((entry) => canonicalize(entry));
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, entry]) => entry !== undefined)
      .sort(([a], [b]) => a.localeCompare(b));
    return Object.fromEntries(entries.map(([key, entry]) => [key, canonicalize(entry)]));
  }
  return String(value);
}

function stringifyCanonicalJson(value: unknown) {
  return `${JSON.stringify(canonicalize(value), null, 2)}\n`;
}

function sha256Text(text: string) {
  return createHash("sha256").update(text).digest("hex");
}

function uniqueIds(values: Array<number | null | undefined>) {
  return [...new Set(values.filter((value): value is number => Number.isInteger(value) && value > 0))].sort((a, b) => a - b);
}

function exportFileName(document: DocumentRecord) {
  const base = document.documentNo ?? `document-${document.id}`;
  const original = document.originalFilename ?? basename(document.storedPath ?? `document-${document.id}`);
  return `${base}-${original}`.replace(/[^A-Za-z0-9._-]/g, "_");
}

function packageRelativePath(exportDir: string, path: string) {
  return relative(exportDir, path).replace(/\\/g, "/");
}

function storedPathRelativeToCompany(companyRoot: string, storedPath: string | null) {
  if (!storedPath) return null;
  const rel = relative(companyRoot, storedPath).replace(/\\/g, "/");
  return rel.startsWith("../") ? storedPath : rel;
}

function writeExportJson(exportDir: string, path: string, value: unknown, outputs: ExportedFileMeta[]) {
  const body = stringifyCanonicalJson(value);
  // Atomic write (issue #151): stage to an exclusively-created temp file and
  // rename into place so a crash never leaves a partial JSON file.
  writeFileAtomic(path, body);
  outputs.push({
    path: packageRelativePath(exportDir, path),
    sha256: sha256Text(body),
    sizeBytes: Buffer.byteLength(body),
  });
}

function recordExistingFile(exportDir: string, path: string, outputs: ExportedFileMeta[]) {
  const body = readFileSync(path);
  outputs.push({
    path: packageRelativePath(exportDir, path),
    sha256: createHash("sha256").update(body).digest("hex"),
    sizeBytes: statSync(path).size,
  });
}

function fetchJournalEntries(db: Database, periodStart: string, periodEnd: string): JournalEntryRecord[] {
  const entries = db.query(
    `SELECT id, entry_no, transaction_date, registration_datetime, text, document_id, source_bank_transaction_id,
            currency, amount_foreign, amount_dkk, fx_rate_to_dkk,
            status, reversal_of_entry_id, created_by, created_by_program, retain_until
     FROM journal_entries
     WHERE transaction_date BETWEEN ? AND ?
     ORDER BY transaction_date ASC, id ASC`
  ).all(periodStart, periodEnd) as any[];

  const linesStmt = db.query(
    `SELECT a.account_no, a.name AS account_name, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
     FROM journal_lines jl
     JOIN accounts a ON a.id = jl.account_id
     WHERE jl.journal_entry_id = ?
     ORDER BY jl.id ASC`
  );

  return entries.map((entry) => ({
    id: entry.id,
    entryNo: entry.entry_no,
    transactionDate: entry.transaction_date,
    registrationDatetime: entry.registration_datetime,
    text: entry.text,
    documentId: entry.document_id ?? null,
    sourceBankTransactionId: entry.source_bank_transaction_id ?? null,
    currency: entry.currency,
    amountForeign: entry.amount_foreign == null ? null : Number(entry.amount_foreign),
    amountDkk: entry.amount_dkk == null ? null : Number(entry.amount_dkk),
    fxRateToDkk: entry.fx_rate_to_dkk == null ? null : Number(entry.fx_rate_to_dkk),
    status: entry.status,
    reversalOfEntryId: entry.reversal_of_entry_id ?? null,
    createdBy: entry.created_by,
    createdByProgram: entry.created_by_program,
    retainUntil: effectiveRetainUntil(db, entry.retain_until, entry.transaction_date),
    lines: (linesStmt.all(entry.id) as any[]).map((line) => ({
      accountNo: line.account_no,
      accountName: line.account_name,
      debitAmount: Number(line.debit_amount),
      creditAmount: Number(line.credit_amount),
      vatCode: line.vat_code ?? null,
      text: line.text ?? null,
    })),
  }));
}

function fetchDocuments(db: Database, journalEntries: JournalEntryRecord[], periodStart: string, periodEnd: string): DocumentRecord[] {
  const linkedIds = uniqueIds(journalEntries.map((entry) => entry.documentId));
  const linkedDocuments = linkedIds.length === 0 ? [] : db.query(
    `SELECT id, document_no, document_type, invoice_no, invoice_date, original_filename, stored_path, mime_type, source,
            amount_inc_vat, vat_amount, status, retain_until
     FROM documents WHERE id IN (${linkedIds.map(() => "?").join(",")})`
  ).all(...linkedIds) as any[];

  const issuedInPeriod = db.query(
    `SELECT id, document_no, document_type, invoice_no, invoice_date, original_filename, stored_path, mime_type, source,
            amount_inc_vat, vat_amount, status, retain_until
     FROM documents
     WHERE invoice_date BETWEEN ? AND ?
     ORDER BY id ASC`
  ).all(periodStart, periodEnd) as any[];

  const dedup = new Map<number, DocumentRecord>();
  for (const row of [...linkedDocuments, ...issuedInPeriod]) {
    dedup.set(row.id, {
      id: row.id,
      documentNo: row.document_no ?? null,
      documentType: row.document_type,
      invoiceNo: row.invoice_no ?? null,
      invoiceDate: row.invoice_date ?? null,
      originalFilename: row.original_filename ?? null,
      storedPath: row.stored_path ?? null,
      mimeType: row.mime_type ?? null,
      source: row.source,
      amountIncVat: row.amount_inc_vat == null ? null : Number(row.amount_inc_vat),
      vatAmount: row.vat_amount == null ? null : Number(row.vat_amount),
      status: row.status,
      retainUntil: effectiveRetainUntil(db, row.retain_until, row.invoice_date ?? null),
    });
  }
  return [...dedup.values()].sort((a, b) => a.id - b.id);
}

function fetchBankTransactions(db: Database, journalEntries: JournalEntryRecord[], periodStart: string, periodEnd: string): BankTransactionRecord[] {
  const linkedIds = uniqueIds(journalEntries.map((entry) => entry.sourceBankTransactionId));
  const linked = linkedIds.length === 0 ? [] : db.query(
    `SELECT id, transaction_date, booking_date, text, amount, currency, amount_dkk, fx_rate_to_dkk, reference, import_batch_id, status, retain_until
     FROM bank_transactions WHERE id IN (${linkedIds.map(() => "?").join(",")})`
  ).all(...linkedIds) as any[];
  const inPeriod = db.query(
    `SELECT id, transaction_date, booking_date, text, amount, currency, amount_dkk, fx_rate_to_dkk, reference, import_batch_id, status, retain_until
     FROM bank_transactions
     WHERE COALESCE(booking_date, transaction_date) BETWEEN ? AND ?
     ORDER BY COALESCE(booking_date, transaction_date) ASC, id ASC`
  ).all(periodStart, periodEnd) as any[];

  const dedup = new Map<number, BankTransactionRecord>();
  for (const row of [...linked, ...inPeriod]) {
    dedup.set(row.id, {
      id: row.id,
      transactionDate: row.transaction_date,
      bookingDate: row.booking_date ?? null,
      text: row.text,
      amount: Number(row.amount),
      currency: row.currency,
      amountDkk: row.amount_dkk == null ? null : Number(row.amount_dkk),
      fxRateToDkk: row.fx_rate_to_dkk == null ? null : Number(row.fx_rate_to_dkk),
      reference: row.reference ?? null,
      importBatchId: row.import_batch_id ?? null,
      status: row.status,
      retainUntil: effectiveRetainUntil(db, row.retain_until, row.booking_date ?? row.transaction_date ?? null),
    });
  }
  return [...dedup.values()].sort((a, b) => a.id - b.id);
}

function fetchAuditLog(db: Database, periodStart: string, periodEnd: string): AuditLogRecord[] {
  return db.query(
    `SELECT id, event_type, entity_type, entity_id, message, actor, created_at
     FROM audit_log
     WHERE created_at >= ? AND created_at <= ?
     ORDER BY id ASC`
  ).all(`${periodStart} 00:00:00`, `${periodEnd} 23:59:59`) as AuditLogRecord[];
}

function fetchExceptions(db: Database, periodStart: string, periodEnd: string): ExceptionRecord[] {
  return db.query(
    `SELECT id, type, severity, status, related_bank_transaction_id, related_document_id,
            message, required_action, created_at, resolved_at, resolved_by, resolution_note
     FROM exceptions
     WHERE (created_at >= ? AND created_at <= ?)
        OR (status = 'open' AND created_at < ?)
     ORDER BY id ASC`
  ).all(`${periodStart} 00:00:00`, `${periodEnd} 23:59:59`, `${periodStart} 00:00:00`) as ExceptionRecord[];
}

function fetchAccounts(db: Database): AccountRecord[] {
  return (db.query(
    `SELECT id, account_no, name, type, normal_balance, active, default_vat_code, allow_direct_posting
     FROM accounts ORDER BY account_no ASC`
  ).all() as any[]).map((row) => ({
    id: row.id,
    accountNo: row.account_no,
    name: row.name,
    type: row.type,
    normalBalance: row.normal_balance,
    active: !!row.active,
    defaultVatCode: row.default_vat_code ?? null,
    allowDirectPosting: !!row.allow_direct_posting,
  }));
}

function fetchCompanies(db: Database): CompanyRecord[] {
  return (db.query(
    `SELECT id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy, created_at FROM companies ORDER BY id ASC`
  ).all() as any[]).map((row) => ({
    id: row.id,
    name: row.name,
    country: row.country,
    currency: row.currency,
    cvr: row.cvr ?? null,
    fiscalYearStartMonth: row.fiscal_year_start_month,
    fiscalYearLabelStrategy: row.fiscal_year_label_strategy,
    createdAt: row.created_at,
  }));
}

function fetchSchemaMigrations(db: Database): SchemaMigrationRecord[] {
  return (db.query(
    `SELECT id, name, applied_at FROM schema_migrations ORDER BY id ASC`
  ).all() as any[]).map((row) => ({
    id: row.id,
    name: row.name,
    appliedAt: row.applied_at,
  }));
}

function buildExportReadme(input: {
  title: string;
  scopeLines: string[];
  periodStart: string;
  periodEnd: string;
  requester: string | null;
  requestedAt: string | null;
  generatedAt: string;
  deadlineAt: string | null;
}) {
  // Frame text on Danish (revisor / SKAT læser pakken). Field-key og
  // file-stier holdes engelske fordi de mirror'er manifest.json's JSON-felter
  // og er stabile mod tværsprogede consumers (test-scripts, agent-tools).
  return [
    input.title,
    "",
    `Periode: ${input.periodStart}..${input.periodEnd}`,
    `Rekvirent: ${input.requester ?? "ikke angivet"}`,
    `Anmodet: ${input.requestedAt ?? "ikke angivet"}`,
    `Genereret: ${input.generatedAt}`,
    `Frist: ${input.deadlineAt ?? "ikke relevant"}`,
    ...(input.scopeLines.length > 0 ? ["", ...input.scopeLines] : []),
    "",
    "Filer i pakken:",
    "- machine-readable/journal-entries.json — finansposteringer (alle linjer) i perioden",
    "- machine-readable/documents.json — knyttede eller udstedte bilag plus stier til de eksporterede læselige filer",
    "- machine-readable/bank-transactions.json — knyttede eller periodefiltrerede banktransaktioner",
    "- machine-readable/audit-log.json — revisionsspor (audit-events) i perioden",
    "- machine-readable/exceptions.json — undtagelser i perioden plus tidligere stadig-åbne undtagelser",
    "- machine-readable/accounts.json — fuld kontoplan-kontekst",
    "- machine-readable/companies.json — virksomheds-stamdata",
    "- machine-readable/schema-migrations.json — anvendte schema-migrationer",
    "- documents-readable/* — kopierede læselige bilagsfiler inkluderet i pakken",
    "- manifest.json — pakke-metadata plus hashes for hver outputfil",
    "",
  ].join("\n");
}

export function exportAuthorityPackage(db: Database, companyRoot: string, input: ExportAuthorityPackageInput): ExportAuthorityPackageResult {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(input.periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (typeof input.outputDir !== "string" || input.outputDir.trim().length === 0) errors.push("outputDir is required");
  if (errors.length === 0 && input.periodStart > input.periodEnd) errors.push("periodStart cannot be after periodEnd");
  const requestedAt = resolveIsoDateTime(input.requestedAt);
  if (input.requestedAt && !requestedAt) errors.push("requestedAt must be a valid ISO-8601 datetime when provided");
  const generatedAt = resolveIsoDateTime(input.generatedAt ?? input.requestedAt) ?? normalizeExportTimestamp(input.periodEnd);
  if (input.generatedAt && !resolveIsoDateTime(input.generatedAt)) errors.push("generatedAt must be a valid ISO-8601 datetime when provided");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const profile = resolveExportProfile(input.packageProfile);
  const deadlineAt = requestedAt ? new Date(new Date(requestedAt).getTime() + FOUR_WEEKS_MS).toISOString() : null;
  const exportDir = join(
    input.outputDir,
    packageName(input.periodStart, input.periodEnd, generatedAt, input.packageProfile),
  );
  const machineReadableDir = join(exportDir, "machine-readable");
  const documentsDir = join(exportDir, "documents-readable");
  mkdirSync(machineReadableDir, { recursive: true });
  mkdirSync(documentsDir, { recursive: true });

  const journalEntries = fetchJournalEntries(db, input.periodStart, input.periodEnd);
  const documents = fetchDocuments(db, journalEntries, input.periodStart, input.periodEnd);
  const bankTransactions = fetchBankTransactions(db, journalEntries, input.periodStart, input.periodEnd);
  const auditLog = fetchAuditLog(db, input.periodStart, input.periodEnd);
  const exceptions = fetchExceptions(db, input.periodStart, input.periodEnd);
  const accounts = fetchAccounts(db);
  const companies = fetchCompanies(db);
  const schemaMigrations = fetchSchemaMigrations(db);

  const outputs: ExportedFileMeta[] = [];
  writeExportJson(exportDir, join(machineReadableDir, "journal-entries.json"), journalEntries, outputs);
  writeExportJson(exportDir, join(machineReadableDir, "documents.json"), documents.map((document) => ({
    ...document,
    storedPathRelativeToCompany: storedPathRelativeToCompany(companyRoot, document.storedPath),
    exportedReadablePath: document.storedPath ? join("documents-readable", exportFileName(document)).replace(/\\/g, "/") : null,
  })), outputs);
  writeExportJson(exportDir, join(machineReadableDir, "bank-transactions.json"), bankTransactions, outputs);
  writeExportJson(exportDir, join(machineReadableDir, "audit-log.json"), auditLog.map((row: any) => ({
    id: row.id,
    eventType: row.event_type,
    entityType: row.entity_type,
    entityId: row.entity_id ?? null,
    message: row.message,
    actor: row.actor,
    createdAt: row.created_at,
  })), outputs);
  writeExportJson(exportDir, join(machineReadableDir, "exceptions.json"), exceptions.map((row: any) => ({
    id: row.id,
    type: row.type,
    severity: row.severity,
    status: row.status,
    relatedBankTransactionId: row.related_bank_transaction_id ?? null,
    relatedDocumentId: row.related_document_id ?? null,
    message: row.message,
    requiredAction: row.required_action ?? null,
    createdAt: row.created_at,
    resolvedAt: row.resolved_at ?? null,
    resolvedBy: row.resolved_by ?? null,
    resolutionNote: row.resolution_note ?? null,
  })), outputs);
  writeExportJson(exportDir, join(machineReadableDir, "accounts.json"), accounts, outputs);
  writeExportJson(exportDir, join(machineReadableDir, "companies.json"), companies, outputs);
  writeExportJson(exportDir, join(machineReadableDir, "schema-migrations.json"), schemaMigrations, outputs);

  const copiedDocuments: Array<{ documentId: number; sourcePathRelativeToCompany: string | null; exportedPath: string; sha256: string; sizeBytes: number }> = [];
  for (const document of documents) {
    if (!document.storedPath || !existsSync(document.storedPath)) continue;
    const target = join(documentsDir, exportFileName(document));
    copyFileSync(document.storedPath, target);
    recordExistingFile(exportDir, target, outputs);
    const output = outputs[outputs.length - 1]!;
    copiedDocuments.push({
      documentId: document.id,
      sourcePathRelativeToCompany: storedPathRelativeToCompany(companyRoot, document.storedPath),
      exportedPath: output.path,
      sha256: output.sha256,
      sizeBytes: output.sizeBytes,
    });
  }

  const readmePath = join(exportDir, "README.txt");
  writeFileAtomic(readmePath, buildExportReadme({
    title: profile.readmeTitle,
    scopeLines: profile.readmeScopeLines,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    requester: input.requester ?? null,
    requestedAt: requestedAt ?? null,
    generatedAt,
    deadlineAt,
  }));
  recordExistingFile(exportDir, readmePath, outputs);

  outputs.sort((a, b) => a.path.localeCompare(b.path));
  copiedDocuments.sort((a, b) => a.exportedPath.localeCompare(b.exportedPath));

  const manifest = {
    packageType: profile.packageType,
    generatedAt,
    requestedAt: requestedAt ?? null,
    deadlineAt,
    requester: input.requester ?? null,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    sourceCompanyRootName: basename(companyRoot),
    appliedRules: [RULE_ID],
    machineReadableFormat: "json",
    files: {
      journalEntries: "machine-readable/journal-entries.json",
      documents: "machine-readable/documents.json",
      bankTransactions: "machine-readable/bank-transactions.json",
      auditLog: "machine-readable/audit-log.json",
      exceptions: "machine-readable/exceptions.json",
      accounts: "machine-readable/accounts.json",
      companies: "machine-readable/companies.json",
      schemaMigrations: "machine-readable/schema-migrations.json",
      readableDocumentsDir: "documents-readable",
      readme: "README.txt",
    },
    counts: {
      journalEntries: journalEntries.length,
      bankTransactions: bankTransactions.length,
      documents: documents.length,
      auditLog: auditLog.length,
      exceptions: exceptions.length,
      accounts: accounts.length,
      companies: companies.length,
      schemaMigrations: schemaMigrations.length,
      copiedReadableDocuments: copiedDocuments.length,
    },
    copiedDocuments,
    outputs,
    ...profile.manifestExtras,
  };
  const manifestPath = join(exportDir, "manifest.json");
  writeExportJson(exportDir, manifestPath, manifest, outputs);

  insertAuditLog(db, {
    eventType: profile.auditEventType,
    entityType: "company",
    entityId: 1,
    message: `Exported ${profile.packageType} for ${input.periodStart}..${input.periodEnd} to ${packageRelativePath(input.outputDir, exportDir)}${input.requester ? ` for ${input.requester}` : ""}`,
  });

  return {
    ok: true,
    exportDir,
    manifestPath,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    generatedAt,
    deadlineAt,
    journalEntryCount: journalEntries.length,
    bankTransactionCount: bankTransactions.length,
    documentCount: documents.length,
    appliedRules: [RULE_ID],
    errors: [],
  };
}
