import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { ingestDocument, validateDocumentMetadata, type DocumentMetadata } from "../../src/core/documents";
import { setDocumentCompanyContext, validSimplifiedPurchaseCompanyContext } from "../../src/core/document-company-context";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { bookExpenseFromBank } from "../../src/core/expense-booking";
import { registerPayable } from "../../src/core/payables";
import { reviewIncompleteStandardPurchaseVatEvidence, validIncompleteStandardPurchaseVatEvidenceReview } from "../../src/core/document-purchase-vat-evidence-review";

const simplifiedMetadata: DocumentMetadata = {
  source: "email",
  documentType: "purchase_sale",
  issueDate: "2026-08-20",
  invoiceNo: "SYN-570-1",
  deliveryDescription: "Synthetic office supplies",
  amountIncVat: 125,
  currency: "DKK",
  sender: { name: "Synthetic Supplier ApS", address: "Supplier Street 1", vatOrCvr: "DK11223344" },
  recipient: { name: "Printed Individual", address: "Personal Street 2" },
  vatAmount: 25,
  danishSimplifiedPurchaseInvoice: true,
};

function setup(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.run(`INSERT INTO companies
    (id, name, country, currency, cvr, address, postal_code, city, vat_period_type)
    VALUES (1, 'Synthetic Buyer ApS', 'DK', 'DKK', 'DK12345678', 'Business Street 3', '1000', 'Testby', 'quarter')`);
  const file = join(inbox, "simplified.txt");
  writeFileSync(file, "Synthetic simplified invoice\n125 DKK\n");
  const document = ingestDocument(db, root, file, simplifiedMetadata, {
    createdBy: "agent:test",
    createdByProgram: "test",
  });
  expect(document).toMatchObject({ ok: true });
  return { root, inbox, db, documentId: Number(document.documentId) };
}

function close(fixture: ReturnType<typeof setup>) {
  fixture.db.close();
  rmSync(fixture.root, { recursive: true, force: true });
  rmSync(fixture.inbox, { recursive: true, force: true });
}

function recordContext(fixture: ReturnType<typeof setup>) {
  return setDocumentCompanyContext(fixture.db, {
    documentId: fixture.documentId,
    sourceReference: "approval:SYN-570",
    businessUseReason: "Used exclusively for the synthetic company's activity",
    confirm: true,
    createdBy: "agent:test",
    createdByProgram: "unit-test",
  });
}

function accountingEvidenceSnapshot(db: ReturnType<typeof openDb>) {
  return {
    documents: db.query("SELECT * FROM documents ORDER BY id").all(),
    bank: db.query("SELECT * FROM bank_transactions ORDER BY id").all(),
    journals: db.query("SELECT * FROM journal_entries ORDER BY id").all(),
    lines: db.query("SELECT * FROM journal_lines ORDER BY id").all(),
    audit: db.query("SELECT * FROM audit_log ORDER BY id").all(),
    payables: db.query("SELECT * FROM payables ORDER BY id").all(),
    sequences: db.query("SELECT * FROM sequences ORDER BY kind, scope").all(),
    contexts: db.query("SELECT * FROM document_company_contexts ORDER BY document_id").all(),
    reviews: db.query("SELECT * FROM document_purchase_vat_evidence_reviews ORDER BY id").all(),
  };
}

describe("auditable company context for simplified Danish purchase invoices (#570)", () => {
  for (const workflow of ["expense", "payable"] as const) {
    for (const scenario of ["supplier", "context-and-rate", "printed-recipient-and-rate", "reviewed", "tampered-payment"] as const) {
      test(`incomplete purchase preserves ${workflow} gates for ${scenario}`, () => {
        const fixture = setup(`incomplete-order-${workflow}-${scenario}`);
        try {
          const { db } = fixture;
          const metadata: DocumentMetadata = {
            ...simplifiedMetadata,
            invoiceNo: "SYN-INCOMPLETE-ORDER",
            amountIncVat: 3750,
            vatAmount: 750,
            danishSimplifiedPurchaseInvoice: false,
            incompleteStandardPurchaseInvoice: true,
            recipient: { name: "Synthetic Individual", address: "Personal Street 2" },
          };
          const file = join(fixture.inbox, "incomplete-order.txt");
          writeFileSync(file, "Synthetic incomplete invoice\n3750 DKK\n");
          const document = ingestDocument(db, fixture.root, file, metadata, { createdBy: "agent:test", createdByProgram: "test" });
          expect(document.ok).toBe(true);
          const documentId = document.documentId!;
          const bankId = Number((db.query(`INSERT INTO bank_transactions
            (transaction_date,text,amount,currency,transaction_hash,status)
            VALUES('2026-08-20','Synthetic company purchase',-3750,'DKK','bank-incomplete-order','imported') RETURNING id`).get() as { id: number }).id);

          if (scenario === "supplier") db.run("UPDATE documents SET sender_vat_cvr=NULL,supplier_identity_status='human_resolution_required' WHERE id=?", documentId);
          if (scenario === "context-and-rate" || scenario === "printed-recipient-and-rate") db.run("UPDATE documents SET vat_amount=751 WHERE id=?", documentId);
          if (scenario === "printed-recipient-and-rate") db.run("UPDATE documents SET recipient_vat_cvr='DK12345678' WHERE id=?", documentId);
          if (scenario === "reviewed" || scenario === "tampered-payment") {
            expect(setDocumentCompanyContext(db, {
              documentId, sourceReference: "supplier-email:SYN-INCOMPLETE-ORDER",
              businessUseReason: "Synthetic equipment used in taxable activity", confirm: true,
              createdBy: "user:reviewer", createdByProgram: "unit-test",
            }).ok).toBe(true);
            expect(reviewIncompleteStandardPurchaseVatEvidence(db, {
              documentId, bankTransactionId: bankId,
              businessEvidenceReference: "purchase-order:SYN-INCOMPLETE-ORDER", businessEvidenceSha256: "a".repeat(64),
              rationale: "Formal buyer-field deficiency only; payment and business use reviewed",
              principal: "service-account:synthetic-reviewer", confirm: true,
              createdBy: "user:reviewer", createdByProgram: "unit-test",
            }).ok).toBe(true);
            expect(validIncompleteStandardPurchaseVatEvidenceReview(db, documentId)).toBe(true);
            if (scenario === "tampered-payment") {
              db.run("UPDATE bank_transactions SET transaction_hash='tampered-evidence' WHERE id=?", bankId);
              expect(validIncompleteStandardPurchaseVatEvidenceReview(db, documentId)).toBe(false);
            }
          }

          const before = accountingEvidenceSnapshot(db);
          const result = workflow === "expense"
            ? bookExpenseFromBank(db, { documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" })
            : registerPayable(db, { documentId, billDate: "2026-08-20", dueDate: "2026-09-03", expenseAccountNo: "3000", vatTreatment: "standard" });
          if (scenario === "reviewed") {
            expect(result.ok).toBe(true);
            expect(result.errors).toEqual([]);
            expect(db.query("SELECT COUNT(*) AS n FROM journal_entries WHERE document_id=?").get(documentId)).toEqual({ n: 1 });
            expect(verifyAuditChain(db).ok).toBe(true);
          } else {
            expect(result.ok).toBe(false);
            expect(result.appliedRules).toEqual(workflow === "expense" ? [] : ["DK-PAYABLE-001"]);
            if (scenario === "supplier") {
              expect(result.errors).toEqual(["deductible Danish purchase VAT requires a resolved Danish supplier identity; use human resolution or non_deductible instead"]);
            } else if (workflow === "expense") {
              expect(result.errors).toEqual(["incomplete standard invoice requires a valid hash-bound VAT evidence review before input-VAT deduction"]);
            } else if (scenario === "printed-recipient-and-rate") {
              expect(result.errors).toEqual([`document ${documentId} vat_amount 751 is inconsistent with the 25% rate (expected ~749.75 for net 2999)`]);
            } else {
              expect(result.errors).toEqual(["standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context"]);
            }
            expect(accountingEvidenceSnapshot(db)).toEqual(before);
          }
        } finally { close(fixture); }
      });
    }
  }

  test("purchase workflows preserve first rejection and make no accounting changes for overlapping invalid facts", () => {
    const supplierError = "deductible Danish purchase VAT requires a resolved Danish supplier identity; use human resolution or non_deductible instead";
    const contextError = "standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context";
    for (const workflow of ["expense", "payable"] as const) {
      for (const scenario of ["supplier-and-context", "context-and-rate", "supplier-and-json", "context-and-split", "zero-vat"] as const) {
        const fixture = setup(`purchase-order-${workflow}-${scenario}`);
        try {
          const { db, documentId } = fixture;
          if (scenario.startsWith("supplier")) db.run("UPDATE documents SET sender_vat_cvr = NULL, supplier_identity_status = 'human_resolution_required' WHERE id = ?", documentId);
          if (scenario === "supplier-and-context" || scenario === "context-and-rate") db.run("UPDATE documents SET vat_amount = 26 WHERE id = ?", documentId);
          if (scenario === "supplier-and-json") db.run("UPDATE documents SET payload_json = '{' WHERE id = ?", documentId);
          if (scenario === "context-and-split") db.run("UPDATE documents SET payload_json = ? WHERE id = ?", JSON.stringify({ purchaseVatLines: "invalid" }), documentId);
          if (scenario === "zero-vat") db.run("UPDATE documents SET vat_amount = 0, recipient_vat_cvr = 'DK12345678' WHERE id = ?", documentId);
          const bankId = Number((db.query(`INSERT INTO bank_transactions
            (transaction_date, text, amount, currency, transaction_hash, status)
            VALUES ('2026-08-20', 'Synthetic purchase', -125, 'DKK', 'bank-synthetic-order', 'imported') RETURNING id`).get() as { id: number }).id);
          const snapshot = () => ({
            journals: db.query("SELECT * FROM journal_entries ORDER BY id").all(),
            audit: db.query("SELECT * FROM audit_log ORDER BY id").all(),
            payables: db.query("SELECT * FROM payables ORDER BY id").all(),
            sequences: db.query("SELECT * FROM sequences ORDER BY kind, scope").all(),
          });
          const before = snapshot();
          const result = workflow === "expense"
            ? bookExpenseFromBank(db, { documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" })
            : registerPayable(db, { documentId, billDate: "2026-08-20", dueDate: "2026-09-03", expenseAccountNo: "3000", vatTreatment: "standard" });
          expect(result.ok).toBe(false);
          expect(result.appliedRules).toEqual(workflow === "expense" ? [] : ["DK-PAYABLE-001"]);
          if (scenario === "supplier-and-json" && workflow === "payable") {
            expect(result.errors).toEqual([`document ${documentId} has invalid persisted purchaseVatLines: document payload_json is not valid JSON`]);
          } else if (scenario.startsWith("supplier")) {
            expect(result.errors).toEqual([supplierError]);
          } else if (scenario === "context-and-split" && workflow === "payable") {
            expect(result.errors[0]).toContain(`document ${documentId} has invalid persisted purchaseVatLines:`);
          } else if (scenario === "zero-vat") {
            expect(result.errors).toEqual([workflow === "expense"
              ? `document ${documentId} vat_amount 0 is inconsistent with the 25% rate (expected ~31.25 for net 125)`
              : "standard payable registration requires document vat_amount > 0"]);
          } else {
            expect(result.errors).toEqual([contextError]);
          }
          expect(snapshot()).toEqual(before);
        } finally { close(fixture); }
      }
    }
  });

  test("accepts the reduced field set, preserves the printed recipient and keeps company identity separate", () => {
    expect(validateDocumentMetadata(simplifiedMetadata)).toMatchObject({ ok: true });
    expect(validateDocumentMetadata({ ...simplifiedMetadata, danishSimplifiedPurchaseInvoice: false }).ok).toBe(true);
    expect(validateDocumentMetadata({ ...simplifiedMetadata, amountIncVat: 3000.01 }).ok).toBe(false);
    expect(validateDocumentMetadata({ ...simplifiedMetadata, invoiceNo: undefined }).ok).toBe(false);

    const fixture = setup("company-context-preserve");
    try {
      const before = fixture.db.query(`SELECT recipient_name, recipient_address, recipient_vat_cvr, sha256_hash,
        stored_path, original_filename, payload_json FROM documents WHERE id = ?`).get(fixture.documentId) as Record<string, unknown>;
      expect(before).toMatchObject({ recipient_name: "Printed Individual", recipient_address: "Personal Street 2", recipient_vat_cvr: null });
      expect(recordContext(fixture)).toMatchObject({ ok: true, applied: true, documentId: fixture.documentId });
      const after = fixture.db.query(`SELECT recipient_name, recipient_address, recipient_vat_cvr, sha256_hash,
        stored_path, original_filename, payload_json FROM documents WHERE id = ?`).get(fixture.documentId);
      expect(after).toEqual(before);

      const context = fixture.db.query(`SELECT company_id, company_snapshot_json, actor, program,
        document_sha256, payload_sha256, context_sha256 FROM document_company_contexts WHERE document_id = ?`).get(fixture.documentId) as Record<string, unknown>;
      expect(context).toMatchObject({ company_id: 1, actor: "agent:test", program: "unit-test" });
      expect(JSON.parse(String(context.company_snapshot_json))).toMatchObject({ cvr: "DK12345678", name: "Synthetic Buyer ApS" });
      expect(JSON.parse(String(before.payload_json)).recipient).toEqual({ name: "Printed Individual", address: "Personal Street 2" });
      expect(String(before.payload_json)).not.toContain("DK12345678");
      expect(validSimplifiedPurchaseCompanyContext(fixture.db, fixture.documentId)).toBe(true);
      expect(fixture.db.query("SELECT event_type, actor FROM audit_log WHERE entity_id = ? AND event_type = 'document_company_context_set'").get(fixture.documentId))
        .toEqual({ event_type: "document_company_context_set", actor: "agent:test via unit-test" });
      expect(verifyAuditChain(fixture.db).ok).toBe(true);
    } finally { close(fixture); }
  });

  test("requires confirmation, is idempotent, rejects conflicts and is append-only", () => {
    const fixture = setup("company-context-idempotent");
    try {
      expect(setDocumentCompanyContext(fixture.db, {
        documentId: fixture.documentId, sourceReference: "approval:SYN-570", businessUseReason: "Synthetic use", confirm: false,
      }).errors).toContain("document company context requires explicit confirm: true");
      expect(recordContext(fixture)).toMatchObject({ ok: true, applied: true });
      expect(recordContext(fixture)).toMatchObject({ ok: true, applied: false });
      expect(setDocumentCompanyContext(fixture.db, {
        documentId: fixture.documentId,
        sourceReference: "approval:DIFFERENT",
        businessUseReason: "Used exclusively for the synthetic company's activity",
        confirm: true,
        createdBy: "agent:test",
        createdByProgram: "unit-test",
      })).toMatchObject({ ok: false, errors: ["document company context already exists with conflicting evidence"] });
      expect(() => fixture.db.run("UPDATE document_company_contexts SET source_reference = 'changed' WHERE document_id = ?", fixture.documentId)).toThrow("append-only");
      expect(() => fixture.db.run("DELETE FROM document_company_contexts WHERE document_id = ?", fixture.documentId)).toThrow("append-only");
      expect(fixture.db.query("SELECT COUNT(*) AS n FROM document_company_contexts").get()).toEqual({ n: 1 });
      expect(fixture.db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'document_company_context_set'").get()).toEqual({ n: 1 });
    } finally { close(fixture); }
  });

  test("records company attribution for a truthfully incomplete standard invoice without changing its source facts", () => {
    const incomplete: DocumentMetadata = { ...simplifiedMetadata, invoiceNo: "SYN-618-1", danishSimplifiedPurchaseInvoice: false, incompleteStandardPurchaseInvoice: true, recipient: { name: "Synthetic Buyer ApS" } };
    const fixture = setup("incomplete-company-context");
    try {
      const file = join(fixture.inbox, "incomplete.txt"); writeFileSync(file, "Synthetic incomplete invoice\n125 DKK\n");
      const document = ingestDocument(fixture.db, fixture.root, file, incomplete, { createdBy: "agent:test", createdByProgram: "test" });
      expect(document.ok).toBe(true);
      expect(setDocumentCompanyContext(fixture.db, { documentId: document.documentId!, sourceReference: "supplier-email:SYN-618", businessUseReason: "Synthetic company purchase", confirm: true, createdBy: "agent:test", createdByProgram: "unit-test" })).toMatchObject({ ok: true, applied: true });
      expect(fixture.db.query("SELECT recipient_address,recipient_vat_cvr FROM documents WHERE id=?").get(document.documentId!)).toEqual({ recipient_address: null, recipient_vat_cvr: null });
    } finally { close(fixture); }
  });

  test("rejects posted documents and fails closed if either immutable binding changes", () => {
    const posted = setup("company-context-linked");
    try {
      posted.db.run(`INSERT INTO journal_entries
        (document_id, entry_no, transaction_date, text, rule_version, entry_hash)
        VALUES (?, 'J-SYN-570', '2026-08-20', 'Synthetic', 'test', 'synthetic-hash')`, posted.documentId);
      expect(recordContext(posted)).toMatchObject({ ok: false, errors: ["document is linked to accounting evidence"] });
    } finally { close(posted); }

    const tampered = setup("company-context-tamper");
    try {
      expect(recordContext(tampered).ok).toBe(true);
      tampered.db.run("UPDATE documents SET amount_inc_vat = 126 WHERE id = ?", tampered.documentId);
      expect(validSimplifiedPurchaseCompanyContext(tampered.db, tampered.documentId)).toBe(false);
      tampered.db.run("UPDATE documents SET amount_inc_vat = 125 WHERE id = ?", tampered.documentId);
      expect(validSimplifiedPurchaseCompanyContext(tampered.db, tampered.documentId)).toBe(true);
      const payload = tampered.db.query("SELECT payload_json FROM documents WHERE id = ?").get(tampered.documentId) as { payload_json: string };
      tampered.db.run("UPDATE documents SET payload_json = ? WHERE id = ?", `${payload.payload_json} `, tampered.documentId);
      expect(validSimplifiedPurchaseCompanyContext(tampered.db, tampered.documentId)).toBe(false);
    } finally { close(tampered); }
  });

  test("standard VAT is denied without context and accepted through both booking paths with valid context", () => {
    const expense = setup("company-context-expense");
    try {
      const bankId = Number((expense.db.query(`INSERT INTO bank_transactions
        (transaction_date, text, amount, currency, transaction_hash, status)
        VALUES ('2026-08-20', 'Synthetic purchase', -125, 'DKK', 'bank-syn-570', 'imported') RETURNING id`).get() as { id: number }).id);
      const denied = bookExpenseFromBank(expense.db, { documentId: expense.documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" });
      expect(denied.errors).toContain("standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context");
      expect(recordContext(expense).ok).toBe(true);
      expect(bookExpenseFromBank(expense.db, { documentId: expense.documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" }).ok).toBe(true);
    } finally { close(expense); }

    const payable = setup("company-context-payable");
    try {
      const input = { documentId: payable.documentId, billDate: "2026-08-20", dueDate: "2026-09-03", expenseAccountNo: "3000", vatTreatment: "standard" as const };
      expect(registerPayable(payable.db, input).errors).toContain("standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context");
      expect(recordContext(payable).ok).toBe(true);
      expect(registerPayable(payable.db, input).ok).toBe(true);
    } finally { close(payable); }
  });

  test("#622 allows a formally deficient standard invoice only after hash-bound payment and business-evidence review", () => {
    const fixture = setup("formal-deficiency-review");
    try {
      const metadata: DocumentMetadata = { ...simplifiedMetadata, invoiceNo: "SYN-622-1", amountIncVat: 3750, vatAmount: 750, danishSimplifiedPurchaseInvoice: false, incompleteStandardPurchaseInvoice: true, recipient: { name: "Synthetic Individual", address: "Personal Street 2" } };
      const file = join(fixture.inbox, "formal-deficiency.txt"); writeFileSync(file, "Synthetic equipment invoice\n3750 DKK\n");
      const document = ingestDocument(fixture.db, fixture.root, file, metadata, { createdBy: "agent:test", createdByProgram: "test" });
      const documentId = document.documentId!;
      const bankId = Number((fixture.db.query(`INSERT INTO bank_transactions(transaction_date,text,amount,currency,transaction_hash,status) VALUES('2026-08-20','Synthetic company card purchase',-3750,'DKK','bank-syn-622','imported') RETURNING id`).get() as { id:number }).id);
      expect(bookExpenseFromBank(fixture.db, { documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" }).errors).toContain("incomplete standard invoice requires a valid hash-bound VAT evidence review before input-VAT deduction");
      expect(setDocumentCompanyContext(fixture.db, { documentId, sourceReference: "supplier-email:SYN-622", businessUseReason: "Synthetic equipment is used in taxable activity", confirm: true, createdBy: "user:reviewer", createdByProgram: "unit-test" }).ok).toBe(true);
      const input = { documentId, bankTransactionId: bankId, businessEvidenceReference: "purchase-order:SYN-622", businessEvidenceSha256: "a".repeat(64), rationale: "Formal buyer-field deficiency only; payment and business use reviewed", principal: "service-account:synthetic-reviewer", confirm: true, createdBy: "user:reviewer", createdByProgram: "unit-test" };
      const review = reviewIncompleteStandardPurchaseVatEvidence(fixture.db, input);
      expect(review).toMatchObject({ ok: true, applied: true });
      expect(reviewIncompleteStandardPurchaseVatEvidence(fixture.db, input)).toMatchObject({ ok: true, applied: false });
      expect(validIncompleteStandardPurchaseVatEvidenceReview(fixture.db, documentId)).toBe(true);
      const booked = bookExpenseFromBank(fixture.db, { documentId, bankTransactionId: bankId, expenseAccountNo: "3000", vatTreatment: "standard" });
      expect(booked.errors).toEqual([]);
      expect(booked.ok).toBe(true);
    } finally { close(fixture); }
  });
});
