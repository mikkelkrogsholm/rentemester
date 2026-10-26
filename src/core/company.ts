import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ensureCompanyDirs } from "./paths";
import { openDb, migrate } from "./db";
import { openCurrentLedgerReadOnly } from "./ledger-inspection";
import { seedAccounts } from "./ledger";
import { seedNativeAccountRoles } from "./account-roles";
import { insertAuditLog } from "./actor";
import {
  lookupCvrCompany,
  normalizeCvrNumber,
  type CvrCompanyInfo,
  type CvrLookupOptions,
} from "./cvr";
import {
  registerWorkspaceCompany,
  slugifyCompanyName,
  isValidSlug,
} from "./workspace";
import {
  normalizeVatPeriodType,
  DEFAULT_VAT_PERIOD_TYPE,
  type VatPeriodType,
} from "./periods";

export type FiscalYearLabelStrategy = "end-year" | "start-year" | "span";

/** True when the `companies` table carries the `vat_period_type` column. */
function hasVatPeriodColumn(db: Database): boolean {
  const cols = db.query("PRAGMA table_info(companies)").all() as Array<{ name: string }>;
  return cols.some((col) => col.name === "vat_period_type");
}

export type CompanySettings = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: FiscalYearLabelStrategy;
  /** CVR-register stamdata, last written by `company sync-cvr`. */
  address: string | null;
  postalCode: string | null;
  city: string | null;
  companyForm: string | null;
  industryCode: string | null;
  industryText: string | null;
  cvrStatus: string | null;
  auditWaived: boolean | null;
  /** ISO timestamp the CVR stamdata above was last synced; null when never. */
  cvrSyncedAt: string | null;
  /**
   * #221: the owner's own default payment terms — days from an invoice's issue
   * date to its due date. Captured once so every issued invoice inherits it.
   */
  paymentTermsDays: number;
  /**
   * #289: the VAT settlement cadence the company is registered for with SKAT
   * (`month` / `quarter` / `half-year`). Drives VAT period windows and their
   * filing deadlines. `null` means the company is NOT VAT-registered
   * (holding ApS, frivilligt momsfritaget, mikrovirksomhed under § 48); every
   * VAT-aware surface (dashboard, obligations, momsangivelse) gates on this.
   */
  vatPeriodType: VatPeriodType | null;
};

const DEFAULT_PAYMENT_TERMS_DAYS = 14;

const DEFAULT_COMPANY_SETTINGS: CompanySettings = {
  id: 1,
  name: "Rentemester company",
  country: "DK",
  currency: "DKK",
  cvr: null,
  fiscalYearStartMonth: 1,
  fiscalYearLabelStrategy: "end-year",
  address: null,
  postalCode: null,
  city: null,
  companyForm: null,
  industryCode: null,
  industryText: null,
  cvrStatus: null,
  auditWaived: null,
  cvrSyncedAt: null,
  paymentTermsDays: DEFAULT_PAYMENT_TERMS_DAYS,
  // Keep the module's eager default independent of the close/VAT graph.
  // `periods.ts` imports the close-readiness graph, which eventually reads
  // company settings.  The value is the same canonical default exported from
  // that module; using the literal here prevents an ESM initialization cycle
  // from observing its export before initialization.
  vatPeriodType: "quarter",
};

/**
 * #221: the company's own postal address rendered as a single line, built from
 * the stored `address` / `postalCode` / `city` columns. This is the seller
 * address that flows onto every issued invoice. Returns null when nothing is
 * stored so a partially-configured company still produces a valid invoice.
 */
export function companyAddressLine(settings: CompanySettings): string | null {
  const cityLine = [settings.postalCode, settings.city]
    .map((part) => part?.trim())
    .filter((part) => part && part.length > 0)
    .join(" ");
  const full = [settings.address?.trim(), cityLine]
    .filter((part) => part && part.length > 0)
    .join(", ");
  return full.length > 0 ? full : null;
}

/**
 * #221: the company's payment instructions for the customer-facing invoice,
 * sourced from the ledger's `bank_accounts` table. Deterministic: prefers the
 * lowest-id active account whose currency matches the invoice, then any
 * lowest-id active account. Returns undefined when no usable account is
 * configured so the invoice/PDF simply omits the payment block.
 */
export type CompanyPaymentDetails = {
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
};

export function resolveCompanyPaymentDetails(
  db: Database,
  currency = "DKK",
): CompanyPaymentDetails | undefined {
  let rows: Array<{
    bank_name: string | null;
    registration_no: string | null;
    account_no: string | null;
    iban: string | null;
    currency: string | null;
  }> = [];
  try {
    rows = db
      .query(
        `SELECT bank_name, registration_no, account_no, iban, currency
           FROM bank_accounts
          WHERE active = 1
          ORDER BY id ASC`,
      )
      .all() as typeof rows;
  } catch {
    // The table may not exist in very old ledgers; payment block is optional.
    return undefined;
  }
  if (rows.length === 0) return undefined;
  const wanted = currency.trim().toUpperCase();
  const match =
    rows.find((row) => (row.currency ?? "").trim().toUpperCase() === wanted) ?? rows[0];
  const details: CompanyPaymentDetails = {
    bankName: match.bank_name?.trim() || null,
    registrationNo: match.registration_no?.trim() || null,
    accountNo: match.account_no?.trim() || null,
    iban: match.iban?.trim() || null,
  };
  if (!details.bankName && !details.registrationNo && !details.accountNo && !details.iban) {
    return undefined;
  }
  return details;
}

export function normalizeCvr(value?: string | null): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  const stripped = trimmed.toUpperCase().replace(/^DK/, "");
  if (!/^\d{8}$/.test(stripped)) {
    throw new Error(`CVR must be 8 digits (optionally prefixed with DK), got: ${value}`);
  }
  return `DK${stripped}`;
}

export function normalizeFiscalYearStartMonth(value?: number | string | null) {
  const month = typeof value === "string" ? Number(value) : value;
  if (!Number.isInteger(month) || (month ?? 0) < 1 || (month ?? 0) > 12) return null;
  return month;
}

export function normalizeFiscalYearLabelStrategy(value?: string | null): FiscalYearLabelStrategy | null {
  if (value === "end-year" || value === "start-year" || value === "span") return value;
  return null;
}

/**
 * #221: validate the owner's default payment-terms-in-days input. Accepts an
 * integer 0..365 (the `companies.payment_terms_days` CHECK range). Returns null
 * on anything else so callers can surface a clear error.
 */
export function normalizePaymentTermsDays(value?: number | string | null): number | null {
  if (value === undefined || value === null || value === "") return null;
  const days = typeof value === "string" ? Number(value) : value;
  if (!Number.isInteger(days) || (days as number) < 0 || (days as number) > 365) return null;
  return days as number;
}

/**
 * Builds the default `policy.yaml`. #248: the allowlist behaves consistently —
 * a derived actor (OS username) and an explicit `--actor` with the same id are
 * held to the same rule. To make that usable out of the box, the person who
 * runs `init` is seeded into `actor_allowlist.users` (canonical `user:` form),
 * so they can mutate immediately whether or not they pass `--actor`.
 */
function buildDefaultPolicyYaml(onboardingActor?: string | null): string {
  const userLines: string[] = ["    - user:ejer"];
  const agentLines: string[] = ["    - agent:rentemester-bookkeeper"];
  const systemLines: string[] = ["    - system:rentemester"];
  const actor = onboardingActor?.trim();
  if (actor && /^(user|agent|system):\S.+$/.test(actor)) {
    const [kind] = actor.split(":", 1) as [string];
    const line = `    - ${actor}`;
    if (kind === "agent") {
      if (!agentLines.includes(line)) agentLines.push(line);
    } else if (kind === "system") {
      if (!systemLines.includes(line)) systemLines.push(line);
    } else if (!userLines.includes(line)) {
      userLines.push(line);
    }
  }
  return (
    "company_policy:\n" +
    "  country: DK\n" +
    "  currency: DKK\n" +
    "  allow_direct_sql_write: false\n" +
    "  block_if_uncertain: true\n" +
    "\n" +
    "# Aktører der må køre muterende kommandoer.\n" +
    "# Hver muterende kommando kræver en actor — enten et eksplicit --actor-flag\n" +
    "# eller en afledt actor (OS-brugernavn via USER/LOGNAME, eller en\n" +
    "# agent-miljøvariabel). Begge holdes op mod denne liste. Tilføj din egen\n" +
    "# bruger/agent under den rette sektion nedenfor (linjen '    - user:dit-navn').\n" +
    "actor_allowlist:\n" +
    "  users:\n" +
    userLines.join("\n") + "\n" +
    "  agents:\n" +
    agentLines.join("\n") + "\n" +
    "  systems:\n" +
    systemLines.join("\n") + "\n"
  );
}

/**
 * #221: the company's own payment details captured at `init` (or later via the
 * profile). When any field is set, `init`/`company add` creates a primary
 * `bank_accounts` row so the customer-facing invoice PDF always shows where to
 * pay. All fields are optional — a company can be initialised without them.
 */
export type CompanyPaymentInput = {
  bankName?: string | null;
  registrationNo?: string | null;
  accountNo?: string | null;
  iban?: string | null;
};

export type CompanyInitOptions = {
  /** Display name stored in the ledger's `companies` row (id = 1). */
  name?: string;
  cvr?: string | null;
  fiscalYearStartMonth?: number | string | null;
  fiscalYearLabelStrategy?: string | null;
  /** #221: the company's own postal address — the seller address on invoices. */
  address?: string | null;
  postalCode?: string | null;
  city?: string | null;
  /** #221: default payment terms in days (issue date -> due date). */
  paymentTermsDays?: number | string | null;
  /**
   * #289: the company's VAT settlement cadence (`month` / `quarter` /
   * `half-year`). Defaults to `quarter` when omitted.
   */
  vatPeriodType?: string | null;
  /** #221: payment details; when any field is set a bank account is created. */
  payment?: CompanyPaymentInput;
  /**
   * #248: the canonical actor id (user:.../agent:.../system:...) of whoever is
   * running onboarding — seeded into the `actor_allowlist` so the person who
   * runs `init` can immediately mutate, whether they pass `--actor` explicitly
   * or rely on the derived OS username. Keeps the allowlist consistent: the
   * derived actor and an explicit `--actor` with the same id behave the same.
   */
  onboardingActor?: string | null;
};

/** True when at least one payment-detail field carries real information. */
function hasPaymentInfo(payment?: CompanyPaymentInput): payment is CompanyPaymentInput {
  if (!payment) return false;
  return Boolean(
    payment.bankName?.trim() ||
      payment.registrationNo?.trim() ||
      payment.accountNo?.trim() ||
      payment.iban?.trim(),
  );
}

/**
 * #221: create the company's primary bank account from captured payment
 * details, if one does not already exist. Idempotent — a second call with the
 * same slug is a silent no-op so re-running `init` or editing the profile never
 * fails on the append-only `bank_accounts` guard.
 */
function upsertPrimaryBankAccount(db: Database, currency: string, payment: CompanyPaymentInput) {
  const existing = db
    .query("SELECT id FROM bank_accounts WHERE slug = 'primaer' LIMIT 1")
    .get() as { id: number } | null;
  if (existing) return;
  db.query(
    `INSERT INTO bank_accounts (slug, name, bank_name, registration_no, account_no, iban, currency)
     VALUES ('primaer', 'Driftskonto', ?, ?, ?, ?, ?)`,
  ).run(
    payment.bankName?.trim() || null,
    payment.registrationNo?.trim() || null,
    payment.accountNo?.trim() || null,
    payment.iban?.trim() || null,
    currency,
  );
}

export type CompanyInitResult = {
  companyRoot: string;
  dbPath: string;
};

/**
 * Initialises a Rentemester company *volume* at `companyRoot`:
 * creates the directory tree, opens + migrates the ledger DB, seeds the
 * standard chart of accounts, writes the company row and a default
 * `policy.yaml`, and records an `init` audit event.
 *
 * This is the single source of truth for company initialisation — `rentemester
 * init` (raw path), `createCompany` (workspace), and later the MCP tools and
 * the cockpit API all call this. It does NOT touch any workspace manifest.
 *
 * Throws on invalid fiscal-year input so callers can surface a clear error.
 */
export function initialiseCompanyVolume(
  companyRoot: string,
  options: CompanyInitOptions = {},
): CompanyInitResult {
  if (options.fiscalYearStartMonth !== undefined && options.fiscalYearStartMonth !== null) {
    if (normalizeFiscalYearStartMonth(options.fiscalYearStartMonth) === null) {
      throw new Error("fiscalYearStartMonth must be an integer between 1 and 12");
    }
  }
  if (options.fiscalYearLabelStrategy !== undefined && options.fiscalYearLabelStrategy !== null) {
    if (normalizeFiscalYearLabelStrategy(options.fiscalYearLabelStrategy) === null) {
      throw new Error(
        "fiscalYearLabelStrategy must be one of end-year, start-year, span",
      );
    }
  }
  if (options.paymentTermsDays !== undefined && options.paymentTermsDays !== null) {
    if (normalizePaymentTermsDays(options.paymentTermsDays) === null) {
      throw new Error("paymentTermsDays must be an integer between 0 and 365");
    }
  }
  if (options.vatPeriodType !== undefined && options.vatPeriodType !== null) {
    if (normalizeVatPeriodType(options.vatPeriodType) === null) {
      throw new Error("vatPeriodType must be one of month, quarter, half-year, or null");
    }
  }

  const p = ensureCompanyDirs(companyRoot);
  const db = openDb(p.db);
  try {
    migrate(db);
    seedAccounts(db);
    // `migrate` runs before the chart exists in a freshly-created company, so
    // seed confirmed native control roles only after the chart is present.
    seedNativeAccountRoles(db);
    const cvr = normalizeCvr(options.cvr);
    const fiscalYearStartMonth =
      normalizeFiscalYearStartMonth(options.fiscalYearStartMonth) ?? 1;
    const fiscalYearLabelStrategy =
      normalizeFiscalYearLabelStrategy(options.fiscalYearLabelStrategy) ?? "end-year";
    const name = options.name?.trim() || DEFAULT_COMPANY_SETTINGS.name;
    const address = options.address?.trim() || null;
    const postalCode = options.postalCode?.trim() || null;
    const city = options.city?.trim() || null;
    const paymentTermsDays =
      normalizePaymentTermsDays(options.paymentTermsDays) ?? DEFAULT_PAYMENT_TERMS_DAYS;
    // `null` means "not VAT-registered" and is written through verbatim;
    // undefined (caller didn't pass anything) keeps the historical default
    // cadence so back-compat for callers who never specified vatPeriodType
    // is byte-identical to before.
    const vatPeriodType: VatPeriodType | null =
      options.vatPeriodType === null
        ? null
        : normalizeVatPeriodType(options.vatPeriodType) ?? DEFAULT_VAT_PERIOD_TYPE;
    db.query(
      `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy,
                              address, postal_code, city, payment_terms_days, vat_period_type)
       VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(id) DO UPDATE SET
         name = excluded.name,
         cvr = excluded.cvr,
         fiscal_year_start_month = excluded.fiscal_year_start_month,
         fiscal_year_label_strategy = excluded.fiscal_year_label_strategy,
         address = excluded.address,
         postal_code = excluded.postal_code,
         city = excluded.city,
         payment_terms_days = excluded.payment_terms_days,
         vat_period_type = excluded.vat_period_type`,
    ).run(
      name,
      cvr,
      fiscalYearStartMonth,
      fiscalYearLabelStrategy,
      address,
      postalCode,
      city,
      paymentTermsDays,
      vatPeriodType,
    );
    // #221: capture payment details once — create the primary bank account so
    // every customer-facing invoice PDF shows where to pay.
    if (hasPaymentInfo(options.payment)) {
      upsertPrimaryBankAccount(db, DEFAULT_COMPANY_SETTINGS.currency, options.payment);
    }
    const policy = join(p.config, "policy.yaml");
    if (!existsSync(policy)) {
      writeFileSync(policy, buildDefaultPolicyYaml(options.onboardingActor));
    }
    runSql(db,
      "INSERT INTO audit_log (event_type, entity_type, message) VALUES ('init','company','Company volume initialized')",
    );
  } finally {
    // Company creation must be physically complete before the caller receives
    // the new volume; otherwise Bun may remove empty WAL sidecars during the
    // first later request, making that read appear mutating.
    db.run("PRAGMA wal_checkpoint(TRUNCATE)");
    db.close(true);
  }
  return { companyRoot, dbPath: p.db };
}

export type CompanyOnboardingSummary = {
  /** Display name stored in the ledger. */
  name: string;
  cvr: string | null;
  /** Month (1-12) the fiscal year starts in. */
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: FiscalYearLabelStrategy;
  /**
   * #289: the company's VAT settlement cadence — the canonical period-type
   * value (`month` / `quarter` / `half-year`), or `null` when the company is
   * not VAT-registered.
   */
  vatPeriod: VatPeriodType | null;
  /** Number of accounts seeded into the chart of accounts. */
  accountCount: number;
  /**
   * #241: true when the company has bank/payment details on file (a primary
   * bank account row). Without them an issued invoice's PDF carries no BETALING
   * block, so onboarding must warn the owner.
   */
  hasPaymentDetails: boolean;
};

/**
 * Reads a post-`init` onboarding summary from a company volume: the seeded
 * chart-of-accounts size plus the settings a new owner must confirm (VAT
 * period, fiscal year). Pure read — opens the ledger read-only.
 */
export function summariseCompanyVolume(companyRoot: string): CompanyOnboardingSummary {
  const dbPath = join(companyRoot, "data", "ledger.sqlite");
  const db = openCurrentLedgerReadOnly(dbPath);
  try {
    const settings = getCompanySettings(db);
    const row = db.query("SELECT COUNT(*) AS n FROM accounts").get() as { n: number };
    return {
      name: settings.name,
      cvr: settings.cvr,
      fiscalYearStartMonth: settings.fiscalYearStartMonth,
      fiscalYearLabelStrategy: settings.fiscalYearLabelStrategy,
      vatPeriod: settings.vatPeriodType,
      accountCount: Number(row?.n ?? 0),
      // #241: an issued invoice's PDF carries a BETALING block only when a
      // payment account is configured. Reuse the invoice-side resolver so the
      // onboarding warning matches what the invoice would actually print.
      hasPaymentDetails: resolveCompanyPaymentDetails(db, settings.currency) !== undefined,
    };
  } finally {
    db.close();
  }
}

export type CreateCompanyOptions = CompanyInitOptions & {
  /** Required: display name; also the basis for an auto-derived slug. */
  name: string;
  /** Optional explicit slug; auto-derived from `name` when omitted. */
  slug?: string;
};

export type CreateCompanyResult = CompanyInitResult & {
  slug: string;
  name: string;
};

/**
 * Creates a new company *inside a workspace*: derives/validates a slug, builds
 * the company volume under `<workspaceRoot>/<slug>/`, initialises its ledger,
 * and registers the company in the workspace manifest.
 *
 * Throws if the slug is invalid or already registered. The CLI `company add`
 * command, the future MCP tools and the cockpit API all call this.
 */
export function createCompany(
  workspaceRoot: string,
  options: CreateCompanyOptions,
): CreateCompanyResult {
  const name = options.name?.trim();
  if (!name) throw new Error("createCompany requires a non-empty name");
  const slug = options.slug?.trim() || slugifyCompanyName(name);
  if (!slug || !isValidSlug(slug)) {
    throw new Error(
      `cannot derive a valid slug from '${name}' — pass an explicit slug (lowercase letters, digits, dashes)`,
    );
  }
  const companyRoot = join(workspaceRoot, slug);
  if (existsSync(join(companyRoot, "data", "ledger.sqlite"))) {
    throw new Error(`a company already exists at ${companyRoot}`);
  }
  const init = initialiseCompanyVolume(companyRoot, {
    name,
    cvr: options.cvr,
    fiscalYearStartMonth: options.fiscalYearStartMonth,
    fiscalYearLabelStrategy: options.fiscalYearLabelStrategy,
    address: options.address,
    postalCode: options.postalCode,
    city: options.city,
    paymentTermsDays: options.paymentTermsDays,
    vatPeriodType: options.vatPeriodType,
    payment: options.payment,
    onboardingActor: options.onboardingActor,
  });
  // registerWorkspaceCompany throws on a duplicate slug, so the manifest and
  // the on-disk directory cannot drift apart silently.
  registerWorkspaceCompany(workspaceRoot, {
    slug,
    name,
    createdAt: new Date().toISOString(),
    archived: false,
    purpose: "live",
  });
  return { ...init, slug, name };
}

export function getCompanySettings(db: Database): CompanySettings {
  // #289: older ledgers (and the base schema.sql) may lack `vat_period_type`.
  // Read it only when present; absent → fall back to the quarterly default so
  // a pre-#289 company is unaffected.
  const vatColumn = hasVatPeriodColumn(db) ? "vat_period_type" : "NULL AS vat_period_type";
  const row = db.query(
    `SELECT id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy,
            address, postal_code, city, company_form, industry_code, industry_text,
            cvr_status, audit_waived, cvr_synced_at, payment_terms_days, ${vatColumn}
       FROM companies
      ORDER BY id ASC
      LIMIT 1`
  ).get() as {
    id: number;
    name: string;
    country: string;
    currency: string;
    cvr: string | null;
    fiscal_year_start_month: number;
    fiscal_year_label_strategy: FiscalYearLabelStrategy;
    address: string | null;
    postal_code: string | null;
    city: string | null;
    company_form: string | null;
    industry_code: string | null;
    industry_text: string | null;
    cvr_status: string | null;
    audit_waived: number | null;
    cvr_synced_at: string | null;
    payment_terms_days: number | null;
    vat_period_type: string | null;
  } | null;

  if (!row) return DEFAULT_COMPANY_SETTINGS;
  return {
    id: row.id,
    name: row.name,
    country: row.country,
    currency: row.currency,
    cvr: normalizeCvr(row.cvr),
    fiscalYearStartMonth: normalizeFiscalYearStartMonth(row.fiscal_year_start_month) ?? 1,
    fiscalYearLabelStrategy: normalizeFiscalYearLabelStrategy(row.fiscal_year_label_strategy) ?? "end-year",
    address: row.address,
    postalCode: row.postal_code,
    city: row.city,
    companyForm: row.company_form,
    industryCode: row.industry_code,
    industryText: row.industry_text,
    cvrStatus: row.cvr_status,
    auditWaived: row.audit_waived === null ? null : row.audit_waived === 1,
    cvrSyncedAt: row.cvr_synced_at,
    paymentTermsDays:
      normalizePaymentTermsDays(row.payment_terms_days) ?? DEFAULT_PAYMENT_TERMS_DAYS,
    // `null` in the column = "not VAT-registered" — preserved verbatim. A
    // stored value that fails normalisation (corrupt cell) falls back to the
    // historical default so legacy ledgers stay readable.
    vatPeriodType:
      row.vat_period_type === null
        ? null
        : normalizeVatPeriodType(row.vat_period_type) ?? DEFAULT_VAT_PERIOD_TYPE,
  };
}

/**
 * #221: the editable company profile. After `init`, the owner adjusts their own
 * master data (name, address, CVR, default payment terms, payment details)
 * here, once — every subsequently-issued invoice and its PDF pick the new
 * values up automatically. Only the fields actually passed are changed; the
 * rest keep their current value. The primary bank account is created on first
 * use and is append-only thereafter (the `bank_accounts` guard), so updating
 * payment details after one already exists is reported, not silently dropped.
 */
export type SetCompanyProfileOptions = {
  name?: string | null;
  cvr?: string | null;
  address?: string | null;
  postalCode?: string | null;
  city?: string | null;
  paymentTermsDays?: number | string | null;
  payment?: CompanyPaymentInput;
};

export type SetCompanyProfileResult = {
  ok: boolean;
  /** The company settings after the update (unchanged on failure). */
  settings?: CompanySettings;
  /** Names of the profile fields that actually changed. */
  updatedFields?: string[];
  errors: string[];
};

export function setCompanyProfile(
  db: Database,
  options: SetCompanyProfileOptions,
): SetCompanyProfileResult {
  const errors: string[] = [];
  let cvr: string | null | undefined;
  if (options.cvr !== undefined) {
    try {
      cvr = normalizeCvr(options.cvr);
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
    }
  }
  let paymentTermsDays: number | undefined;
  if (options.paymentTermsDays !== undefined && options.paymentTermsDays !== null) {
    const normalized = normalizePaymentTermsDays(options.paymentTermsDays);
    if (normalized === null) {
      errors.push("paymentTermsDays must be an integer between 0 and 365");
    } else {
      paymentTermsDays = normalized;
    }
  }
  if (errors.length > 0) return { ok: false, errors };

  const before = getCompanySettings(db);
  // A first `setCompanyProfile` before `init` ran would have no row; guard it.
  const exists = db.query("SELECT id FROM companies WHERE id = 1").get() as
    | { id: number }
    | null;
  if (!exists) {
    return {
      ok: false,
      errors: ["company has not been initialised — run 'rentemester init' first"],
    };
  }

  const result = db.transaction(() => {
    const name = options.name !== undefined ? options.name?.trim() || before.name : before.name;
    const nextCvr = options.cvr !== undefined ? cvr ?? null : before.cvr;
    const address =
      options.address !== undefined ? options.address?.trim() || null : before.address;
    const postalCode =
      options.postalCode !== undefined ? options.postalCode?.trim() || null : before.postalCode;
    const city = options.city !== undefined ? options.city?.trim() || null : before.city;
    const terms = paymentTermsDays ?? before.paymentTermsDays;

    db.query(
      `UPDATE companies SET
         name = ?, cvr = ?, address = ?, postal_code = ?, city = ?, payment_terms_days = ?
       WHERE id = 1`,
    ).run(name, nextCvr, address, postalCode, city, terms);

    if (hasPaymentInfo(options.payment)) {
      upsertPrimaryBankAccount(db, before.currency, options.payment);
    }

    insertAuditLog(db, {
      eventType: "company_profile_update",
      entityType: "company",
      entityId: 1,
      message: "Updated company profile (identity / payment details)",
    });
    return getCompanySettings(db);
  }).immediate();

  const updatedFields: string[] = [];
  const compare: Array<[string, unknown, unknown]> = [
    ["name", before.name, result.name],
    ["cvr", before.cvr, result.cvr],
    ["address", before.address, result.address],
    ["postalCode", before.postalCode, result.postalCode],
    ["city", before.city, result.city],
    ["paymentTermsDays", before.paymentTermsDays, result.paymentTermsDays],
  ];
  for (const [field, prev, next] of compare) {
    if (prev !== next) updatedFields.push(field);
  }
  if (hasPaymentInfo(options.payment)) updatedFields.push("payment");

  return { ok: true, settings: result, updatedFields, errors: [] };
}

export type SyncCompanyFromCvrResult = {
  ok: boolean;
  /** The CVR number that was looked up (8 digits, no DK prefix). */
  cvr?: string;
  company?: CvrCompanyInfo;
  /** True when the snapshot came from the local cache, not a fresh fetch. */
  cached?: boolean;
  /** Names of the `companies` columns whose value actually changed. */
  updatedFields?: string[];
  /**
   * The fiscal-year start month as configured vs. as registered in CVR. Sync
   * never rewrites the fiscal year — it is a locked accounting setting — it
   * only reports a mismatch so the user can correct it deliberately.
   */
  fiscalYearStartMonth?: { current: number; cvr: number | null; matches: boolean };
  errors: string[];
};

/**
 * Refreshes the owning company's CVR-register stamdata: looks the company's own
 * CVR number up and writes name/address/branche/form/status into the
 * `companies` row. The fiscal-year configuration is never touched — it is
 * locked after the first journal entry — but a mismatch is reported.
 */
export async function syncCompanyFromCvr(
  db: Database,
  options: CvrLookupOptions = {},
): Promise<SyncCompanyFromCvrResult> {
  const before = getCompanySettings(db);
  const cvrNumber = normalizeCvrNumber(before.cvr);
  if (!cvrNumber) {
    return {
      ok: false,
      errors: [
        "virksomheden har intet gyldigt CVR-nummer registreret — sæt det via 'init --cvr' / 'company add --cvr'",
      ],
    };
  }

  const lookup = await lookupCvrCompany(db, cvrNumber, options);
  if (!lookup.ok || !lookup.company) {
    return { ok: false, cvr: cvrNumber, errors: lookup.errors };
  }

  const info = lookup.company;
  const syncedAt = options.asOf ?? new Date().toISOString();
  const auditWaived = info.auditWaived === null ? null : info.auditWaived ? 1 : 0;

  db.query(
    `UPDATE companies SET
       name = ?, address = ?, postal_code = ?, city = ?, company_form = ?,
       industry_code = ?, industry_text = ?, cvr_status = ?, audit_waived = ?,
       cvr_synced_at = ?
     WHERE id = (SELECT id FROM companies ORDER BY id ASC LIMIT 1)`,
  ).run(
    info.name,
    info.address,
    info.postalCode,
    info.city,
    info.companyFormShort,
    info.industryCode,
    info.industryText,
    info.status,
    auditWaived,
    syncedAt,
  );

  const after = getCompanySettings(db);
  const updatedFields: string[] = [];
  const compare: Array<[string, unknown, unknown]> = [
    ["name", before.name, after.name],
    ["address", before.address, after.address],
    ["postalCode", before.postalCode, after.postalCode],
    ["city", before.city, after.city],
    ["companyForm", before.companyForm, after.companyForm],
    ["industryCode", before.industryCode, after.industryCode],
    ["industryText", before.industryText, after.industryText],
    ["cvrStatus", before.cvrStatus, after.cvrStatus],
    ["auditWaived", before.auditWaived, after.auditWaived],
  ];
  for (const [field, prev, next] of compare) {
    if (prev !== next) updatedFields.push(field);
  }

  insertAuditLog(db, {
    eventType: "company_cvr_sync",
    entityType: "company",
    entityId: after.id,
    message:
      updatedFields.length > 0
        ? `Synced company stamdata from CVR ${cvrNumber}: ${updatedFields.join(", ")}`
        : `Synced company stamdata from CVR ${cvrNumber} (no changes)`,
  });

  return {
    ok: true,
    cvr: cvrNumber,
    company: info,
    cached: lookup.cached,
    updatedFields,
    fiscalYearStartMonth: {
      current: before.fiscalYearStartMonth,
      cvr: info.fiscalYearStartMonth,
      matches:
        info.fiscalYearStartMonth === null ||
        info.fiscalYearStartMonth === before.fiscalYearStartMonth,
    },
    errors: [],
  };
}
