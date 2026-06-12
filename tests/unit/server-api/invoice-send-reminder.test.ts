// Cockpit API — "Send rykker" (#434) atomicity invariant.
//
// handleInvoiceSendReminder runs three sequential, individually-committed core
// writes (register reminder → book fee → send e-mail). ANY send-blocking
// condition — no SMTP config, invalid SMTP content (empty host / bad
// fromAddress), or an invalid recipient — must fail the whole action with
// NOTHING written to the append-only ledger; otherwise the rentel. § 9b
// reminder and the booked fee would be permanent even though the e-mail never
// went out, and the phantom reminder would count against the max-3 /
// 10-days-apart limits on the next attempt. The fix validates all of this at
// step 0, before any write; these tests pin that.
import { mkdirSync } from "node:fs";
import { describe, expect, test } from "bun:test";
import {
  config,
  makeWorkspace,
  companyRootForSlug,
  companyPaths,
  openDb,
  handleRequest,
  issueTestInvoice,
  writeFileSync,
  join,
  rmSync,
  type ServerConfig,
} from "./_shared";

/** Sets up an overdue issued invoice and returns its document id + paths. */
function overdueInvoice(label: string) {
  const ws = makeWorkspace(label, ["Acme ApS"]);
  const cfg = config({ workspaceRoot: ws });
  const slug = "acme-aps";
  // Issued far enough in the past that it is overdue today.
  issueTestInvoice(ws, slug, "2026-01-01", 1000);
  const companyRoot = companyRootForSlug(ws, slug);
  const lookup = openDb(companyPaths(companyRoot).db);
  const invoice = lookup
    .query(
      "SELECT id FROM documents WHERE document_type = 'issued_invoice' ORDER BY id ASC LIMIT 1",
    )
    .get() as { id: number };
  lookup.close();
  return { ws, cfg, slug, companyRoot, invoiceId: invoice.id };
}

/** POSTs send-reminder; returns the HTTP status. */
async function sendReminder(
  cfg: ServerConfig,
  slug: string,
  body: Record<string, unknown>,
): Promise<number> {
  const res = await handleRequest(
    new Request(`http://localhost/api/companies/${slug}/invoices/send-reminder`, {
      method: "POST",
      // The cockpit's localhost write-gate reads the Host header.
      headers: { "content-type": "application/json", host: "localhost" },
      body: JSON.stringify(body),
    }),
    cfg,
  );
  return res.status;
}

/** Counts the reminder + journal rows that must stay 0 after a failed send. */
function ledgerWrites(companyRoot: string) {
  const db = openDb(companyPaths(companyRoot).db);
  try {
    return {
      reminders: (
        db.query("SELECT COUNT(*) AS n FROM invoice_reminders").get() as { n: number }
      ).n,
      journal: (
        db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }
      ).n,
    };
  } finally {
    db.close();
  }
}

function writeSmtpConfig(companyRoot: string, smtp: Record<string, unknown>) {
  const dir = join(companyRoot, "config");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "smtp.json"), JSON.stringify(smtp), "utf8");
}

describe("cockpit API — send-reminder commits nothing when the send cannot happen", () => {
  test("no SMTP config → 400 and no reminder/journal row", async () => {
    const { ws, cfg, slug, companyRoot, invoiceId } = overdueInvoice(
      "send-reminder-no-smtp",
    );
    const status = await sendReminder(cfg, slug, {
      invoiceDocumentId: invoiceId,
      to: "kunde@eksempel.dk",
      confirm: true,
    });
    expect(status).toBe(400);
    expect(ledgerWrites(companyRoot)).toEqual({ reminders: 0, journal: 0 });
    rmSync(ws, { recursive: true, force: true });
  });

  test("present-but-invalid SMTP content (empty host) → 400 and no reminder/journal row", async () => {
    const { ws, cfg, slug, companyRoot, invoiceId } = overdueInvoice(
      "send-reminder-bad-smtp",
    );
    // The config file EXISTS and parses, but its content is invalid — the old
    // file-existence-only check let this through to a post-write step-3 failure.
    writeSmtpConfig(companyRoot, { host: "", port: 587, fromAddress: "", fromName: "Acme" });
    const status = await sendReminder(cfg, slug, {
      invoiceDocumentId: invoiceId,
      to: "kunde@eksempel.dk",
      confirm: true,
    });
    expect(status).toBe(400);
    expect(ledgerWrites(companyRoot)).toEqual({ reminders: 0, journal: 0 });
    rmSync(ws, { recursive: true, force: true });
  });

  test("a valid SMTP config but an invalid recipient → 400 and no reminder/journal row", async () => {
    const { ws, cfg, slug, companyRoot, invoiceId } = overdueInvoice(
      "send-reminder-bad-recipient",
    );
    writeSmtpConfig(companyRoot, {
      host: "smtp.example.test",
      port: 587,
      fromAddress: "faktura@acme.test",
      fromName: "Acme",
    });
    const status = await sendReminder(cfg, slug, {
      invoiceDocumentId: invoiceId,
      to: "ikke-en-email",
      confirm: true,
    });
    expect(status).toBe(400);
    expect(ledgerWrites(companyRoot)).toEqual({ reminders: 0, journal: 0 });
    rmSync(ws, { recursive: true, force: true });
  });
});
