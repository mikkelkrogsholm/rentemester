/**
 * Billy contacts JSON import.
 *
 * Parses `contacts.json` from a Billy API export and lands each contact in
 * Rentemester's `customers` / `vendors` master data. Follows the same pattern
 * as the Dinero contacts importer (`dinero-contacts.ts`).
 *
 * Design:
 *  - Billy contacts carry explicit `isCustomer` / `isSupplier` booleans.
 *  - A contact with both flags becomes both a customer AND a vendor.
 *  - A contact with neither flag falls back to `defaultRole`.
 *  - Re-import is idempotent: matched on (vat_or_cvr, name) natural key.
 *  - Archived contacts are skipped.
 *  - `registrationNo` is normalised to `DK########` for Danish companies.
 *  - No CVR enrichment — the export already carries the API's full data.
 */

import type { Database } from "bun:sqlite";
import {
  createCustomer,
  createVendor,
  findCustomerByKey,
  findVendorByKey,
  type CreateCustomerInput,
  type CreateVendorInput,
} from "../master-data";

export type ContactRole = "customer" | "vendor";

export type BillyContact = {
  id: string;
  name: string;
  type?: string;
  countryId?: string;
  street?: string;
  cityText?: string;
  zipcodeText?: string;
  phone?: string;
  registrationNo?: string;
  vatNo?: string;
  ean?: string;
  isCustomer?: boolean;
  isSupplier?: boolean;
  isArchived?: boolean;
  paymentTermsDays?: number | null;
  defaultExpenseAccountId?: string | null;
  contactNo?: string;
};

export type BillyContactImportSummary = {
  parsed: number;
  customersCreated: number;
  vendorsCreated: number;
  skipped: number;
  archivedSkipped: number;
};

export type BillyContactImportResult = {
  ok: boolean;
  summary: BillyContactImportSummary;
  errors: string[];
};

export type ImportBillyContactsOptions = {
  defaultRole?: ContactRole;
  /** Map of Billy account IDs to Rentemester account numbers (for default_expense_account). */
  accountIdMap?: Map<string, string>;
};

function trimOrNull(value: string | null | undefined): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function composeAddress(street: string | null, zipcode: string | null, city: string | null): string | null {
  const cleanStreet = street ? street.replace(/[,\s]+$/, "") : null;
  const cityLine = [zipcode, city].filter(Boolean).join(" ");
  const full = [cleanStreet, cityLine].filter((part) => part && part.length > 0).join(", ");
  return full.length > 0 ? full : null;
}

function normalizeRegistrationNo(
  registrationNo: string | null | undefined,
  vatNo: string | null | undefined,
  countryId: string | null | undefined,
): string | null {
  // Prefer registrationNo (bare digits), fall back to vatNo (may have country prefix)
  const raw = trimOrNull(registrationNo) ?? trimOrNull(vatNo);
  if (!raw) return null;
  const compact = raw.replace(/\s+/g, "").toUpperCase();
  if (compact.length === 0) return null;
  const isDanish = (countryId ?? "").toUpperCase() === "DK";
  if (isDanish && /^(DK)?\d{8}$/.test(compact)) {
    const digits = compact.replace(/^DK/, "");
    return `DK${digits}`;
  }
  return compact;
}

function classifyRoles(contact: BillyContact, defaultRole: ContactRole): ContactRole[] {
  const roles: ContactRole[] = [];
  if (contact.isCustomer) roles.push("customer");
  if (contact.isSupplier) roles.push("vendor");
  if (roles.length === 0) roles.push(defaultRole);
  return roles;
}

export function parseBillyContacts(raw: string): {
  ok: boolean;
  contacts: BillyContact[];
  errors: string[];
} {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ok: false, contacts: [], errors: ["contacts.json: invalid JSON"] };
  }
  if (!Array.isArray(parsed)) {
    return { ok: false, contacts: [], errors: ["contacts.json: expected a JSON array"] };
  }
  const contacts = parsed.filter(
    (entry): entry is BillyContact =>
      typeof entry === "object" && entry !== null && typeof entry.name === "string",
  );
  return { ok: true, contacts, errors: [] };
}

export function importBillyContacts(
  db: Database,
  contactsJson: string,
  options: ImportBillyContactsOptions = {},
): BillyContactImportResult {
  const parsed = parseBillyContacts(contactsJson);
  if (!parsed.ok) {
    return {
      ok: false,
      summary: { parsed: 0, customersCreated: 0, vendorsCreated: 0, skipped: 0, archivedSkipped: 0 },
      errors: parsed.errors,
    };
  }

  const defaultRole: ContactRole = options.defaultRole ?? "vendor";
  const accountIdMap = options.accountIdMap ?? new Map<string, string>();
  const summary: BillyContactImportSummary = {
    parsed: parsed.contacts.length,
    customersCreated: 0,
    vendorsCreated: 0,
    skipped: 0,
    archivedSkipped: 0,
  };
  const errors: string[] = [];

  // Sort by name for deterministic ordering
  const sorted = [...parsed.contacts].sort((a, b) => (a.name ?? "").localeCompare(b.name ?? ""));

  for (const contact of sorted) {
    if (!contact.name || contact.name.trim().length === 0) {
      errors.push(`contact ${contact.id ?? "?"}: missing name — skipped`);
      continue;
    }
    if (contact.isArchived) {
      summary.archivedSkipped += 1;
      continue;
    }

    const vatOrCvr = normalizeRegistrationNo(contact.registrationNo, contact.vatNo, contact.countryId);
    const address = composeAddress(
      trimOrNull(contact.street),
      trimOrNull(contact.zipcodeText),
      trimOrNull(contact.cityText),
    );
    const phone = trimOrNull(contact.phone);
    const ean = trimOrNull(contact.ean);

    for (const role of classifyRoles(contact, defaultRole)) {
      if (role === "customer") {
        if (findCustomerByKey(db, vatOrCvr, contact.name)) {
          summary.skipped += 1;
          continue;
        }
        const input: CreateCustomerInput = {
          name: contact.name,
          ...(address ? { address } : {}),
          ...(vatOrCvr ? { vatOrCvr } : {}),
          ...(phone ? { phone } : {}),
          ...(ean ? { eanNumber: ean } : {}),
          ...(contact.paymentTermsDays && contact.paymentTermsDays > 0
            ? { paymentTermsDays: contact.paymentTermsDays }
            : {}),
        };
        const result = createCustomer(db, input);
        if (result.ok) summary.customersCreated += 1;
        else errors.push(`customer '${contact.name}': ${result.errors.join(", ")}`);
      } else {
        if (findVendorByKey(db, vatOrCvr, contact.name)) {
          summary.skipped += 1;
          continue;
        }
        const expenseAccount = contact.defaultExpenseAccountId
          ? accountIdMap.get(contact.defaultExpenseAccountId) ?? null
          : null;
        const input: CreateVendorInput = {
          name: contact.name,
          ...(address ? { address } : {}),
          ...(vatOrCvr ? { vatOrCvr } : {}),
          ...(phone ? { phone } : {}),
          ...(expenseAccount ? { defaultExpenseAccount: expenseAccount } : {}),
        };
        const result = createVendor(db, input);
        if (result.ok) summary.vendorsCreated += 1;
        else errors.push(`vendor '${contact.name}': ${result.errors.join(", ")}`);
      }
    }
  }

  return { ok: true, summary, errors };
}
