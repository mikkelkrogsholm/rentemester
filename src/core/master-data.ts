import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import type { InvoicePayload } from "./invoice";
import type { DocumentMetadata } from "./documents";
import { insertAuditLog } from "./actor";
import { addDays } from "./dates";
import { normalizeEanNumber, trimToNull } from "./ean";
import { lookupCvrCompany, type CvrCompanyInfo, type CvrLookupOptions } from "./cvr";
import { resolveSupplierIdentity, type SupplierIdentifierKind } from "./supplier-identity";
import { strengthenGdprErasureAliasesForIdentity } from "./gdpr";

export type CustomerRecord = {
  id: number;
  name: string;
  address: string | null;
  vatOrCvr: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  /** EJER-3: null = ingen eksplicit kundefrist — fakturaen arver virksomhedens profilfrist på fakturatidspunktet. */
  paymentTermsDays: number | null;
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
  countryCode: string | null;
  identifierKind: SupplierIdentifierKind | null;
  identityStatus: "resolved" | "human_resolution_required";
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
  /** EJER-3: udeladt/null = ingen eksplicit frist — kunden arver virksomhedens profilfrist på fakturatidspunktet. */
  paymentTermsDays?: number | null;
  defaultCurrency?: string;
  notes?: string;
};

export type CreateVendorInput = {
  name: string;
  address?: string;
  vatOrCvr?: string;
  countryCode?: string;
  identifierKind?: SupplierIdentifierKind;
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

export function createCustomer(db: Database, input: Partial<CreateCustomerInput>) {
  const name = trimToNull(input.name);
  if (!name) return { ok: false, errors: ["name is required"] };
  const vatOrCvr = trimToNull(input.vatOrCvr);
  const rawEanNumber = trimToNull(input.eanNumber);
  const eanNumber = rawEanNumber ? normalizeEanNumber(rawEanNumber) : null;
  if (rawEanNumber && !eanNumber) return { ok: false, errors: ["eanNumber must be 13 digits"] };
  // EJER-3: no explicit frist → store NULL so the invoice inherits the
  // COMPANY profile's payment terms at invoice time (previously a silent,
  // hardcoded 30 that overrode the owner's own standard without a word).
  // Backward compat: customers created before this change carry a stored 30
  // that cannot be told apart from a deliberate 30 — they keep their 30, and
  // the deviation note in invoice create makes it visible.
  let paymentTermsDays: number | null = null;
  if (input.paymentTermsDays !== undefined && input.paymentTermsDays !== null) {
    const value = Number(input.paymentTermsDays);
    if (!Number.isInteger(value) || value <= 0) {
      return { ok: false, errors: ["paymentTermsDays must be a positive integer"] };
    }
    paymentTermsDays = value;
  }
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
      vatOrCvr,
      trimToNull(input.email),
      trimToNull(input.phone),
      trimToNull(input.website),
      eanNumber,
      paymentTermsDays,
      defaultCurrency,
      trimToNull(input.notes),
    ) as { id: number; created_at: string };

    strengthenGdprErasureAliasesForIdentity(db, {
      name,
      cvr: vatOrCvr,
    });

    insertAuditLog(db, {
      eventType: "customer_create",
      entityType: "customer",
      entityId: row.id,
      message: `Created customer ${name}`,
    });

    return row;
  }).immediate();

  return { ok: true, customerId: inserted.id, appliedRules: ["DK-MASTER-DATA-CUSTOMER-001"], errors: [] };
}

export function listCustomers(db: Database, options: { archived?: boolean } = {}) {
  const rows = db.query(
    `SELECT id, name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes, archived, created_at
     FROM customers
     WHERE archived = CASE WHEN ? THEN archived ELSE 0 END
     ORDER BY lower(name) ASC, id ASC`
  ).all(options.archived ? 1 : 0) as Array<{
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; ean_number: string | null; payment_terms_days: number | null; default_currency: string; notes: string | null; archived: number; created_at: string;
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

export function createVendor(db: Database, input: Partial<CreateVendorInput>) {
  const name = trimToNull(input.name);
  if (!name) return { ok: false, errors: ["name is required"] };
  const identity = input.countryCode !== undefined || input.identifierKind !== undefined
    ? resolveSupplierIdentity({ country: input.countryCode ?? "", identifier: input.vatOrCvr, identifierKind: input.identifierKind })
    : null;
  if (identity && !identity.ok) return { ok: false, status: identity.status, errors: identity.errors };
  const vatOrCvr = identity?.ok ? identity.identifier : trimToNull(input.vatOrCvr);

  const inserted = db.transaction(() => {
    const row = db.query(
      `INSERT INTO vendors (name, address, vat_or_cvr, country_code, identifier_kind, identity_status, email, phone, website, default_expense_account, default_vat_treatment, notes)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       RETURNING id, created_at`
    ).get(
      name,
      trimToNull(input.address),
      vatOrCvr,
      identity?.ok ? identity.country : null,
      identity?.ok ? identity.identifierKind : null,
      identity?.ok ? identity.status : "human_resolution_required",
      trimToNull(input.email),
      trimToNull(input.phone),
      trimToNull(input.website),
      trimToNull(input.defaultExpenseAccount),
      trimToNull(input.defaultVatTreatment),
      trimToNull(input.notes),
    ) as { id: number; created_at: string };

    strengthenGdprErasureAliasesForIdentity(db, {
      name,
      cvr: vatOrCvr,
    });

    insertAuditLog(db, {
      eventType: "vendor_create",
      entityType: "vendor",
      entityId: row.id,
      message: `Created vendor ${name}`,
    });

    return row;
  }).immediate();

  return { ok: true, vendorId: inserted.id, appliedRules: ["DK-MASTER-DATA-VENDOR-001"], errors: [] };
}

export function listVendors(db: Database, options: { archived?: boolean } = {}) {
  const rows = db.query(
    `SELECT id, name, address, vat_or_cvr, country_code, identifier_kind, identity_status, email, phone, website, default_expense_account, default_vat_treatment, notes, archived, created_at
     FROM vendors
     WHERE archived = CASE WHEN ? THEN archived ELSE 0 END
     ORDER BY lower(name) ASC, id ASC`
  ).all(options.archived ? 1 : 0) as Array<{
    id: number; name: string; address: string | null; vat_or_cvr: string | null; country_code: string | null; identifier_kind: SupplierIdentifierKind | null; identity_status: "resolved" | "human_resolution_required"; email: string | null; phone: string | null; website: string | null; default_expense_account: string | null; default_vat_treatment: string | null; notes: string | null; archived: number; created_at: string;
  }>;

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      name: row.name,
      address: row.address,
      vatOrCvr: row.vat_or_cvr,
      countryCode: row.country_code,
      identifierKind: row.identifier_kind,
      identityStatus: row.identity_status,
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
    id: number; name: string; address: string | null; vat_or_cvr: string | null; email: string | null; phone: string | null; website: string | null; ean_number: string | null; payment_terms_days: number | null; default_currency: string; notes: string | null; archived: number; created_at: string;
  } | null;
}

export function getVendorById(db: Database, id: number) {
  return db.query(
    `SELECT id, name, address, vat_or_cvr, country_code, identifier_kind, identity_status, email, phone, website, default_expense_account, default_vat_treatment, notes, archived, created_at
     FROM vendors WHERE id = ? LIMIT 1`
  ).get(id) as {
    id: number; name: string; address: string | null; vat_or_cvr: string | null; country_code: string | null; identifier_kind: SupplierIdentifierKind | null; identity_status: "resolved" | "human_resolution_required"; email: string | null; phone: string | null; website: string | null; default_expense_account: string | null; default_vat_treatment: string | null; notes: string | null; archived: number; created_at: string;
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
    // EJER-3: an explicit null clears the kundefrist back to "inherit the
    // company profile's terms at invoice time" — same null-clears convention
    // as the other optional fields above.
    if (input.paymentTermsDays === null) {
      nextPaymentTerms = null;
    } else {
      const value = Number(input.paymentTermsDays);
      if (!Number.isInteger(value) || value <= 0) {
        return { ok: false, errors: ["paymentTermsDays must be a positive integer"] };
      }
      nextPaymentTerms = value;
    }
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
    runSql(db,
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

    strengthenGdprErasureAliasesForIdentity(db, {
      name: nextName,
      cvr: nextVatOrCvr,
    });

    insertAuditLog(db, {
      eventType: "customer_update",
      entityType: "customer",
      entityId: id,
      message: `Updated customer ${nextName}`,
    });
  }).immediate();

  return { ok: true, customerId: id, appliedRules: ["DK-MASTER-DATA-CUSTOMER-001"], errors: [] };
}

/**
 * Find issued invoices that reference a given customer by buyer.name +
 * buyer.vatOrCvr snapshot (the snapshot is the only link — kontakter er IKKE
 * en FK på fakturasnapshots — sletning er derfor ikke en data-corruption
 * risiko for historikken). Returns the open invoices that should block a
 * delete: an invoice counts as "open" when its status is `open`, `overdue`
 * or `overpaid` — we lean on the same status derivation the cockpit shows
 * on the invoice-listen, so the human's mental model lines up.
 *
 * Used by `deleteCustomer` (#430) to refuse the delete and surface a clear
 * "kontakten er i brug på en åben faktura: <nummer>" message.
 */
function findOpenIssuedInvoicesForCustomer(
  db: Database,
  customer: { name: string; vatOrCvr: string | null },
): Array<{ invoiceNo: string }> {
  // Pull every issued-invoice document; the buyer match is done in JS
  // because the buyer block lives in `payload_json` (a snapshot — there is
  // no FK back to customers, by design). The cost is bounded by the
  // company's invoice count, which is small for an SMB.
  const rows = db
    .query(
      `SELECT id, invoice_no, payload_json
       FROM documents
       WHERE document_type = 'issued_invoice'`,
    )
    .all() as Array<{
      id: number;
      invoice_no: string;
      payload_json: string | null;
    }>;

  const wantedName = customer.name.trim().toLowerCase();
  const wantedCvr = (customer.vatOrCvr ?? "").trim().toUpperCase();

  const matches: Array<{ invoiceNo: string; documentId: number }> = [];
  for (const row of rows) {
    if (!row.payload_json) continue;
    let payload: { buyer?: { name?: string; vatOrCvr?: string } } | null = null;
    try {
      payload = JSON.parse(row.payload_json) as {
        buyer?: { name?: string; vatOrCvr?: string };
      };
    } catch {
      continue;
    }
    const buyerName = (payload?.buyer?.name ?? "").trim().toLowerCase();
    const buyerCvr = (payload?.buyer?.vatOrCvr ?? "").trim().toUpperCase();
    // A match is either (vat/cvr matches when both have one) OR (names match
    // when no vat/cvr is set on either side). Mirrors how a human would
    // recognise "same kunde" in the cockpit.
    const matchByCvr = wantedCvr.length > 0 && buyerCvr === wantedCvr;
    const matchByName =
      wantedCvr.length === 0 && buyerCvr.length === 0 && buyerName === wantedName;
    if (matchByCvr || matchByName) {
      matches.push({ invoiceNo: row.invoice_no, documentId: row.id });
    }
  }

  // For each matched invoice, check whether the invoice is fully settled.
  // "Open" is defined by the absence of a balancing payment / credit /
  // refund / write-off: gross > sum of payments. We use the raw tables
  // here (rather than the rich `getInvoiceStatus` helper) to keep this
  // function dependency-free and side-effect-free.
  const open: Array<{ invoiceNo: string }> = [];
  for (const match of matches) {
    const settled = (db
      .query(
        `SELECT
           COALESCE((SELECT SUM(amount) FROM invoice_payments WHERE invoice_document_id = ?), 0) AS paid,
           COALESCE((SELECT SUM(amount) FROM invoice_refunds WHERE invoice_document_id = ?), 0) AS refunded,
           COALESCE((SELECT SUM(gross_amount) FROM invoice_bad_debt_writeoffs WHERE invoice_document_id = ?), 0) AS written_off,
           (SELECT amount_inc_vat FROM documents WHERE id = ?) AS gross`,
      )
      .get(match.documentId, match.documentId, match.documentId, match.documentId) as
      | { paid: number; refunded: number; written_off: number; gross: number }
      | null);
    if (!settled) continue;
    const gross = Number(settled.gross ?? 0);
    const closed = Number(settled.paid ?? 0) + Number(settled.refunded ?? 0) + Number(settled.written_off ?? 0);
    if (gross > closed + 0.005) {
      open.push({ invoiceNo: match.invoiceNo });
    }
  }
  return open;
}

/**
 * Slet en kunde fra master data (#430). En fejl-importeret eller dubleret
 * kunde skal kunne fjernes fra cockpittet — ikke kun fra CLI'en. Vi blokerer
 * sletningen hvis kunden er i brug på en åben (ikke-betalt) udstedt faktura
 * og giver et klart navngivet fakturanummer tilbage, så ejeren ved hvor
 * problemet ligger.
 *
 * Bogførte fakturaer påvirkes IKKE: buyer-feltet er et snapshot i
 * `documents.payload_json`, ikke en FK — den fortsatte revisions-eksport og
 * det historiske ledger forbliver intakt. Sletningen audit-logges.
 */
export function deleteCustomer(db: Database, id: number) {
  const existing = getCustomerById(db, id);
  if (!existing) return { ok: false, errors: [`customer ${id} does not exist`] };

  const openInvoices = findOpenIssuedInvoicesForCustomer(db, {
    name: existing.name,
    vatOrCvr: existing.vat_or_cvr,
  });
  if (openInvoices.length > 0) {
    const list = openInvoices.map((inv) => inv.invoiceNo).join(", ");
    return {
      ok: false,
      errors: [
        `Kunden er i brug på åben faktura: ${list}. Bogfør betalingen eller udsted en kreditnota før kunden kan slettes.`,
      ],
      openInvoices,
    };
  }

  db.transaction(() => {
    runSql(db, `DELETE FROM customers WHERE id = ?`, id);
    insertAuditLog(db, {
      eventType: "customer_delete",
      entityType: "customer",
      entityId: id,
      message: `Deleted customer ${existing.name}`,
    });
  }).immediate();

  return { ok: true, customerId: id, errors: [] };
}

/**
 * Slet en leverandør (#430). En leverandør med en åben gæld i `payables`
 * blokeres med en klar besked — payables har en `vendor_id` FK, så vi kan
 * detektere "er i brug" direkte uden at gætte på navne-snapshots.
 * Historiske bogførte bilag har deres `sender_*`-felter som snapshot i
 * `documents`, så de påvirkes ikke.
 */
export function deleteVendor(db: Database, id: number) {
  const existing = getVendorById(db, id);
  if (!existing) return { ok: false, errors: [`vendor ${id} does not exist`] };

  // A payable is "open" when its gross > the sum of applied payable_payments.
  const openPayables = db
    .query(
      `SELECT p.id, p.bill_no, p.gross_amount,
              COALESCE((SELECT SUM(amount) FROM payable_payments pp WHERE pp.payable_id = p.id), 0) AS paid
         FROM payables p
        WHERE p.vendor_id = ?`,
    )
    .all(id) as Array<{ id: number; bill_no: string | null; gross_amount: number; paid: number }>;
  const stillOpen = openPayables.filter(
    (row) => Number(row.gross_amount) > Number(row.paid) + 0.005,
  );
  if (stillOpen.length > 0) {
    const list = stillOpen
      .map((row) => row.bill_no ?? `payable #${row.id}`)
      .join(", ");
    return {
      ok: false,
      errors: [
        `Leverandøren er i brug på åben gæld: ${list}. Bogfør betalingen før leverandøren kan slettes.`,
      ],
      openPayables: stillOpen,
    };
  }

  db.transaction(() => {
    runSql(db, `DELETE FROM vendors WHERE id = ?`, id);
    insertAuditLog(db, {
      eventType: "vendor_delete",
      entityType: "vendor",
      entityId: id,
      message: `Deleted vendor ${existing.name}`,
    });
  }).immediate();

  return { ok: true, vendorId: id, errors: [] };
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
  const nextCountryCode = input.countryCode !== undefined ? input.countryCode : existing.country_code ?? undefined;
  const nextIdentifierKind = input.identifierKind !== undefined ? input.identifierKind : existing.identifier_kind ?? undefined;
  const identity = nextCountryCode !== undefined || nextIdentifierKind !== undefined
    ? resolveSupplierIdentity({ country: nextCountryCode ?? "", identifier: nextVatOrCvr ?? undefined, identifierKind: nextIdentifierKind })
    : null;
  if (identity && !identity.ok) return { ok: false, status: identity.status, errors: identity.errors };
  const nextEmail = input.email !== undefined ? trimToNull(input.email) : existing.email;
  const nextPhone = input.phone !== undefined ? trimToNull(input.phone) : existing.phone;
  const nextWebsite = input.website !== undefined ? trimToNull(input.website) : existing.website;
  const nextExpenseAcct = input.defaultExpenseAccount !== undefined ? trimToNull(input.defaultExpenseAccount) : existing.default_expense_account;
  const nextVatTreatment = input.defaultVatTreatment !== undefined ? trimToNull(input.defaultVatTreatment) : existing.default_vat_treatment;
  const nextNotes = input.notes !== undefined ? trimToNull(input.notes) : existing.notes;
  const resolvedVatOrCvr = identity?.ok ? identity.identifier : nextVatOrCvr;

  db.transaction(() => {
    runSql(db,
      `UPDATE vendors
         SET name = ?, address = ?, vat_or_cvr = ?, country_code = ?, identifier_kind = ?, identity_status = ?, email = ?, phone = ?,
             website = ?, default_expense_account = ?, default_vat_treatment = ?,
             notes = ?
       WHERE id = ?`,
      [
        nextName,
        nextAddress,
        resolvedVatOrCvr,
        identity?.ok ? identity.country : existing.country_code,
        identity?.ok ? identity.identifierKind : existing.identifier_kind,
        identity?.ok ? identity.status : existing.identity_status,
        nextEmail,
        nextPhone,
        nextWebsite,
        nextExpenseAcct,
        nextVatTreatment,
        nextNotes,
        id,
      ],
    );

    strengthenGdprErasureAliasesForIdentity(db, {
      name: nextName,
      cvr: resolvedVatOrCvr,
    });

    insertAuditLog(db, {
      eventType: "vendor_update",
      entityType: "vendor",
      entityId: id,
      message: `Updated vendor ${nextName}`,
    });
  }).immediate();

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

/**
 * EJER-3: the company profile's own default payment terms, or null when the
 * ledger has no companies row (or predates the column). Queried directly
 * instead of importing `getCompanySettings` to keep master-data free of a
 * company.ts dependency.
 */
function companyDefaultPaymentTermsDays(db: Database): number | null {
  try {
    const row = db.query(`SELECT payment_terms_days FROM companies ORDER BY id ASC LIMIT 1`).get() as
      | { payment_terms_days: number | null }
      | null;
    if (!row) return null;
    const days = Number(row.payment_terms_days);
    return Number.isInteger(days) && days >= 0 && days <= 365 ? days : null;
  } catch {
    return null;
  }
}

export function resolveInvoiceMasterData(
  db: Database,
  payload: InvoicePayload,
  options: { customerId?: number | null },
): { ok: true; payload: InvoicePayload; notes?: string[] } | { ok: false; errors: string[] } {
  if (!options.customerId) return { ok: true, payload };
  const customer = getCustomerById(db, options.customerId);
  if (!customer || customer.archived) return { ok: false, errors: [`customer ${options.customerId} does not exist`] };

  // EJER-3: only an EXPLICIT kundefrist sets the due date here. A customer
  // stored with NULL has no own frist — the due date is left unset so
  // issueInvoice's enrichInvoiceFromCompany fills it from the company
  // profile's payment terms (the owner's own standard).
  const explicitDueDate = trimToNull(payload.dueDate);
  const customerTerms = customer.payment_terms_days;
  const dueDate =
    explicitDueDate ??
    (trimToNull(payload.issueDate) && customerTerms != null && customerTerms > 0
      ? addDays(payload.issueDate!, customerTerms)
      : undefined);

  // EJER-3: when the kundekort's explicit frist (not the company standard)
  // decides the due date AND deviates from the profile, say so out loud —
  // the owner must never discover a silently different betalingsfrist.
  const notes: string[] = [];
  if (!explicitDueDate && dueDate !== undefined && customerTerms != null) {
    const companyTerms = companyDefaultPaymentTermsDays(db);
    if (companyTerms != null && companyTerms !== customerTerms) {
      notes.push(
        `Betalingsfrist ${customerTerms} dage fra kundekortet — virksomhedens standard er ${companyTerms} dage.`,
      );
    }
  }

  return {
    ok: true,
    payload: {
      ...payload,
      buyer: {
        buyerReference: trimToNull(payload.buyer?.buyerReference) ?? undefined,
        orderReference: trimToNull(payload.buyer?.orderReference) ?? undefined,
        name: trimToNull(payload.buyer?.name) ?? customer.name,
        address: trimToNull(payload.buyer?.address) ?? customer.address ?? undefined,
        vatOrCvr: trimToNull(payload.buyer?.vatOrCvr) ?? customer.vat_or_cvr ?? undefined,
        eanNumber: normalizeEanNumber(payload.buyer?.eanNumber) ?? customer.ean_number ?? undefined,
        publicRecipient: payload.buyer?.publicRecipient ?? Boolean(normalizeEanNumber(payload.buyer?.eanNumber) ?? customer.ean_number),
      },
      currency: trimToNull(payload.currency) ?? customer.default_currency,
      dueDate,
    },
    ...(notes.length > 0 ? { notes } : {}),
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
  base: Partial<CreateCustomerInput>,
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
  base: Partial<CreateVendorInput>,
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

export function resolveDocumentMasterData(
  db: Database,
  metadata: DocumentMetadata,
  options: { vendorId?: number | null },
): { ok: true; metadata: DocumentMetadata } | { ok: false; errors: string[] } {
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
        countryCode: metadata.sender?.countryCode ?? vendor.country_code ?? undefined,
        identifierKind: metadata.sender?.identifierKind ?? vendor.identifier_kind ?? undefined,
      },
    },
  };
}
