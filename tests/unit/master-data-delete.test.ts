// Tests: SMB-ejeren kan slette en fejl-importeret kunde eller leverandør
// fra cockpittet (#430). Sletning er en almindelig master-data mutation
// (kontakter er IKKE append-only — det er kun det bogførte ledger og
// fakturasnapshots). Forretningsregler:
//
//   - Bogførte fakturaer beholder navne-snapshot (de er ikke FK til kunden).
//   - Hvis kunden har en ÅBEN udstedt faktura (status != paid/credited/refunded/written_off):
//     sletningen blokeres med en klar besked + reference til fakturanummeret.
//   - For leverandører er der en `vendor_id` FK i `payables` — hvis der findes
//     en åben gæld med dette vendor_id, blokeres sletningen.
//   - Sletninger audit-logges (event_type `customer_delete` / `vendor_delete`).

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  createCustomer,
  createVendor,
  deleteCustomer,
  deleteVendor,
  listCustomers,
  listVendors,
} from "../../src/core/master-data";

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-md-del-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("deleteCustomer / deleteVendor — #430", () => {
  test("a customer with no open invoices can be deleted", () => {
    const { root, db } = freshDb();
    const created = createCustomer(db, { name: "Fejl-import ApS" });
    const id = (created as { customerId: number }).customerId;

    const deleted = deleteCustomer(db, id);
    expect(deleted.ok).toBe(true);
    expect(listCustomers(db).rows.find((r) => r.id === id)).toBeUndefined();

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a customer-delete writes an audit_log row", () => {
    const { root, db } = freshDb();
    const created = createCustomer(db, { name: "Slet-mig A/S" });
    const id = (created as { customerId: number }).customerId;

    deleteCustomer(db, id);
    const audit = db
      .query(
        `SELECT event_type, entity_type, entity_id, message FROM audit_log
         WHERE event_type = 'customer_delete' ORDER BY id DESC LIMIT 1`,
      )
      .get() as
      | { event_type: string; entity_type: string; entity_id: number; message: string }
      | null;
    expect(audit).not.toBeNull();
    expect(audit!.entity_type).toBe("customer");
    expect(String(audit!.entity_id)).toBe(String(id));
    expect(audit!.message).toContain("Slet-mig A/S");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("deleting a non-existent customer fails with a clear error", () => {
    const { root, db } = freshDb();
    const result = deleteCustomer(db, 9999);
    expect(result.ok).toBe(false);
    expect((result as { errors: string[] }).errors[0]).toMatch(/9999/);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a customer with an open issued invoice cannot be deleted", () => {
    const { root, db } = freshDb();
    const created = createCustomer(db, {
      name: "Aktiv kunde ApS",
      vatOrCvr: "DK12345678",
    });
    const id = (created as { customerId: number }).customerId;

    // Insert a minimal issued-invoice document whose payload buyer matches.
    db.run(
      `INSERT INTO documents (
         document_no, source, sha256_hash, document_type, invoice_no,
         invoice_date, amount_inc_vat, currency, payload_json
       ) VALUES (
         'F-0001', 'cockpit', 'hash-aktiv-kunde-0001', 'issued_invoice',
         'F-0001', '2026-05-01', 1250.00, 'DKK', ?
       )`,
      JSON.stringify({
        buyer: { name: "Aktiv kunde ApS", vatOrCvr: "DK12345678" },
        totals: { grossAmount: 1250.0 },
      }),
    );

    const result = deleteCustomer(db, id);
    expect(result.ok).toBe(false);
    const message = (result as { errors: string[] }).errors.join(" ");
    expect(message).toMatch(/åben/i);
    expect(message).toContain("F-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a customer remains deletable once the invoice has been paid", () => {
    const { root, db } = freshDb();
    const created = createCustomer(db, {
      name: "Betalt-kunde ApS",
      vatOrCvr: "DK87654321",
    });
    const id = (created as { customerId: number }).customerId;

    // Issue + insert a fully-paid invoice via raw SQL — we exercise the
    // status-detect path on the documents+invoice_payments tables.
    db.run(
      `INSERT INTO documents (
         document_no, source, sha256_hash, document_type, invoice_no,
         invoice_date, amount_inc_vat, currency, payload_json
       ) VALUES (
         'F-9001', 'cockpit', 'hash-betalt-kunde-9001', 'issued_invoice',
         'F-9001', '2026-04-01', 500.00, 'DKK', ?
       ) RETURNING id`,
      JSON.stringify({
        buyer: { name: "Betalt-kunde ApS", vatOrCvr: "DK87654321" },
        totals: { grossAmount: 500.0 },
      }),
    );
    const docId = (
      db.query("SELECT id FROM documents WHERE document_no = 'F-9001'").get() as {
        id: number;
      }
    ).id;

    // Minimal balanced journal entry — required as `journal_entry_id` FK
    // for `invoice_payments`. We use an existing or a fresh entry_no.
    db.run(
      `INSERT INTO journal_entries (entry_no, transaction_date, text, document_id, rule_version, entry_hash)
       VALUES ('JE-0001', '2026-04-15', 'Betaling F-9001', ?, 'test', 'hash-je-0001')`,
      docId,
    );
    const jeId = (
      db.query("SELECT id FROM journal_entries WHERE entry_no = 'JE-0001'").get() as {
        id: number;
      }
    ).id;

    db.run(
      `INSERT INTO invoice_payments (invoice_document_id, journal_entry_id, payment_date, amount)
       VALUES (?, ?, '2026-04-15', 500.00)`,
      [docId, jeId],
    );

    const result = deleteCustomer(db, id);
    expect(result.ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a vendor with no open payable can be deleted", () => {
    const { root, db } = freshDb();
    const created = createVendor(db, { name: "Gammel-leverandør ApS" });
    const id = (created as { vendorId: number }).vendorId;

    const deleted = deleteVendor(db, id);
    expect(deleted.ok).toBe(true);
    expect(listVendors(db).rows.find((r) => r.id === id)).toBeUndefined();

    const audit = db
      .query(
        `SELECT entity_type, entity_id FROM audit_log
         WHERE event_type = 'vendor_delete' ORDER BY id DESC LIMIT 1`,
      )
      .get() as { entity_type: string; entity_id: number } | null;
    expect(audit).not.toBeNull();
    expect(String(audit!.entity_id)).toBe(String(id));

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a vendor referenced by an open payable cannot be deleted", () => {
    const { root, db } = freshDb();
    const created = createVendor(db, { name: "Aktiv-leverandør ApS" });
    const id = (created as { vendorId: number }).vendorId;

    // Insert minimum: a document, a balanced journal entry, a payable row
    // with an open balance (no payable_payments yet).
    db.run(
      `INSERT INTO documents (
         document_no, source, sha256_hash, document_type
       ) VALUES (
         'P-0001', 'cockpit', 'hash-aktiv-lev-0001', 'purchase_sale'
       )`,
    );
    const docId = (
      db.query("SELECT id FROM documents WHERE document_no = 'P-0001'").get() as {
        id: number;
      }
    ).id;
    db.run(
      `INSERT INTO journal_entries (entry_no, transaction_date, text, document_id, rule_version, entry_hash)
       VALUES ('JE-9001', '2026-05-01', 'Indkøb fra leverandør', ?, 'test', 'hash-je-9001')`,
      docId,
    );
    const jeId = (
      db.query("SELECT id FROM journal_entries WHERE entry_no = 'JE-9001'").get() as {
        id: number;
      }
    ).id;
    db.run(
      `INSERT INTO payables (document_id, vendor_id, bill_date, due_date,
         gross_amount, net_amount, vat_amount, journal_entry_id)
       VALUES (?, ?, '2026-05-01', '2026-05-31', 1000, 800, 200, ?)`,
      [docId, id, jeId],
    );

    const result = deleteVendor(db, id);
    expect(result.ok).toBe(false);
    expect((result as { errors: string[] }).errors.join(" ")).toMatch(/åben|gæld/i);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
