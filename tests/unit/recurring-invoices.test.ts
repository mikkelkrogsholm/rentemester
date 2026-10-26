// Tests: src/core/recurring-invoices.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  createRecurringInvoiceTemplate,
  generateRecurringInvoice,
  listRecurringInvoiceGenerations,
  listRecurringInvoiceTemplates,
  retireRecurringInvoiceTemplate,
} from "../../src/core/recurring-invoices";
import { storeViesValidation } from "../../src/core/vies";
import { initialiseCompanyVolume, setCompanyProfile } from "../../src/core/company";

function baseTemplateInput(overrides: Record<string, unknown> = {}) {
  return {
    name: "Monthly retainer",
    interval: "monthly" as const,
    firstIssueDate: "2026-01-15",
    invoice: {
      invoiceType: "full" as const,
      vatTreatment: "standard" as const,
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    },
    paymentTermsDays: 30,
    deliveryPeriodMode: "issue_month" as const,
    ...overrides,
  };
}

describe("recurring invoice templates", () => {
  test("creates a template and lists it", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-create-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const created = createRecurringInvoiceTemplate(db, baseTemplateInput());
    expect(created.ok).toBe(true);
    expect(created.templateId).toBeGreaterThan(0);
    expect(created.appliedRules).toContain("DK-RECURRING-INVOICE-TEMPLATE-001");

    const listed = listRecurringInvoiceTemplates(db);
    expect(listed.ok).toBe(true);
    expect(listed.count).toBe(1);
    expect(listed.rows[0]!.name).toBe("Monthly retainer");
    expect(listed.rows[0]!.interval).toBe("monthly");
    expect(listed.rows[0]!.nextIssueDate).toBe("2026-01-15");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects an unknown interval", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-bad-interval-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const created = createRecurringInvoiceTemplate(
      db,
      baseTemplateInput({ interval: "fortnightly" }),
    );
    expect(created.ok).toBe(false);
    expect(created.errors[0]).toContain("interval");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a template whose embedded invoice payload is invalid", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-bad-payload-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const created = createRecurringInvoiceTemplate(
      db,
      baseTemplateInput({
        invoice: {
          invoiceType: "full",
          vatTreatment: "standard",
          seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          buyer: { name: "Kunde A/S", address: "Købervej 9" },
          lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
          // grossAmount intentionally wrong
          totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 9999 },
          currency: "DKK",
        },
      }),
    );
    expect(created.ok).toBe(false);
    expect(created.errors.join(" ")).toContain("grossAmount");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

describe("recurring invoice generation", () => {
  test("uses the company's daily invoice series", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-daily-"));
    try {
      initialiseCompanyVolume(root, { name: "Synthetic ApS", cvr: "DK12345678", address: "Testvej 1" });
      const db = openDb(ensureCompanyDirs(root).db);
      expect(setCompanyProfile(db, { invoiceNumberFormat: "daily" }).ok).toBe(true);
      const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
      const generated = generateRecurringInvoice(db, root, { templateId: template.templateId!, asOfDate: "2026-01-20" });
      expect(generated.invoiceNumber).toBe("RM-26-01-15-01");
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
  test("removes promoted invoice artifacts when the outer generation link rolls back", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-artifact-rollback-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
    migrate(db);
    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    db.exec(`CREATE TRIGGER recurring_generation_forced_abort
      BEFORE INSERT ON recurring_invoice_generations
      BEGIN SELECT RAISE(ABORT, 'forced generation link failure'); END;`);

    expect(() => generateRecurringInvoice(db, root, {
      templateId: template.templateId!, asOfDate: "2026-01-20",
    })).toThrow("forced generation link failure");
    expect(db.query("SELECT COUNT(*) AS n FROM documents WHERE document_type IN ('issued_invoice','issued_invoice_pdf')").get()).toEqual({ n: 0 });
    expect(db.query("SELECT COUNT(*) AS n FROM recurring_invoice_generation_claims").get()).toEqual({ n: 0 });
    expect(db.query("SELECT COUNT(*) AS n FROM recurring_invoice_generations").get()).toEqual({ n: 0 });
    expect(readdirSync(paths.invoicesIssued).filter((name) => name.endsWith(".json") || name.endsWith(".pdf"))).toEqual([]);

    db.exec("DROP TRIGGER recurring_generation_forced_abort;");
    const retried = generateRecurringInvoice(db, root, {
      templateId: template.templateId!, asOfDate: "2026-01-20",
    });
    expect(retried.ok).toBe(true);
    expect(retried.created).toBe(true);
    expect(readdirSync(paths.invoicesIssued).sort()).toEqual(["2026-0001.json", "2026-0001.pdf"]);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("materializes the first due invoice for an as-of date", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-first-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const result = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-01-20",
    });

    expect(result.ok).toBe(true);
    expect(result.created).toBe(true);
    expect(result.periodIndex).toBe(0);
    expect(result.issueDate).toBe("2026-01-15");
    expect(result.invoiceNumber).toBe("2026-0001");
    expect(result.deliveryPeriodStart).toBe("2026-01-01");
    expect(result.deliveryPeriodEnd).toBe("2026-01-31");
    expect(result.appliedRules).toContain("DK-RECURRING-INVOICE-GENERATE-001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("does not regenerate the same template/period on rerun (idempotent)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-idem-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const first = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-01-20",
    });
    const second = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-01-20",
    });

    expect(first.ok).toBe(true);
    expect(first.created).toBe(true);
    expect(second.ok).toBe(true);
    expect(second.created).toBe(false);
    expect(second.periodIndex).toBe(first.periodIndex);
    expect(second.documentId).toBe(first.documentId);
    expect(second.invoiceNumber).toBe(first.invoiceNumber);

    // Exactly one issued invoice exists for the template.
    const docCount = db
      .query("SELECT COUNT(*) AS n FROM documents WHERE document_type = 'issued_invoice'")
      .get() as { n: number };
    expect(docCount.n).toBe(1);
    const genCount = db
      .query("SELECT COUNT(*) AS n FROM recurring_invoice_generations WHERE template_id = ?")
      .get(template.templateId!) as { n: number };
    expect(genCount.n).toBe(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("advances period index deterministically across intervals", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-advance-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const jan = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-01-15",
    });
    const feb = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-02-15",
    });
    const mar = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-03-31",
    });

    expect(jan.periodIndex).toBe(0);
    expect(jan.issueDate).toBe("2026-01-15");
    expect(feb.periodIndex).toBe(1);
    expect(feb.issueDate).toBe("2026-02-15");
    expect(mar.periodIndex).toBe(2);
    expect(mar.issueDate).toBe("2026-03-15");
    expect(mar.deliveryPeriodStart).toBe("2026-03-01");
    expect(mar.deliveryPeriodEnd).toBe("2026-03-31");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("generates only one invoice per period even when as-of skips ahead", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-skip-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    // First call lands directly on period 2 (March); no back-fill of 0/1.
    const skipped = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-03-20",
    });
    expect(skipped.ok).toBe(true);
    expect(skipped.periodIndex).toBe(2);

    const docCount = db
      .query("SELECT COUNT(*) AS n FROM documents WHERE document_type = 'issued_invoice'")
      .get() as { n: number };
    expect(docCount.n).toBe(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("refuses to generate before the first issue date", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-early-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const result = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2025-12-01",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("not yet due");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("records an audit link from generated invoice back to the template", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-audit-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const result = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-01-20",
    });
    expect(result.ok).toBe(true);

    const generations = listRecurringInvoiceGenerations(db, template.templateId!);
    expect(generations.count).toBe(1);
    expect(generations.rows[0]!.templateId).toBe(template.templateId!);
    expect(generations.rows[0]!.documentId).toBe(result.documentId!);
    expect(generations.rows[0]!.periodIndex).toBe(0);
    expect(generations.rows[0]!.invoiceNumber).toBe(result.invoiceNumber!);

    const auditRow = db
      .query(
        "SELECT entity_type, entity_id, message FROM audit_log WHERE event_type = 'recurring_invoice_generate' ORDER BY id DESC LIMIT 1",
      )
      .get() as { entity_type: string; entity_id: string; message: string } | null;
    expect(auditRow).not.toBeNull();
    expect(auditRow!.message).toContain(`template ${template.templateId!}`);
    expect(auditRow!.message).toContain(result.invoiceNumber!);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rolls back a rejected claim and succeeds after repairable VIES evidence is added", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-vies-retry-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const template = createRecurringInvoiceTemplate(db, baseTemplateInput({
      firstIssueDate: "2026-05-16",
      invoice: {
        invoiceType: "full",
        vatTreatment: "foreign_reverse_charge",
        seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
        lines: [{ description: "EU consulting", quantity: 1, unitPriceExVat: 8000, lineTotalExVat: 8000 }],
        totals: { netAmount: 8000, grossAmount: 8000 },
        reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
        reverseChargeNote: "VAT reverse charge — VAT to be accounted by the recipient",
        currency: "DKK",
      },
    }));
    const rejected = generateRecurringInvoice(db, root, {
      templateId: template.templateId!, asOfDate: "2026-05-16",
    });
    expect(rejected.ok).toBe(false);
    expect(rejected.errors.join(" ")).toContain("VIES lookup not yet performed");
    expect(db.query("SELECT COUNT(*) AS n FROM recurring_invoice_generation_claims").get()).toEqual({ n: 0 });
    expect(db.query("SELECT COUNT(*) AS n FROM documents WHERE document_type = 'issued_invoice'").get()).toEqual({ n: 0 });

    storeViesValidation(db, {
      vatOrCvr: "DE123456789", valid: true,
      validatedAt: "2026-05-15T00:00:00.000Z",
      expiresAt: "2026-12-15T00:00:00.000Z",
    });
    const repaired = generateRecurringInvoice(db, root, {
      templateId: template.templateId!, asOfDate: "2026-05-16",
    });
    expect(repaired.ok).toBe(true);
    expect(repaired.created).toBe(true);
    expect(db.query("SELECT COUNT(*) AS n FROM recurring_invoice_generation_claims").get()).toEqual({ n: 1 });
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("attributes issue, render and generation audits to the scheduler actor", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-actor-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const template = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const result = generateRecurringInvoice(db, root, {
      templateId: template.templateId!, asOfDate: "2026-01-20",
      createdBy: "system:scheduler", createdByProgram: "recurring-cron",
    });
    expect(result.ok).toBe(true);
    const rows = db.query(
      `SELECT event_type, actor FROM audit_log
        WHERE event_type IN ('invoice_issue','invoice_render_pdf','recurring_invoice_generate')
        ORDER BY id`,
    ).all() as Array<{ event_type: string; actor: string }>;
    expect(rows.map((row) => row.event_type)).toEqual([
      "invoice_issue", "invoice_render_pdf", "recurring_invoice_generate",
    ]);
    expect(rows.every((row) => row.actor === "system:scheduler via recurring-cron")).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("quarterly interval advances three months per period", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-gen-quarter-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const template = createRecurringInvoiceTemplate(
      db,
      baseTemplateInput({ interval: "quarterly", firstIssueDate: "2026-01-31" }),
    );
    const q1 = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-02-01",
    });
    const q2 = generateRecurringInvoice(db, root, {
      templateId: template.templateId!,
      asOfDate: "2026-05-01",
    });

    expect(q1.periodIndex).toBe(0);
    expect(q1.issueDate).toBe("2026-01-31");
    expect(q2.periodIndex).toBe(1);
    // Month-end clamps deterministically: Jan 31 + 3 months -> Apr 30.
    expect(q2.issueDate).toBe("2026-04-30");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("retires an active template and blocks generation afterwards", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-retire-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const created = createRecurringInvoiceTemplate(db, baseTemplateInput());
    expect(created.ok).toBe(true);
    const templateId = created.templateId!;

    const retired = retireRecurringInvoiceTemplate(db, {
      templateId,
      reason: "Customer cancelled subscription",
      createdBy: "user:test",
      createdByProgram: "cockpit",
    });
    expect(retired.ok).toBe(true);
    expect(retired.appliedRules).toContain("DK-RECURRING-INVOICE-TEMPLATE-001");

    // Template must show up as inactive in the list-with-inactive view.
    const listed = listRecurringInvoiceTemplates(db, { includeInactive: true });
    const row = listed.rows.find((r) => r.id === templateId);
    expect(row?.active).toBe(false);

    // Generation must refuse a retired template.
    const gen = generateRecurringInvoice(db, root, {
      templateId,
      asOfDate: "2026-02-15",
    });
    expect(gen.ok).toBe(false);
    expect(gen.errors.join(" ")).toContain("inactive");

    // Audit log must record the retirement.
    const auditRows = db
      .query(
        `SELECT event_type, entity_id, message FROM audit_log
           WHERE entity_type = 'recurring_invoice_template' AND event_type = 'recurring_invoice_template_retire'`,
      )
      .all() as { event_type: string; entity_id: string | number; message: string }[];
    expect(auditRows.length).toBe(1);
    expect(String(auditRows[0]!.entity_id)).toBe(String(templateId));
    expect(auditRows[0]!.message).toContain("Customer cancelled subscription");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("retiring a non-existent template is an error", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-retire-missing-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const retired = retireRecurringInvoiceTemplate(db, { templateId: 9999 });
    expect(retired.ok).toBe(false);
    expect(retired.errors[0]).toContain("does not exist");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("retiring an already-retired template is idempotent (no-op success)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-retire-idempotent-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const created = createRecurringInvoiceTemplate(db, baseTemplateInput());
    const templateId = created.templateId!;

    const first = retireRecurringInvoiceTemplate(db, { templateId });
    expect(first.ok).toBe(true);
    const second = retireRecurringInvoiceTemplate(db, { templateId });
    expect(second.ok).toBe(true);

    // Only one audit log row even after two retire calls.
    const auditRows = db
      .query(
        `SELECT event_type FROM audit_log
           WHERE entity_type = 'recurring_invoice_template' AND event_type = 'recurring_invoice_template_retire'`,
      )
      .all() as { event_type: string }[];
    expect(auditRows.length).toBe(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
