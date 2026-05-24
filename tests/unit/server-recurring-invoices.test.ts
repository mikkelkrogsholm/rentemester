// Tests: GET /api/companies/:slug/recurring-invoices (list) +
// POST /api/companies/:slug/recurring-invoices/:id/generate — the cockpit
// surface for the existing recurring-invoice core.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  createRecurringInvoiceTemplate,
  type RecurringInvoiceTemplateInput,
} from "../../src/core/recurring-invoices";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS" });
  return { root, slug: created.slug };
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    workspaceRoot,
  };
}

async function getJson(cfg: ServerConfig, path: string) {
  const res = await handleRequest(
    new Request(`http://localhost${path}`, { headers: { host: "127.0.0.1" } }),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1" },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

function templateInput(
  overrides: Partial<RecurringInvoiceTemplateInput> = {},
): RecurringInvoiceTemplateInput {
  return {
    name: "Monthly retainer",
    interval: "monthly",
    firstIssueDate: "2026-01-15",
    invoice: {
      invoiceType: "full",
      vatTreatment: "standard",
      seller: { name: "Acme ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [
        {
          description: "Bogføring",
          quantity: 1,
          unitPriceExVat: 1000,
          lineTotalExVat: 1000,
        },
      ],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    },
    paymentTermsDays: 30,
    deliveryPeriodMode: "issue_month",
    ...overrides,
  };
}

function seedTemplate(ws: string, slug: string): number {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const created = createRecurringInvoiceTemplate(db, templateInput());
    if (!created.ok) {
      throw new Error(`template seed failed: ${created.errors.join("; ")}`);
    }
    return created.templateId!;
  } finally {
    db.close();
  }
}

describe("cockpit API — recurring invoices", () => {
  test("GET .../recurring-invoices on a fresh company returns an empty list", async () => {
    const { root: ws, slug } = makeWorkspace("rec-empty");
    try {
      const res = await getJson(
        config(ws),
        `/api/companies/${slug}/recurring-invoices`,
      );
      expect(res.status).toBe(200);
      expect(res.body.recurringInvoices.templates).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET .../recurring-invoices lists seeded templates with their key fields", async () => {
    const { root: ws, slug } = makeWorkspace("rec-list");
    try {
      seedTemplate(ws, slug);
      const res = await getJson(
        config(ws),
        `/api/companies/${slug}/recurring-invoices`,
      );
      expect(res.status).toBe(200);
      const t = res.body.recurringInvoices.templates[0];
      expect(t.name).toBe("Monthly retainer");
      expect(t.interval).toBe("monthly");
      expect(t.nextIssueDate).toBe("2026-01-15");
      expect(t.active).toBe(true);
      expect(Array.isArray(t.generations)).toBe(true);
      expect(t.generations).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../generate issues an invoice from a template", async () => {
    const { root: ws, slug } = makeWorkspace("rec-gen");
    try {
      const templateId = seedTemplate(ws, slug);
      const res = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/generate`,
        { asOfDate: "2026-01-20", confirm: true },
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.generation.created).toBe(true);
      expect(typeof res.body.generation.invoiceNumber).toBe("string");
      // Re-running for the same period must be idempotent — returns the same
      // existing generation, no second invoice.
      const again = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/generate`,
        { asOfDate: "2026-01-20", confirm: true },
      );
      expect(again.status).toBe(200);
      expect(again.body.generation.created).toBe(false);
      expect(again.body.generation.invoiceNumber).toBe(
        res.body.generation.invoiceNumber,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../generate without confirm is refused", async () => {
    const { root: ws, slug } = makeWorkspace("rec-gen-noconfirm");
    try {
      const templateId = seedTemplate(ws, slug);
      const res = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/generate`,
        { asOfDate: "2026-01-20" },
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../generate for an unknown template is rejected", async () => {
    const { root: ws, slug } = makeWorkspace("rec-gen-404");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/99999/generate`,
        { asOfDate: "2026-01-20", confirm: true },
      );
      expect([400, 404, 409]).toContain(res.status);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #435 — cockpit can retire a recurring-invoice template so it stops
  // suggesting itself. Schema trigger forbids unretiring; retired templates
  // refuse generation. The retire route exposes that behaviour over HTTP.
  test("POST .../retire deactivates an active template and blocks future generation", async () => {
    const { root: ws, slug } = makeWorkspace("rec-retire");
    try {
      const templateId = seedTemplate(ws, slug);
      const retire = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/retire`,
        { confirm: true, reason: "Kunde opsagt aftalen" },
      );
      expect(retire.status).toBe(200);
      expect(retire.body.ok).toBe(true);
      expect(retire.body.template.id).toBe(templateId);
      expect(retire.body.template.retired).toBe(true);

      // The list now shows the template as inactive.
      const listed = await getJson(
        config(ws),
        `/api/companies/${slug}/recurring-invoices`,
      );
      const row = listed.body.recurringInvoices.templates.find(
        (t: { id: number }) => t.id === templateId,
      );
      expect(row.active).toBe(false);

      // Generation refuses a retired template.
      const gen = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/generate`,
        { asOfDate: "2026-02-15", confirm: true },
      );
      expect(gen.body.ok).toBe(false);
      expect(JSON.stringify(gen.body)).toContain("inactive");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../retire without confirm is refused (400)", async () => {
    const { root: ws, slug } = makeWorkspace("rec-retire-noconfirm");
    try {
      const templateId = seedTemplate(ws, slug);
      const res = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/retire`,
        {},
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../retire is idempotent — a second retire is still ok", async () => {
    const { root: ws, slug } = makeWorkspace("rec-retire-idempotent");
    try {
      const templateId = seedTemplate(ws, slug);
      const first = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/retire`,
        { confirm: true },
      );
      expect(first.status).toBe(200);
      const second = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/${templateId}/retire`,
        { confirm: true },
      );
      expect(second.status).toBe(200);
      expect(second.body.ok).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST .../retire for an unknown template surfaces an error", async () => {
    const { root: ws, slug } = makeWorkspace("rec-retire-404");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/recurring-invoices/99999/retire`,
        { confirm: true },
      );
      // The handler echoes core errors inside an ok-envelope; the surface is
      // a 200 with ok:false. Either shape is acceptable here.
      if (res.status === 200) {
        expect(res.body.ok).toBe(false);
      } else {
        expect([400, 404, 409]).toContain(res.status);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
