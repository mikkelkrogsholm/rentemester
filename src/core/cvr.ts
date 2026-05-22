import type { Database } from "bun:sqlite";
import { addDaysToTimestamp } from "./dates";

/**
 * CVR-register integration — looks a company up in the Danish Central Business
 * Register (CVR) via the public Elasticsearch distribution API at virk.dk.
 *
 * The shape mirrors `vies.ts`: a network function with an injectable
 * `fetchImpl`, a mutable SQLite cache (`cvr_lookups`) and graceful degradation.
 * CVR data is non-deterministic network data — it is fetched once, snapshotted
 * to the cache with a timestamp, and never read live during bookkeeping.
 *
 * Credentials are NEVER bundled: the caller must set `CVR_USERNAME` and
 * `CVR_PASSWORD` (sign up at virk.dk). Without them a lookup degrades to
 * whatever is already cached, or fails with a clear message.
 */

const DEFAULT_CVR_BASE_URL = "http://distribution.virk.dk/cvr-permanent";
const DEFAULT_TTL_DAYS = 30;

export type CvrManagementMember = {
  /** Person or company name of the management member. */
  name: string;
  /** Danish role label, e.g. "Direktion", "Bestyrelse", "FORMAND". */
  role: string;
};

/** A normalised, cockpit-ready snapshot of one CVR company record. */
export type CvrCompanyInfo = {
  /** CVR number, 8 digits, no DK prefix. */
  cvr: string;
  name: string;
  /** Street line of the registered address (vejnavn + husnummer). */
  address: string | null;
  postalCode: string | null;
  city: string | null;
  municipalityCode: number | null;
  /** Numeric company-form code (60 = A/S, 80 = ApS, ...). */
  companyFormCode: number | null;
  /** Short company form, e.g. "ApS". */
  companyFormShort: string | null;
  /** Long company form, e.g. "Anpartsselskab". */
  companyFormLong: string | null;
  /** Company status, e.g. "NORMAL", "UNDER KONKURS". */
  status: string | null;
  industryCode: string | null;
  industryText: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  /** Incorporation date, YYYY-MM-DD. */
  startDate: string | null;
  /** Fiscal-year start as a gMonthDay string, e.g. "--01-01". */
  fiscalYearStart: string | null;
  /** Fiscal-year end as a gMonthDay string, e.g. "--12-31". */
  fiscalYearEnd: string | null;
  /** Fiscal-year start month (1-12) derived from `fiscalYearStart`. */
  fiscalYearStartMonth: number | null;
  /** Whether statutory audit is opted out (REVISION_FRAVALGT). */
  auditWaived: boolean | null;
  /** Registered share capital, in the currency of `shareCapitalCurrency`. */
  shareCapital: number | null;
  shareCapitalCurrency: string | null;
  /** Latest reported employee headcount; CVR lags 1-2 years. */
  employees: number | null;
  /** True when the company opted out of marketing contact (reklamebeskyttet). */
  advertisingProtected: boolean;
  management: CvrManagementMember[];
};

export type CvrLookupResult = {
  ok: boolean;
  company?: CvrCompanyInfo;
  /** True when `company` was served from the local cache, not a fresh fetch. */
  cached: boolean;
  /** ISO timestamp the returned snapshot was fetched from CVR. */
  fetchedAt?: string;
  errors: string[];
};

export type CvrLookupOptions = {
  fetchImpl?: typeof fetch;
  /** Override the CVR Elasticsearch base URL (else RENTEMESTER_CVR_ENDPOINT). */
  endpoint?: string;
  username?: string;
  password?: string;
  /** Cache freshness window in days; default 30. */
  maxAgeDays?: number;
  /** Ignore a fresh cache entry and force a network fetch. */
  forceRefresh?: boolean;
  /** ISO timestamp treated as "now" — for deterministic tests. */
  asOf?: string;
};

// ---------------------------------------------------------------------------
// CVR-number normalisation
// ---------------------------------------------------------------------------

/**
 * Normalise free-form CVR input to the bare 8-digit number CVR's API expects.
 * Accepts "DK12345678", "12345678", spaced or padded variants. Returns null
 * when the input is not a plausible 8-digit CVR number.
 */
export function normalizeCvrNumber(input?: string | null): string | null {
  const compact = input?.trim().toUpperCase().replace(/\s+/g, "").replace(/^DK/, "");
  if (!compact || !/^\d{8}$/.test(compact)) return null;
  return compact;
}

// ---------------------------------------------------------------------------
// Temporal helpers — CVR records carry arrays of period-stamped values;
// `gyldigTil: null` marks the currently valid element.
// ---------------------------------------------------------------------------

function getCurrent<T extends { periode?: { gyldigTil?: string | null } }>(
  items: T[] | undefined,
): T | null {
  if (!items || items.length === 0) return null;
  const current = items.find((item) => item?.periode?.gyldigTil === null);
  return current ?? items[items.length - 1] ?? null;
}

function trimToNull(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function isExpired(expiresAt: string, asOfIso: string) {
  const expires = new Date(expiresAt).getTime();
  const asOf = new Date(asOfIso).getTime();
  return Number.isFinite(expires) && Number.isFinite(asOf) ? expires < asOf : true;
}

// ---------------------------------------------------------------------------
// Elasticsearch query + response mapping
// ---------------------------------------------------------------------------

const LOOKUP_SOURCE = [
  "Vrvirksomhed.cvrNummer",
  "Vrvirksomhed.navne",
  "Vrvirksomhed.virksomhedsform",
  "Vrvirksomhed.virksomhedsstatus",
  "Vrvirksomhed.beliggenhedsadresse",
  "Vrvirksomhed.hovedbranche",
  "Vrvirksomhed.virksomhedMetadata",
  "Vrvirksomhed.telefonNummer",
  "Vrvirksomhed.elektroniskPost",
  "Vrvirksomhed.hjemmeside",
  "Vrvirksomhed.attributter",
  "Vrvirksomhed.deltagerRelation",
  "Vrvirksomhed.reklamebeskyttet",
];

function buildLookupQuery(cvrNumber: string): object {
  return {
    from: 0,
    size: 1,
    _source: LOOKUP_SOURCE,
    query: { term: { "Vrvirksomhed.cvrNummer": Number(cvrNumber) } },
  };
}

/** Read the current public value of a contact-info array (telefon/email/...). */
function extractPublicContact(arr: unknown): string | null {
  if (!Array.isArray(arr)) return null;
  const visible = arr.filter((entry) => entry && (entry as any).hemmelig !== true);
  const current = getCurrent(visible as any[]);
  return trimToNull((current as any)?.kontaktoplysning);
}

/** Read the current value of a typed `attributter` entry. */
function attrValue(entity: any, type: string): string | null {
  const attr = (entity?.attributter ?? []).find((a: any) => a?.type === type);
  if (!attr) return null;
  return trimToNull(getCurrent(attr.vaerdier ?? [])?.vaerdi);
}

function formatStreet(address: any): string | null {
  if (!address) return null;
  const parts: string[] = [];
  if (address.conavn) parts.push(`c/o ${address.conavn}`);
  const street: string[] = [];
  if (address.vejnavn) street.push(String(address.vejnavn));
  if (address.husnummerFra) {
    street.push(String(address.husnummerFra) + (address.bogstavFra ?? ""));
  }
  if (street.length > 0) parts.push(street.join(" "));
  const door: string[] = [];
  if (address.etage) door.push(`${address.etage}.`);
  if (address.sidedoer) door.push(String(address.sidedoer));
  if (door.length > 0) parts.push(door.join(" "));
  return parts.length > 0 ? parts.join(", ") : null;
}

/** "--MM-DD" gMonthDay -> month number 1-12, or null. */
function monthOfGMonthDay(value: string | null): number | null {
  if (!value) return null;
  const match = /^--(\d{2})-\d{2}$/.exec(value);
  if (!match) return null;
  const month = Number(match[1]);
  return month >= 1 && month <= 12 ? month : null;
}

/** The value of a temporal array whose period is still open, else null. */
function openValue(vaerdier: unknown): string | null {
  if (!Array.isArray(vaerdier)) return null;
  const open = vaerdier.find((v) => v && (v as any).periode?.gyldigTil === null);
  return open ? trimToNull((open as any).vaerdi) : null;
}

/**
 * Current management only. `deltagerRelation` carries every participant a
 * company ever had; a member is current when their LEDELSESORGAN function has
 * a value whose period is still open (`gyldigTil: null`).
 */
function extractManagement(entity: any): CvrManagementMember[] {
  const out: CvrManagementMember[] = [];
  const seen = new Set<string>();
  for (const rel of entity?.deltagerRelation ?? []) {
    const name = trimToNull(getCurrent(rel?.deltager?.navne ?? [])?.navn);
    if (!name) continue;
    for (const org of rel?.organisationer ?? []) {
      if (org?.hovedtype !== "LEDELSESORGAN") continue;
      for (const member of org?.medlemsData ?? []) {
        const functionAttr = (member?.attributter ?? []).find(
          (attr: any) => attr?.type === "FUNKTION",
        );
        const role = functionAttr ? openValue(functionAttr.vaerdier) : null;
        if (!role) continue; // not a currently-active member
        const key = `${name}|${role}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push({ name, role });
      }
    }
  }
  return out;
}

/** Map a raw `Vrvirksomhed` entity to the normalised snapshot shape. */
export function mapVirksomhed(entity: any, cvrNumber: string): CvrCompanyInfo {
  const metadata = entity?.virksomhedMetadata ?? {};
  const address =
    metadata.nyesteBeliggenhedsadresse ?? getCurrent(entity?.beliggenhedsadresse ?? []) ?? null;
  const form = metadata.nyesteVirksomhedsform ?? getCurrent(entity?.virksomhedsform ?? []) ?? null;
  const industry = metadata.nyesteHovedbranche ?? getCurrent(entity?.hovedbranche ?? []) ?? null;
  const name =
    trimToNull(metadata.nyesteNavn?.navn) ??
    trimToNull(getCurrent(entity?.navne ?? [])?.navn) ??
    "Ukendt";
  const status =
    trimToNull(metadata.sammensatStatus) ??
    trimToNull(getCurrent(entity?.virksomhedsstatus ?? [])?.status);

  const fiscalYearStart = attrValue(entity, "REGNSKABSÅR_START");
  const fiscalYearEnd = attrValue(entity, "REGNSKABSÅR_SLUT");
  const auditWaivedRaw = attrValue(entity, "REVISION_FRAVALGT");
  const shareCapitalRaw = attrValue(entity, "KAPITAL");
  const shareCapital = shareCapitalRaw !== null ? Number(shareCapitalRaw) : null;

  const employeesRaw = metadata.nyesteAarsbeskaeftigelse?.antalAnsatte;

  return {
    cvr: cvrNumber,
    name,
    address: formatStreet(address),
    postalCode: address?.postnummer != null ? String(address.postnummer) : null,
    city: trimToNull(address?.postdistrikt),
    municipalityCode:
      address?.kommune?.kommuneKode != null ? Number(address.kommune.kommuneKode) : null,
    companyFormCode: form?.virksomhedsformkode != null ? Number(form.virksomhedsformkode) : null,
    companyFormShort: trimToNull(form?.kortBeskrivelse),
    companyFormLong: trimToNull(form?.langBeskrivelse),
    status,
    industryCode: trimToNull(industry?.branchekode),
    industryText: trimToNull(industry?.branchetekst),
    email: extractPublicContact(entity?.elektroniskPost),
    phone: extractPublicContact(entity?.telefonNummer),
    website: extractPublicContact(entity?.hjemmeside),
    startDate: trimToNull(metadata.stiftelsesDato),
    fiscalYearStart,
    fiscalYearEnd,
    fiscalYearStartMonth: monthOfGMonthDay(fiscalYearStart),
    auditWaived: auditWaivedRaw === null ? null : auditWaivedRaw === "true",
    shareCapital: shareCapital !== null && Number.isFinite(shareCapital) ? shareCapital : null,
    shareCapitalCurrency: attrValue(entity, "KAPITALVALUTA"),
    employees: typeof employeesRaw === "number" ? employeesRaw : null,
    advertisingProtected: entity?.reklamebeskyttet === true,
    management: extractManagement(entity),
  };
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

type CvrLookupRow = {
  cvr: string;
  name: string | null;
  address: string | null;
  postal_code: string | null;
  city: string | null;
  municipality_code: number | null;
  company_form_code: number | null;
  company_form_short: string | null;
  company_form_long: string | null;
  status: string | null;
  industry_code: string | null;
  industry_text: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  start_date: string | null;
  fiscal_year_start: string | null;
  fiscal_year_end: string | null;
  audit_waived: number | null;
  share_capital: number | null;
  share_capital_currency: string | null;
  employees: number | null;
  advertising_protected: number;
  management_json: string | null;
  fetched_at: string;
  expires_at: string;
};

function rowToCompany(row: CvrLookupRow): CvrCompanyInfo {
  let management: CvrManagementMember[] = [];
  if (row.management_json) {
    try {
      const parsed = JSON.parse(row.management_json);
      if (Array.isArray(parsed)) management = parsed;
    } catch {
      management = [];
    }
  }
  return {
    cvr: row.cvr,
    name: row.name ?? "Ukendt",
    address: row.address,
    postalCode: row.postal_code,
    city: row.city,
    municipalityCode: row.municipality_code,
    companyFormCode: row.company_form_code,
    companyFormShort: row.company_form_short,
    companyFormLong: row.company_form_long,
    status: row.status,
    industryCode: row.industry_code,
    industryText: row.industry_text,
    email: row.email,
    phone: row.phone,
    website: row.website,
    startDate: row.start_date,
    fiscalYearStart: row.fiscal_year_start,
    fiscalYearEnd: row.fiscal_year_end,
    fiscalYearStartMonth: monthOfGMonthDay(row.fiscal_year_start),
    auditWaived: row.audit_waived === null ? null : row.audit_waived === 1,
    shareCapital: row.share_capital,
    shareCapitalCurrency: row.share_capital_currency,
    employees: row.employees,
    advertisingProtected: row.advertising_protected === 1,
    management,
  };
}

/** Read a cached CVR snapshot, or null when none exists. */
export function getCachedCvrLookup(
  db: Database,
  cvrInput: string | null | undefined,
): { company: CvrCompanyInfo; fetchedAt: string; expiresAt: string } | null {
  const cvrNumber = normalizeCvrNumber(cvrInput);
  if (!cvrNumber) return null;
  const row = db
    .query(`SELECT * FROM cvr_lookups WHERE cvr = ?`)
    .get(cvrNumber) as CvrLookupRow | null;
  if (!row) return null;
  return { company: rowToCompany(row), fetchedAt: row.fetched_at, expiresAt: row.expires_at };
}

/** Upsert a CVR snapshot into the cache. */
export function storeCvrLookup(
  db: Database,
  company: CvrCompanyInfo,
  meta: { rawResponse?: string | null; fetchedAt: string; expiresAt: string },
): void {
  db.query(
    `INSERT INTO cvr_lookups (
       cvr, name, address, postal_code, city, municipality_code,
       company_form_code, company_form_short, company_form_long, status,
       industry_code, industry_text, email, phone, website, start_date,
       fiscal_year_start, fiscal_year_end, audit_waived, share_capital,
       share_capital_currency, employees, advertising_protected,
       management_json, raw_response, fetched_at, expires_at
     ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
     ON CONFLICT(cvr) DO UPDATE SET
       name = excluded.name, address = excluded.address,
       postal_code = excluded.postal_code, city = excluded.city,
       municipality_code = excluded.municipality_code,
       company_form_code = excluded.company_form_code,
       company_form_short = excluded.company_form_short,
       company_form_long = excluded.company_form_long, status = excluded.status,
       industry_code = excluded.industry_code, industry_text = excluded.industry_text,
       email = excluded.email, phone = excluded.phone, website = excluded.website,
       start_date = excluded.start_date, fiscal_year_start = excluded.fiscal_year_start,
       fiscal_year_end = excluded.fiscal_year_end, audit_waived = excluded.audit_waived,
       share_capital = excluded.share_capital,
       share_capital_currency = excluded.share_capital_currency,
       employees = excluded.employees,
       advertising_protected = excluded.advertising_protected,
       management_json = excluded.management_json, raw_response = excluded.raw_response,
       fetched_at = excluded.fetched_at, expires_at = excluded.expires_at`,
  ).run(
    company.cvr,
    company.name,
    company.address,
    company.postalCode,
    company.city,
    company.municipalityCode,
    company.companyFormCode,
    company.companyFormShort,
    company.companyFormLong,
    company.status,
    company.industryCode,
    company.industryText,
    company.email,
    company.phone,
    company.website,
    company.startDate,
    company.fiscalYearStart,
    company.fiscalYearEnd,
    company.auditWaived === null ? null : company.auditWaived ? 1 : 0,
    company.shareCapital,
    company.shareCapitalCurrency,
    company.employees,
    company.advertisingProtected ? 1 : 0,
    JSON.stringify(company.management),
    meta.rawResponse ?? null,
    meta.fetchedAt,
    meta.expiresAt,
  );
}

// ---------------------------------------------------------------------------
// Network lookup
// ---------------------------------------------------------------------------

type FetchOutcome =
  | { ok: true; entity: any }
  | { ok: false; error: string };

async function fetchCvrCompany(
  cvrNumber: string,
  options: { fetchImpl: typeof fetch; baseUrl: string; username: string; password: string },
): Promise<FetchOutcome> {
  const url = `${options.baseUrl.replace(/\/+$/, "")}/virksomhed/_search`;
  const auth = Buffer.from(`${options.username}:${options.password}`).toString("base64");

  let response: Response;
  try {
    response = await options.fetchImpl(url, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Basic ${auth}`,
      },
      body: JSON.stringify(buildLookupQuery(cvrNumber)),
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : "netværksfejl";
    return { ok: false, error: `CVR-opslag fejlede: ${detail}` };
  }

  if (!response.ok) {
    return { ok: false, error: `CVR-opslag fejlede med HTTP ${response.status}` };
  }

  let json: any;
  try {
    json = await response.json();
  } catch {
    return { ok: false, error: "CVR returnerede et svar der ikke er gyldig JSON" };
  }

  const entity = json?.hits?.hits?.[0]?._source?.Vrvirksomhed;
  if (!entity) {
    return { ok: false, error: `Ingen virksomhed fundet for CVR-nummer ${cvrNumber}` };
  }
  return { ok: true, entity };
}

/**
 * Look a company up in the CVR register. A fresh cache hit is returned without
 * a network call. Otherwise the register is queried, the snapshot is cached and
 * returned. When credentials are missing the call degrades to any cached
 * snapshot, else fails with a clear, non-throwing error.
 */
export async function lookupCvrCompany(
  db: Database,
  cvrInput: string | null | undefined,
  options: CvrLookupOptions = {},
): Promise<CvrLookupResult> {
  const cvrNumber = normalizeCvrNumber(cvrInput);
  if (!cvrNumber) {
    return {
      ok: false,
      cached: false,
      errors: ["cvr skal være et 8-cifret CVR-nummer (evt. med DK-præfiks)"],
    };
  }

  const asOf = options.asOf ?? new Date().toISOString();
  const maxAgeDays = options.maxAgeDays ?? DEFAULT_TTL_DAYS;
  const cached = getCachedCvrLookup(db, cvrNumber);

  if (cached && !options.forceRefresh && !isExpired(cached.expiresAt, asOf)) {
    return {
      ok: true,
      company: cached.company,
      cached: true,
      fetchedAt: cached.fetchedAt,
      errors: [],
    };
  }

  const username = options.username ?? process.env.CVR_USERNAME;
  const password = options.password ?? process.env.CVR_PASSWORD;
  if (!username || !password) {
    if (cached) {
      // No credentials, but a stale snapshot is still better than nothing.
      return {
        ok: true,
        company: cached.company,
        cached: true,
        fetchedAt: cached.fetchedAt,
        errors: [],
      };
    }
    return {
      ok: false,
      cached: false,
      errors: [
        "CVR-opslag kræver miljøvariablerne CVR_USERNAME og CVR_PASSWORD — opret adgang på virk.dk",
      ],
    };
  }

  const baseUrl =
    options.endpoint ?? process.env.RENTEMESTER_CVR_ENDPOINT ?? DEFAULT_CVR_BASE_URL;
  const fetched = await fetchCvrCompany(cvrNumber, {
    fetchImpl: options.fetchImpl ?? fetch,
    baseUrl,
    username,
    password,
  });

  if (!fetched.ok) {
    if (cached) {
      // The network failed — fall back to the stale snapshot rather than error.
      return {
        ok: true,
        company: cached.company,
        cached: true,
        fetchedAt: cached.fetchedAt,
        errors: [],
      };
    }
    return { ok: false, cached: false, errors: [fetched.error] };
  }

  const company = mapVirksomhed(fetched.entity, cvrNumber);
  storeCvrLookup(db, company, {
    rawResponse: JSON.stringify(fetched.entity),
    fetchedAt: asOf,
    expiresAt: addDaysToTimestamp(asOf, maxAgeDays),
  });

  return {
    ok: true,
    company,
    cached: false,
    fetchedAt: asOf,
    errors: [],
  };
}
