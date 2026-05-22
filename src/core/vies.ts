import type { Database } from "bun:sqlite";
import { addDaysToTimestamp } from "./dates";

export type NormalizedEuVat = {
  countryCode: string;
  vatNumber: string;
  normalized: string;
};

export type ViesValidationRecord = NormalizedEuVat & {
  valid: boolean;
  name: string | null;
  address: string | null;
  validatedAt: string;
  expiresAt: string;
  rawResponse: string | null;
};

export type ValidateVatResult = {
  ok: boolean;
  validation?: ViesValidationRecord;
  appliedRules: string[];
  errors: string[];
};

const RULE_ID = "DK-VAT-REVERSE-CHARGE-001";
const DEFAULT_TTL_DAYS = 90;
const DEFAULT_VIES_ENDPOINT = "https://ec.europa.eu/taxation_customs/vies/rest-api/check-vat-number";

// EU member-state VAT country codes recognised by VIES. EU service reverse
// charge (momsloven §46) applies only to suppliers in *other* EU member
// states, so DK and non-EU codes (NO, CH, GB, ...) must be rejected here.
// EL is the VIES code for Greece.
const EU_VAT_COUNTRY_CODES = new Set([
  "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR",
  "HR", "HU", "IE", "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO",
  "SE", "SI", "SK",
]);

export function normalizeEuVatNumber(input?: string | null): NormalizedEuVat | null {
  const compact = input?.trim().toUpperCase().replace(/[^A-Z0-9]/g, "");
  if (!compact || compact.length < 3) return null;
  const countryCode = compact.slice(0, 2);
  const vatNumber = compact.slice(2);
  if (!/^[A-Z]{2}$/.test(countryCode) || !/^[A-Z0-9]{2,14}$/.test(vatNumber)) return null;
  // Domestic (DK) numbers are valid EU VAT numbers for VIES caching but must
  // not be treated as a foreign EU supplier — the reverse-charge path rejects
  // them explicitly below.
  if (!EU_VAT_COUNTRY_CODES.has(countryCode)) return null;
  return { countryCode, vatNumber, normalized: `${countryCode}${vatNumber}` };
}

export function lookupCachedViesValidation(db: Database, input?: string | null) {
  const parsed = normalizeEuVatNumber(input);
  if (!parsed) return null;
  const row = db.query(
    `SELECT country_code, vat_number, valid, name, address, validated_at, expires_at, raw_response
       FROM vies_validations
      WHERE country_code = ? AND vat_number = ?`
  ).get(parsed.countryCode, parsed.vatNumber) as {
    country_code: string;
    vat_number: string;
    valid: number;
    name: string | null;
    address: string | null;
    validated_at: string;
    expires_at: string;
    raw_response: string | null;
  } | null;
  if (!row) return null;
  return {
    countryCode: row.country_code,
    vatNumber: row.vat_number,
    normalized: `${row.country_code}${row.vat_number}`,
    valid: row.valid === 1,
    name: row.name,
    address: row.address,
    validatedAt: row.validated_at,
    expiresAt: row.expires_at,
    rawResponse: row.raw_response,
  } satisfies ViesValidationRecord;
}

export function storeViesValidation(db: Database, validation: {
  vatOrCvr?: string | null;
  countryCode?: string;
  vatNumber?: string;
  valid: boolean;
  name?: string | null;
  address?: string | null;
  validatedAt?: string;
  expiresAt?: string;
  rawResponse?: string | null;
}) {
  const parsed = validation.vatOrCvr ? normalizeEuVatNumber(validation.vatOrCvr) : (validation.countryCode && validation.vatNumber
    ? normalizeEuVatNumber(`${validation.countryCode}${validation.vatNumber}`)
    : null);
  if (!parsed) throw new Error("valid EU VAT number is required");
  const validatedAt = validation.validatedAt ?? new Date().toISOString();
  const expiresAt = validation.expiresAt ?? addDaysToTimestamp(validatedAt, DEFAULT_TTL_DAYS);
  db.query(
    `INSERT INTO vies_validations (country_code, vat_number, valid, name, address, validated_at, expires_at, raw_response)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(country_code, vat_number) DO UPDATE SET
       valid = excluded.valid,
       name = excluded.name,
       address = excluded.address,
       validated_at = excluded.validated_at,
       expires_at = excluded.expires_at,
       raw_response = excluded.raw_response`
  ).run(
    parsed.countryCode,
    parsed.vatNumber,
    validation.valid ? 1 : 0,
    validation.name ?? null,
    validation.address ?? null,
    validatedAt,
    expiresAt,
    validation.rawResponse ?? null,
  );
  return lookupCachedViesValidation(db, parsed.normalized)!;
}

function isExpired(expiresAt: string, asOfIso?: string) {
  const expires = new Date(expiresAt).getTime();
  const asOf = new Date(asOfIso ?? new Date().toISOString()).getTime();
  return Number.isFinite(expires) && Number.isFinite(asOf) ? expires < asOf : true;
}

export function requireCachedViesValidation(db: Database, vatOrCvr: string | null | undefined, label: string, asOfIso?: string): ValidateVatResult {
  const parsed = normalizeEuVatNumber(vatOrCvr);
  if (!parsed) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`${label} must be a plausible EU VAT number`] };
  }
  const cached = lookupCachedViesValidation(db, parsed.normalized);
  if (!cached) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [
        `VIES lookup not yet performed for ${label} (${parsed.normalized}) — ` +
          `validate the VAT number against VIES first ` +
          `(CLI: \`customer validate-vat\`; MCP: \`customer_validate_vat\`).`,
      ],
    };
  }
  if (!cached.valid) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`${label} ${parsed.normalized} is not a valid EU VAT number per cached VIES result from ${cached.validatedAt}`] };
  }
  if (isExpired(cached.expiresAt, asOfIso)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`VIES validation for ${label} ${parsed.normalized} expired at ${cached.expiresAt} — re-run validation`] };
  }
  return { ok: true, validation: cached, appliedRules: [RULE_ID], errors: [] };
}

/**
 * Pick the first recognised validity field that is an explicit boolean.
 * Returns `undefined` when the response carries no unambiguous boolean
 * validity field (schema change, partial outage, error body) — the caller
 * must NOT treat such a response as authoritative.
 */
function extractValidity(json: any): boolean | undefined {
  for (const candidate of [json?.valid, json?.isValid, json?.result?.valid]) {
    if (typeof candidate === "boolean") return candidate;
  }
  return undefined;
}

type ParsedValidationResponse =
  | { ok: true; record: ViesValidationRecord }
  | { ok: false; error: string };

function parseValidationResponse(json: any, parsed: NormalizedEuVat, validatedAt: string): ParsedValidationResponse {
  const valid = extractValidity(json);
  if (valid === undefined) {
    // "VIES could not answer" must be distinguishable from "VIES says
    // invalid" — refuse to cache an ambiguous body.
    return { ok: false, error: `VIES response for ${parsed.normalized} did not contain a recognised boolean validity field` };
  }
  const name = typeof (json?.name ?? json?.traderName ?? json?.result?.name) === "string" ? (json?.name ?? json?.traderName ?? json?.result?.name) : null;
  const address = typeof (json?.address ?? json?.traderAddress ?? json?.result?.address) === "string" ? (json?.address ?? json?.traderAddress ?? json?.result?.address) : null;
  return {
    ok: true,
    record: {
      ...parsed,
      valid,
      name,
      address,
      validatedAt,
      expiresAt: addDaysToTimestamp(validatedAt, DEFAULT_TTL_DAYS),
      rawResponse: JSON.stringify(json),
    },
  };
}

export async function validateVatAgainstVies(db: Database, vatOrCvr: string, options: { endpoint?: string; fetchImpl?: typeof fetch } = {}): Promise<ValidateVatResult> {
  const parsed = normalizeEuVatNumber(vatOrCvr);
  if (!parsed) return { ok: false, appliedRules: [RULE_ID], errors: ["cvr must be a plausible EU VAT number"] };

  const fetchImpl = options.fetchImpl ?? fetch;
  const endpoint = options.endpoint ?? process.env.RENTEMESTER_VIES_ENDPOINT ?? DEFAULT_VIES_ENDPOINT;
  const response = await fetchImpl(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ countryCode: parsed.countryCode, vatNumber: parsed.vatNumber }),
  });

  if (!response.ok) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`VIES lookup failed with HTTP ${response.status}`] };
  }

  let json: unknown;
  try {
    json = await response.json();
  } catch {
    return { ok: false, appliedRules: [RULE_ID], errors: ["VIES returned a non-JSON response body"] };
  }
  const parsedResponse = parseValidationResponse(json, parsed, new Date().toISOString());
  if (!parsedResponse.ok) {
    // Ambiguous / error body — do NOT write to vies_validations so a later
    // genuine lookup is not blocked for the full TTL window.
    return { ok: false, appliedRules: [RULE_ID], errors: [parsedResponse.error] };
  }
  const stored = storeViesValidation(db, parsedResponse.record);
  return { ok: true, validation: stored, appliedRules: [RULE_ID], errors: [] };
}
