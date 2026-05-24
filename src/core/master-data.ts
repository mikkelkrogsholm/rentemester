import type { Database } from "bun:sqlite";
import type { InvoicePayload } from "./invoice";
import type { DocumentMetadata } from "./documents";
import { insertAuditLog } from "./actor";
import { addDays } from "./dates";
import { normalizeEanNumber, trimToNull } from "./ean";
import { lookupCvrCompany, type CvrCompanyInfo, type CvrLookupOptions } from "./cvr";

export type CustomerRecord = {
  id: number;
  name: string;
  address: string | null;
  vatOrCvr: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  paymentTermsDays: number;
  defaultCurrency: string;
  notes: string | null;
  archived: number;
  createdAt: string;
};

export type VendorRecord = {
  id: number;
  name: string;
  address: string | null;
  vatOrCvr: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
  notes: string | null;
  archived: number;
  createdAt: string;
};

export type CreateCustomerInput = {
  name: string;
  address?: string;
  vatOrCvr?: string;
  email?: string;
  phone?: string;
  website?: string;
  eanNumber?: string;
  paymentTermsDays?: number;
  defaultCurrency?: string;
  notes?: string;
};

export type CreateVendorInput = {
  name: string;
  address?: string;
  vatOrCvr?: string;
  email?: string;
  phone?: string;
  website?: string;
  defaultExpenseAccount?: string;
  defaultVatTreatment?: string;
  notes?: string;
};

function normalizeCurrency(value: string | null | undefined) {
  return (trimToNull(value) ?? "DKK").toUpperCase();
}

export function createCustomer(db: Database, input: CreateCustomerInput) {
  const name = trimToNull(input.name);
  if (!name) return { ok: false, errors: ["name is required"] };
  const rawEanNumber = trimToNull(input.eanNumber);
  const eanNumber = rawEanNumber ? normalizeEanNumber(rawEanNumber) : null;
  if (rawEanNumber && !eanNumber) return { ok: false, errors: ["eanNumber must be 13 digits"] };
  const paymentTermsDays = Number.isInteger(input.paymentTermsDays) && Number(input.paymentTermsDays) > 0 ? Number(input.paymentTermsDays) : 30;
  const defaultCurrency = normalizeCurrency(input.defaultCurrency);
  if (!/^[A-Z]{3}$/.test(defaultCurrency)) return { ok: false, errors: ["defaultCurrency must be a 3-letter ISO code"] };

  const inserted = db.transaction(() => {
    const row = db.query(
      `INSERT INTO customers (name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       RETURNING id, created_at`
    ).get(
      name,
      trimToNull(input.address),
      trimToNull(input.vatOrCvr),
      trimToNull(input.email),
      trimToNull(input.phone),
      trimToNull(input.website),
      eanNumber,
      paymentTermsDays,
      defaultCurrency,
      trimToNull(input.notes),
    ) as { id: number; created_at: string };

    insertAuditLog(db, {
      eventType: "customer_create",
      entityType: "customer",
      entityId: row.id,
      message: `Created customer ${name}`,
    });

    return row;
  }, { immediate: true })();

  return { ok: true, customerId: inserted.id, appliedRules: ["DK-MASTER-DATA-CUSTOMER-001"], errors: [] };
}

export function listCustomers(db: Database, options: { archived?: boolean } = {}) {
  const rows = db.query(
    `SELECT id, name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes, archived, created_at
     FROM customers
     WHERE archived = CASE WHEN ? THEN archived ELSE 0 END
     ORDER BY lower(name) ASC, id ASC`
  ).all(options.archived ? 1 : 0) as Array<{
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; ean_number: string | null; payment_terms_days: number; default_currency: string; notes: string | null; archived: number; created_at: string;
  }>;

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      name: row.name,
      address: row.address,
      vatOrCvr: row.vat_or_cvr,
      email: row.email,
      phone: row.phone,
      website: row.website,
      eanNumber: row.ean_number,
      paymentTermsDays: row.payment_terms_days,
      defaultCurrency: row.default_currency,
      notes: row.notes,
      archived: Boolean(row.archived),
      createdAt: row.created_at,
    })),
    errors: [],
  };
}

export function createVendor(db: Database, input: CreateVendorInput) {
  const name = trimToNull(input.name);
  if (!name) return { ok: false, errors: ["name is required"] };

  const inserted = db.transaction(() => {
    const row = db.query(
      `INSERT INTO vendors (name, address, vat_or_cvr, email, phone, website, default_expense_account, default_vat_treatment, notes)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
       RETURNING id, created_at`
    ).get(
      name,
      trimToNull(input.address),
      trimToNull(input.vatOrCvr),
      trimToNull(input.email),
      trimToNull(input.phone),
      trimToNull(input.website),
      trimToNull(input.defaultExpenseAccount),
      trimToNull(input.defaultVatTreatment),
      trimToNull(input.notes),
    ) as { id: number; created_at: string };

    insertAuditLog(db, {
      eventType: "vendor_create",
      entityType: "vendor",
      entityId: row.id,
      message: `Created vendor ${name}`,
    });

    return row;
  }, { immediate: true })();

  return { ok: true, vendorId: inserted.id, appliedRules: ["DK-MASTER-DATA-VENDOR-001"], errors: [] };
}

export function listVendors(db: Database, options: { archived?: boolean } = {}) {
  const rows = db.query(
    `SELECT id, name, address, vat_or_cvr, email, phone, website, default_expense_account, default_vat_treatment, notes, archived, created_at
     FROM vendors
     WHERE archived = CASE WHEN ? THEN archived ELSE 0 END
     ORDER BY lower(name) ASC, id ASC`
  ).all(options.archived ? 1 : 0) as Array<{
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; default_expense_account: string | null; default_vat_treatment: string | null; notes: string | null; archived: number; created_at: string;
  }>;

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      name: row.name,
      address: row.address,
      vatOrCvr: row.vat_or_cvr,
      email: row.email,
      phone: row.phone,
      website: row.website,
      defaultExpenseAccount: row.default_expense_account,
      defaultVatTreatment: row.default_vat_treatment,
      notes: row.notes,
      archived: Boolean(row.archived),
      createdAt: row.created_at,
    })),
    errors: [],
  };
}

export function getCustomerById(db: Database, id: number) {
  return db.query(
    `SELECT id, name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes, archived, created_at
     FROM customers WHERE id = ? LIMIT 1`
  ).get(id) as {
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; ean_number: string | null; payment_terms_days: number; default_currency: string; notes: string | null; archived: number; created_at: string;
  } | null;
}

export function getVendorById(db: Database, id: number) {
  return db.query(
    `SELECT id, name, address, vat_or_cvr, email, phone, website, default_expense_account, default_vat_treatment, notes, archived, created_at
     FROM vendors WHERE id = ? LIMIT 1`
  ).get(id) as {
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; default_expense_account: string | null; default_vat_treatment: string | null; notes: string | null; archived: number; created_at: string;
  } | null;
}

/**
 * Update fields on an existing customer (#390). Mirrors `createCustomer`'s
 * validation: a present `name` may not be blank, `defaultCurrency` must stay a
 * 3-letter ISO code, and `eanNumber` must normalise to 13 digits. Fields that
 * are absent (`undefined`) are left untouched; an explicit `null` clears them.
 */
export type UpdateCustomerInput = Partial<Omit<CreateCustomerInput, "name">> & {
  name?: string;
};

export function updateCustomer(
  db: Database,
  id: number,
  input: UpdateCustomerInput,
) {
  const existing = getCustomerById(db, id);
  if (!existing) return { ok: false, errors: [`customer ${id} does not exist`] };

  let nextName = existing.name;
  if (input.name !== undefined) {
    const trimmed = trimToNull(input.name);
    if (!trimmed) return { ok: false, errors: ["name must not be empty"] };
    nextName = trimmed;
  }

  let nextEan = existing.ean_number;
  if (input.eanNumber !== undefined) {
    const raw = trimToNull(input.eanNumber);
    if (raw === null) {
      nextEan = null;
    } else {
      const norm = normalizeEanNumber(raw);
      if (!norm) return { ok: false, errors: ["eanNumber must be 13 digits"] };
      nextEan = norm;
    }
  }

  let nextPaymentTerms = existing.payment_terms_days;
  if (input.paymentTermsDays !== undefined) {
    const value = Number(input.paymentTermsDays);
    if (!Number.isInteger(value) || value <= 0) {
      return { ok: false, errors: ["paymentTermsDays must be a positive integer"] };
    }
    nextPaymentTerms = value;
  }

  let nextCurrency = existing.default_currency;
  if (input.defaultCurrency !== undefined) {
    const value = normalizeCurrency(input.defaultCurrency);
    if (!/^[A-Z]{3}$/.test(value)) {
      return { ok: false, errors: ["defaultCurrency must be a 3-letter ISO code"] };
    }
    nextCurrency = value;
  }

  const nextAddress = input.address !== undefined ? trimToNull(input.address) : existing.address;
  const nextVatOrCvr = input.vatOrCvr !== undefined ? trimToNull(input.vatOrCvr) : existing.vat_or_cvr;
  const nextEmail = input.email !== undefined ? trimToNull(input.email) : existing.email;
  const nextPhone = input.phone !== undefined ? trimToNull(input.phone) : existing.phone;
  const nextWebsite = input.website !== undefined ? trimToNull(input.website) : existing.website;
  const nextNotes = input.notes !== undefined ? trimToNull(input.notes) : existing.notes;

  db.transaction(() => {
    db.run(
      `UPDATE customers
         SET name = ?, address = ?, vat_or_cvr = ?, email = ?, phone = ?,
             website = ?, ean_number = ?, payment_terms_days = ?,
             default_currency = ?, notes = ?
       WHERE id = ?`,
      [
        nextName,
        nextAddress,
        nextVatOrCvr,
        nextEmail,
        nextPhone,
        nextWebsite,
        nextEan,
        nextPaymentTerms,
        nextCurrency,
        nextNotes,
        id,
      ],
    );

    insertAuditLog(db, {
      eventType: "customer_update",
      entityType: "customer",
      entityId: id,
      message: `Updated customer ${nextName}`,
    });
  }, { immediate: true })();

  return { ok: true, customerId: id, appliedRules: ["DK-MASTER-DATA-CUSTOMER-001"], errors: [] };
}

export type UpdateVendorInput = Partial<Omit<CreateVendorInput, "name">> & {
  name?: string;
};

export function updateVendor(
  db: Database,
  id: number,
  input: UpdateVendorInput,
) {
  const existing = getVendorById(db, id);
  if (!existing) return { ok: false, errors: [`vendor ${id} does not exist`] };

  let nextName = existing.name;
  if (input.name !== undefined) {
    const trimmed = trimToNull(input.name);
    if (!trimmed) return { ok: false, errors: ["name must not be empty"] };
    nextName = trimmed;
  }

  const nextAddress = input.address !== undefined ? trimToNull(input.address) : existing.address;
  const nextVatOrCvr = input.vatOrCvr !== undefined ? trimToNull(input.vatOrCvr) : existing.vat_or_cvr;
  const nextEmail = input.email !== undefined ? trimToNull(input.email) : existing.email;
  const nextPhone = input.phone !== undefined ? trimToNull(input.phone) : existing.phone;
  const nextWebsite = input.website !== undefined ? trimToNull(input.website) : existing.website;
  const nextExpenseAcct = input.defaultExpenseAccount !== undefined ? trimToNull(input.defaultExpenseAccount) : existing.default_expense_account;
  const nextVatTreatment = input.defaultVatTreatment !== undefined ? trimToNull(input.defaultVatTreatment) : existing.default_vat_treatment;
  const nextNotes = input.notes !== undefined ? trimToNull(input.notes) : existing.notes;

  db.transaction(() => {
    db.run(
      `UPDATE vendors
         SET name = ?, address = ?, vat_or_cvr = ?, email = ?, phone = ?,
             website = ?, default_expense_account = ?, default_vat_treatment = ?,
             notes = ?
       WHERE id = ?`,
      [
        nextName,
        nextAddress,
        nextVatOrCvr,
        nextEmail,
        nextPhone,
        nextWebsite,
        nextExpenseAcct,
        nextVatTreatment,
        nextNotes,
        id,
      ],
    );

    insertAuditLog(db, {
      eventType: "vendor_update",
      entityType: "vendor",
      entityId: id,
      message: `Updated vendor ${nextName}`,
    });
  }, { immediate: true })();

  return { ok: true, vendorId: id, appliedRules: ["DK-MASTER-DATA-VENDOR-001"], errors: [] };
}

/** Find a customer by its (vat_or_cvr, name) natural key, or null. */
export function findCustomerByKey(db: Database, vatOrCvr: string | null, name: string) {
  return db.query(
    `SELECT id FROM customers WHERE name = ? AND vat_or_cvr IS ? LIMIT 1`,
  ).get(name, vatOrCvr) as { id: number } | null;
}

/** Find a vendor by its (vat_or_cvr, name) natural key, or null. */
export function findVendorByKey(db: Database, vatOrCvr: string | null, name: string) {
  return db.query(
    `SELECT id FROM vendors WHERE name = ? AND vat_or_cvr IS ? LIMIT 1`,
  ).get(name, vatOrCvr) as { id: number } | null;
}

export function resolveInvoiceMasterData(db: Database, payload: InvoicePayload, options: { customerId?: number | null }) {
  if (!options.customerId) return { ok: true, payload };
  const customer = getCustomerById(db, options.customerId);
  if (!customer || customer.archived) return { ok: false, errors: [`customer ${options.customerId} does not exist`] };
  return {
    ok: true,
    payload: {
      ...payload,
      buyer: {
        name: trimToNull(payload.buyer?.name) ?? customer.name,
        address: trimToNull(payload.buyer?.address) ?? customer.address ?? undefined,
        vatOrCvr: trimToNull(payload.buyer?.vatOrCvr) ?? customer.vat_or_cvr ?? undefined,
        eanNumber: normalizeEanNumber(payload.buyer?.eanNumber) ?? customer.ean_number ?? undefined,
        publicRecipient: payload.buyer?.publicRecipient ?? Boolean(normalizeEanNumber(payload.buyer?.eanNumber) ?? customer.ean_number),
      },
      currency: trimToNull(payload.currency) ?? customer.default_currency,
      dueDate: trimToNull(payload.dueDate) ?? (trimToNull(payload.issueDate) && customer.payment_terms_days > 0 ? addDays(payload.issueDate!, customer.payment_terms_days) : undefined),
    },
  };
}

// ---------------------------------------------------------------------------
// CVR autofill — prefill an unset master-data field from the CVR register.
// The lookup runs once at creation time and the snapshot is copied into the
// customer/vendor row; an explicit caller value always wins over CVR.
// ---------------------------------------------------------------------------

/** A one-line postal address built from a CVR snapshot, or undefined. */
function cvrFullAddress(company: CvrCompanyInfo): string | undefined {
  const cityLine = [company.postalCode, company.city].filter(Boolean).join(" ");
  const full = [company.address, cityLine].filter((part) => part && part.length > 0).join(", ");
  return full.length > 0 ? full : undefined;
}

export type CvrAutofillResult<T> =
  | { ok: true; input: T; company: CvrCompanyInfo }
  | { ok: false; errors: string[] };

/**
 * Resolve a `createCustomer` input by filling every field the caller left
 * unset from a CVR-register lookup. Explicit caller values always win.
 */
export async function customerInputFromCvr(
  db: Database,
  cvrInput: string,
  base: CreateCustomerInput,
  options: CvrLookupOptions = {},
): Promise<CvrAutofillResult<CreateCustomerInput>> {
  const lookup = await lookupCvrCompany(db, cvrInput, options);
  if (!lookup.ok || !lookup.company) return { ok: false, errors: lookup.errors };
  const company = lookup.company;
  return {
    ok: true,
    company,
    input: {
      ...base,
      name: trimToNull(base.name) ?? company.name,
      address: trimToNull(base.address) ?? cvrFullAddress(company),
      vatOrCvr: trimToNull(base.vatOrCvr) ?? `DK${company.cvr}`,
      email: trimToNull(base.email) ?? company.email ?? undefined,
      phone: trimToNull(base.phone) ?? company.phone ?? undefined,
      website: trimToNull(base.website) ?? company.website ?? undefined,
    },
  };
}

/**
 * Resolve a `createVendor` input by filling every field the caller left unset
 * from a CVR-register lookup. Explicit caller values always win.
 */
export async function vendorInputFromCvr(
  db: Database,
  cvrInput: string,
  base: CreateVendorInput,
  options: CvrLookupOptions = {},
): Promise<CvrAutofillResult<CreateVendorInput>> {
  const lookup = await lookupCvrCompany(db, cvrInput, options);
  if (!lookup.ok || !lookup.company) return { ok: false, errors: lookup.errors };
  const company = lookup.company;
  return {
    ok: true,
    company,
    input: {
      ...base,
      name: trimToNull(base.name) ?? company.name,
      address: trimToNull(base.address) ?? cvrFullAddress(company),
      vatOrCvr: trimToNull(base.vatOrCvr) ?? `DK${company.cvr}`,
      email: trimToNull(base.email) ?? company.email ?? undefined,
      phone: trimToNull(base.phone) ?? company.phone ?? undefined,
      website: trimToNull(base.website) ?? company.website ?? undefined,
    },
  };
}

export function resolveDocumentMasterData(db: Database, metadata: DocumentMetadata, options: { vendorId?: number | null }) {
  if (!options.vendorId) return { ok: true, metadata };
  const vendor = getVendorById(db, options.vendorId);
  if (!vendor || vendor.archived) return { ok: false, errors: [`vendor ${options.vendorId} does not exist`] };
  return {
    ok: true,
    metadata: {
      ...metadata,
      sender: {
        name: trimToNull(metadata.sender?.name) ?? vendor.name,
        address: trimToNull(metadata.sender?.address) ?? vendor.address ?? undefined,
        vatOrCvr: trimToNull(metadata.sender?.vatOrCvr) ?? vendor.vat_or_cvr ?? undefined,
      },
    },
  };
}
