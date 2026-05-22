// Import framework — the Billy export parser.
//
// A Billy export is a directory of JSON files produced by `scripts/billy-export.ts`
// from the Billy REST API (https://api.billysbilling.com/v2). This parser reads:
//
//  - `organization.json`  — company master data (name, CVR, address).
//  - `accounts.json`      — the chart of accounts with account groups and VAT info.
//  - `account-groups.json`— account group classification (used to derive type).
//  - `daybook-transactions.json` — journal entries (historical postings).
//
// The parser produces a normalised `ImportSource`: the chart classified onto
// Rentemester account types, the company master data, and — when daybook
// transactions are present — the cut-over year's opening balance and historical
// entries.
//
// The parser is PURE and DETERMINISTIC: the same export always yields the same
// `ImportSource`, including the order of `chartOfAccounts`, `openingBalances`
// and `unmappedVatCodes`.

import { requireFile } from "./source";
import { parseBillyPostings } from "./billy-postings";
import type {
  ImportAccount,
  ImportAccountType,
  ImportCompanyMasterData,
  ImportHistoricalEntry,
  ImportNormalBalance,
  ImportOpeningBalanceLine,
  MultiArtifactSource,
  ParseResult,
  SourceParser,
} from "./types";

const SYSTEM = "billy";
const LABEL = "Billy (API JSON export — chart of accounts, master data & opening balance)";

// --- Billy account nature → Rentemester type --------------------------------
//
// Billy accounts have a `nature` field that classifies them:
//   "asset", "liability", "equity", "revenue", "expense"
// Some also have a `group` → `natureV2` on the account group.
//
// Billy's `nature` maps cleanly onto Rentemester's types with one rename:
// "revenue" → "income".
function classifyNature(
  nature: string,
): { type: ImportAccountType; normalBalance: ImportNormalBalance } | null {
  const n = (nature ?? "").toLowerCase().trim();
  if (n === "asset") return { type: "asset", normalBalance: "debit" };
  if (n === "liability") return { type: "liability", normalBalance: "credit" };
  if (n === "equity") return { type: "equity", normalBalance: "credit" };
  if (n === "revenue" || n === "income") return { type: "income", normalBalance: "credit" };
  if (n === "expense") return { type: "expense", normalBalance: "debit" };
  return null;
}

// --- Billy tax rate → Rentemester VAT code ----------------------------------
//
// Billy uses UUID-based taxRateIds that reference a tax-rates table. The mapping
// reads `tax-rates.json` (from the export) and classifies each rate by its name
// and percentage onto a Rentemester VAT code.

type BillyTaxRate = {
  id: string;
  name: string;
  rate: number;
  isActive?: boolean;
};

function buildTaxRateMap(taxRatesText: string | undefined): Map<string, string | null> {
  const map = new Map<string, string | null>();
  if (!taxRatesText) return map;
  let rates: BillyTaxRate[];
  try {
    rates = JSON.parse(taxRatesText);
  } catch {
    return map;
  }
  if (!Array.isArray(rates)) return map;

  for (const rate of rates) {
    const name = (rate.name ?? "").toLowerCase().trim();
    const pct = rate.rate ?? 0;
    let vatCode: string | null = null;

    // Billy stores rates as decimals (0.25 = 25%). Normalize if given as percentage.
    const normalizedPct = pct > 1 ? pct / 100 : pct;
    if (normalizedPct >= 0.245 && normalizedPct <= 0.255) {
      if (name.includes("representation") || name.includes("repræsentation")) {
        vatCode = "REPRESENTATION_SPECIAL";
      } else {
        // 25% rate — classified as purchase or sale based on account context later.
        // Default to purchase since most accounts with explicit tax rates are expenses.
        vatCode = "DK_PURCHASE_25";
      }
    } else if (name.includes("reverse charge")) {
      vatCode = "EU_SERVICE_REVERSE_CHARGE";
    } else if (name.includes("services eu") || name.includes("ydelser eu")) {
      vatCode = "EU_SERVICE_REVERSE_CHARGE";
    }
    // 0% rates (goods EU, rest of world, etc.) map to null — no VAT code needed.

    map.set(rate.id, vatCode);
  }
  return map;
}

function mapVatCodeForAccount(
  taxRateId: string | null | undefined,
  accountNature: ImportAccountType,
  taxRateMap: Map<string, string | null>,
): string | null {
  if (!taxRateId) return null;
  const mapped = taxRateMap.get(taxRateId);
  if (mapped === undefined) return null;
  if (mapped === null) return null;
  // Disambiguate 25% rate: income accounts get DK_SALE_25, expenses get DK_PURCHASE_25
  if (mapped === "DK_PURCHASE_25" && accountNature === "income") {
    return "DK_SALE_25";
  }
  return mapped;
}

type BillyAccount = {
  id: string;
  accountNo: number;
  name: string;
  description?: string;
  natureId?: string;
  groupId?: string;
  taxRateId?: string;
  isPaymentEnabled?: boolean;
  isBankAccount?: boolean;
  systemRole?: string;
  isArchived?: boolean;
};

type BillyAccountGroup = {
  id: string;
  name: string;
  natureId?: string;
  type?: string;
};

type BillyOrganization = {
  id: string;
  name?: string;
  registrationNo?: string;
  street?: string;
  zipcode?: string;
  city?: string;
  countryId?: string;
  phone?: string;
  email?: string;
  url?: string;
};

function parseOrganization(text: string, errors: string[]): ImportCompanyMasterData | undefined {
  let org: BillyOrganization;
  try {
    org = JSON.parse(text);
  } catch {
    errors.push("organization.json: invalid JSON");
    return undefined;
  }
  const md: ImportCompanyMasterData = {};
  if (org.name) md.name = org.name;
  if (org.registrationNo) md.cvr = org.registrationNo;
  if (org.street) md.address = org.street;
  if (org.zipcode) md.postalCode = org.zipcode;
  if (org.city) md.city = org.city;
  if (org.countryId) md.country = org.countryId;
  if (org.phone) md.phone = org.phone;
  if (org.email) md.email = org.email;
  if (org.url) md.website = org.url;
  return md;
}

function parseAccounts(
  accountsText: string,
  groupsText: string | undefined,
  taxRatesText: string | undefined,
  errors: string[],
): { accounts: ImportAccount[]; unmappedVatCodes: string[]; accountIdToNo: Map<string, string> } {
  let rawAccounts: BillyAccount[];
  try {
    rawAccounts = JSON.parse(accountsText);
  } catch {
    errors.push("accounts.json: invalid JSON");
    return { accounts: [], unmappedVatCodes: [], accountIdToNo: new Map() };
  }
  if (!Array.isArray(rawAccounts)) {
    errors.push("accounts.json: expected a JSON array");
    return { accounts: [], unmappedVatCodes: [], accountIdToNo: new Map() };
  }

  const groupMap = new Map<string, BillyAccountGroup>();
  if (groupsText) {
    try {
      const groups: BillyAccountGroup[] = JSON.parse(groupsText);
      if (Array.isArray(groups)) {
        for (const g of groups) groupMap.set(g.id, g);
      }
    } catch {
      errors.push("account-groups.json: invalid JSON (non-fatal, continuing without group data)");
    }
  }

  const taxRateMap = buildTaxRateMap(taxRatesText);
  const accounts: ImportAccount[] = [];
  const unmapped = new Set<string>();
  const accountIdToNo = new Map<string, string>();

  const sorted = [...rawAccounts].sort((a, b) => (a.accountNo ?? 0) - (b.accountNo ?? 0));

  for (const raw of sorted) {
    const accountNo = String(raw.accountNo ?? "");
    if (!accountNo || accountNo === "0") continue;

    // Skip archived accounts
    if (raw.isArchived) continue;

    // Derive nature: prefer account's own natureId, fall back to group's natureId
    let nature = raw.natureId ?? "";
    if (!nature && raw.groupId) {
      const group = groupMap.get(raw.groupId);
      if (group) nature = group.natureId ?? "";
    }

    const classified = classifyNature(nature);
    if (!classified) {
      // Billy system accounts (tax settlement, etc.) without a clear nature
      // are skipped with a note — they'll be seeded by Rentemester's init.
      if (raw.systemRole) continue;
      errors.push(
        `accounts.json: account '${accountNo}' (${raw.name}) has unrecognised nature '${nature}'`,
      );
      continue;
    }

    const account: ImportAccount = {
      accountNo,
      name: raw.name ?? `Konto ${accountNo}`,
      type: nature,
      normalizedType: classified.type,
      normalBalance: classified.normalBalance,
      defaultVatCode: null,
    };

    const mappedVat = mapVatCodeForAccount(
      raw.taxRateId ?? null,
      classified.type,
      taxRateMap,
    );
    if (mappedVat) {
      account.defaultVatCode = mappedVat;
    } else if (raw.taxRateId && !taxRateMap.has(raw.taxRateId)) {
      unmapped.add(raw.taxRateId);
    }

    accounts.push(account);
  }

  // Build ID → accountNo map from all raw accounts (not just parsed ones)
  for (const raw of rawAccounts) {
    if (raw.id && raw.accountNo) accountIdToNo.set(raw.id, String(raw.accountNo));
  }

  return { accounts, unmappedVatCodes: [...unmapped].sort(), accountIdToNo };
}

// Opening balance support.
//
// Billy stores journal entries as header objects (daybookTransactions) with
// separate line items (daybookTransactionLines) on a per-transaction endpoint.
// A full opening-balance import requires fetching all line items — this is a
// follow-up feature. For now, the parser supports an optional
// `balances.json` file: an array of { accountNo, balance } pairs representing
// account balances as of the cut-over date. The export script can produce this
// from Billy's balance sheet report.
//
// When `balances.json` is absent, the import proceeds as a chart-only import
// (no primobalance posted), which is valid and useful on its own.

function parseBalances(
  text: string,
  errors: string[],
): { openingBalances: ImportOpeningBalanceLine[]; cutOverDate: string } {
  let data: { cutOverDate?: string; balances?: Array<{ accountNo: string | number; balance: number }> };
  try {
    data = JSON.parse(text);
  } catch {
    errors.push("balances.json: invalid JSON");
    return { openingBalances: [], cutOverDate: "" };
  }

  const cutOverDate = data.cutOverDate ?? "";
  const balances = data.balances ?? [];
  if (!Array.isArray(balances) || balances.length === 0) {
    return { openingBalances: [], cutOverDate };
  }

  const openingBalances: ImportOpeningBalanceLine[] = [];
  for (const b of balances) {
    const accountNo = String(b.accountNo ?? "");
    if (!accountNo) continue;
    const amount = b.balance ?? 0;
    if (amount === 0) continue;
    if (amount > 0) {
      openingBalances.push({ accountNo, debitAmount: amount });
    } else {
      openingBalances.push({ accountNo, creditAmount: -amount });
    }
  }

  return { openingBalances, cutOverDate };
}

function parseBillySource(input: MultiArtifactSource): ParseResult {
  const errors: string[] = [];

  const orgFile = requireFile(input, "organization.json", errors);
  const accountsFile = requireFile(input, "accounts.json", errors);
  if (!orgFile || !accountsFile) {
    return { ok: false, errors };
  }

  const groupsFile = input.files["account-groups.json"];
  const taxRatesFile = input.files["tax-rates.json"];
  const companyMasterData = parseOrganization(orgFile.text, errors);
  const { accounts, unmappedVatCodes, accountIdToNo } = parseAccounts(
    accountsFile.text,
    groupsFile?.text,
    taxRatesFile?.text,
    errors,
  );

  if (accounts.length === 0) {
    errors.push("accounts.json: no accounts parsed from the chart of accounts");
  }

  // Opening balance: from an optional `balances.json` (produced by the export
  // script from Billy's balance sheet). Without it, the import is chart-only.
  const balancesFile = input.files["balances.json"];
  const { openingBalances, cutOverDate } = balancesFile
    ? parseBalances(balancesFile.text, errors)
    : { openingBalances: [] as ImportOpeningBalanceLine[], cutOverDate: "" };

  // Historical entries: year-to-date postings after the cut-over date.
  // Parsed from `postings.json` when present and a cut-over date is set.
  let historicalEntries: ImportHistoricalEntry[] = [];
  const postingsFile = input.files["postings.json"];
  if (postingsFile && cutOverDate) {
    historicalEntries = parseBillyPostings(
      postingsFile.text,
      { cutOverDate, accountIdToNo },
      errors,
    );
  }

  if (errors.length > 0) {
    return { ok: false, errors };
  }

  return {
    ok: true,
    errors: [],
    source: {
      sourceSystem: SYSTEM,
      cutOverDate,
      chartOfAccounts: accounts,
      openingBalances,
      ...(historicalEntries.length > 0 ? { historicalEntries } : {}),
      ...(companyMasterData ? { companyMasterData } : {}),
      ...(unmappedVatCodes.length > 0 ? { unmappedVatCodes } : {}),
    },
  };
}

export const billyParser: SourceParser = {
  system: SYSTEM,
  label: LABEL,
  requiredFiles: ["organization.json", "accounts.json"],
  parseSource: parseBillySource,
};
