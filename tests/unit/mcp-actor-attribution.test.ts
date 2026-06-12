// Tests: the actor-attribution invariant (#63/#76) for MCP write tools that
// book to the hash-chained ledger.
//
// THE BUG (HIGH-severity, adversarially-confirmed): MCP write tools used to
// destructure only `{ db, args }` from the tool runtime and never thread
// `ctx.actor` into the core call. The core's `resolveActor()` then fell back to
// `process.env.USER/LOGNAME` (the OS user) or "system" for
// journal_entries.created_by / created_by_program and audit_log.actor — so a
// booking made by an agent (e.g. `agent:claude-code/9.9.9`) was silently
// attributed to whoever's shell launched the server. That defeats the entire
// point of the actor module for an agent-first system.
//
// THE FIX: each booking tool now threads `ctx.actor` (derived from the MCP
// initialize-handshake's Implementation) into the core via `withActor(...)`
// (payload-object inputs) or explicit createdBy/createdByProgram (flat inputs).
//
// These tests simulate a client handshake by setting the underlying Server's
// `_clientVersion` — exactly what the SDK assigns from `request.params.clientInfo`
// on `initialize` — to `{ name: "claude-code", version: "9.9.9" }`, then invoke
// the real registered tool handler and assert the STORED attribution on the new
// journal entry is `agent:claude-code/9.9.9` (NOT process.env.USER, NOT "system").
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { registerExpenseTools } from "../../src/mcp/tools/expense";
import { registerPayableTools } from "../../src/mcp/tools/payable";
import { registerInvoiceSettlementTools } from "../../src/mcp/tools/invoice/settlement";
import { registerInvoiceReminderTools } from "../../src/mcp/tools/invoice/reminder";

const AGENT_NAME = "claude-code";
const AGENT_VERSION = "9.9.9";
const EXPECTED_CREATED_BY = `agent:${AGENT_NAME}/${AGENT_VERSION}`;
// deriveMcpActor uses `mcp:<RENTEMESTER_MCP_USER>` when that env-var is set,
// else FALLBACK_PROGRAM. The test deliberately leaves the env-var unset so the
// program is the stable fallback; the assertion below only checks it is NOT an
// OS-user/"system" attribution, since the program string is config-dependent.
const FALLBACK_PROGRAM = "rentemester-mcp";

type ToolMap = Record<
  string,
  { handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: unknown }> }
>;

/**
 * Builds an MCP server, registers the given tool group(s), and simulates the
 * `initialize` handshake by stamping the underlying Server's `_clientVersion`
 * with a concrete Implementation — the same field `deriveMcpActor()` reads via
 * `server.server.getClientVersion()`.
 */
function harness(...registrars: Array<(server: McpServer) => void>) {
  const server = new McpServer({ name: "actor-attribution-test", version: "0.0.0" });
  for (const register of registrars) register(server);
  // Simulate the client handshake: the SDK sets `_clientVersion` from
  // `initialize`'s `params.clientInfo`. We set it directly so the runtime's
  // `getClientVersion()` returns our agent identity.
  (server.server as unknown as { _clientVersion?: unknown })._clientVersion = {
    name: AGENT_NAME,
    version: AGENT_VERSION,
  };
  const tools = (server as unknown as { _registeredTools: ToolMap })._registeredTools;
  return {
    async call(name: string, args: unknown) {
      const tool = tools[name];
      if (!tool) throw new Error(`tool not registered: ${name}`);
      const result = await tool.handler(args, { signal: new AbortController().signal });
      return result.structuredContent as { ok: boolean; data?: any; errors: string[] };
    },
  };
}

function tmpCompany(label: string): string {
  const root = mkdtempSync(join(tmpdir(), `rentemester-actor-${label}-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.close();
  return root;
}

/** Reads created_by / created_by_program for a specific journal entry id. */
function attributionOf(companyRoot: string, entryId: number) {
  const db = openDb(ensureCompanyDirs(companyRoot).db);
  try {
    return db
      .query(`SELECT created_by, created_by_program FROM journal_entries WHERE id = ?`)
      .get(entryId) as { created_by: string; created_by_program: string } | null;
  } finally {
    db.close();
  }
}

/** Reads the audit_log.actor of the most recent row of a given event type. */
function auditActorFor(companyRoot: string, eventType: string): string | null {
  const db = openDb(ensureCompanyDirs(companyRoot).db);
  try {
    const row = db
      .query(`SELECT actor FROM audit_log WHERE event_type = ? ORDER BY id DESC LIMIT 1`)
      .get(eventType) as { actor: string } | null;
    return row?.actor ?? null;
  } finally {
    db.close();
  }
}

describe("MCP write tools attribute the agent actor to the ledger (#63/#76)", () => {
  let companies: string[] = [];
  // Guard: the bug's symptom was a fallback to process.env.USER. Pin a value so
  // a regression that re-introduced the OS-user fallback would produce
  // `agent:...` vs this sentinel and the inequality assertions below would fire.
  const savedUser = process.env.USER;
  const savedLogname = process.env.LOGNAME;
  beforeEach(() => {
    process.env.USER = "os-user-should-not-leak";
    process.env.LOGNAME = "os-user-should-not-leak";
  });
  afterEach(() => {
    if (savedUser === undefined) delete process.env.USER;
    else process.env.USER = savedUser;
    if (savedLogname === undefined) delete process.env.LOGNAME;
    else process.env.LOGNAME = savedLogname;
    for (const c of companies) rmSync(c, { recursive: true, force: true });
    companies = [];
  });

  test("expense_book stamps created_by = agent:<client>/<version>", async () => {
    const company = tmpCompany("expense");
    companies.push(company);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-actor-expense-inbox-"));
    companies.push(inbox);

    // Fixtures (written to the on-disk DB the tool re-opens): a bank receipt +
    // an ingested purchase document.
    const db = openDb(ensureCompanyDirs(company).db);
    const csv = join(company, "bank.csv");
    writeFileSync(
      csv,
      "transaction_date,booking_date,text,amount,currency,reference\n2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-EXP-1\n",
    );
    expect(importBankCsv(db, company, csv).ok).toBe(true);
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");
    const doc = ingestDocument(db, company, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-1001",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);
    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = 'REF-EXP-1'")
      .get() as { id: number };
    db.close();

    const h = harness(registerExpenseTools);
    const env = await h.call("expense_book", {
      company,
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccount: "3000",
      confirm: true,
    });
    expect(env.ok).toBe(true);
    const entryId = env.data?.entryId as number;
    expect(entryId).toBeGreaterThan(0);

    const attribution = attributionOf(company, entryId);
    expect(attribution?.created_by).toBe(EXPECTED_CREATED_BY);
    expect(attribution?.created_by).not.toBe("os-user-should-not-leak");
    expect(attribution?.created_by).not.toBe("system");
    expect(attribution?.created_by_program).toBe(FALLBACK_PROGRAM);
  });

  test("payable_register stamps created_by = agent:<client>/<version>", async () => {
    const company = tmpCompany("payable");
    companies.push(company);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-actor-payable-inbox-"));
    companies.push(inbox);

    const db = openDb(ensureCompanyDirs(company).db);
    const sourceFile = join(inbox, "V-2001.txt");
    writeFileSync(sourceFile, "Invoice V-2001\n1250 DKK\n");
    const doc = ingestDocument(db, company, sourceFile, {
      source: "email",
      issueDate: "2026-01-10",
      invoiceNo: "V-2001",
      deliveryDescription: "Leverandørydelse",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "Leverandørvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);
    db.close();

    const h = harness(registerPayableTools);
    const env = await h.call("payable_register", {
      company,
      documentId: doc.documentId!,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccount: "3000",
      confirm: true,
    });
    expect(env.ok).toBe(true);
    const entryId = env.data?.entryId as number;
    expect(entryId).toBeGreaterThan(0);

    const attribution = attributionOf(company, entryId);
    expect(attribution?.created_by).toBe(EXPECTED_CREATED_BY);
    expect(attribution?.created_by).not.toBe("os-user-should-not-leak");
    expect(attribution?.created_by).not.toBe("system");
    expect(attribution?.created_by_program).toBe(FALLBACK_PROGRAM);
  });

  test("invoice_settle_bank (a settlement tool) stamps created_by = agent:<client>/<version>", async () => {
    const company = tmpCompany("settle");
    companies.push(company);

    // Fixtures: issue + post an invoice (so there is an open receivable), then
    // import the customer payment.
    const db = openDb(ensureCompanyDirs(company).db);
    const issued = issueInvoice(db, company, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    const csv = join(company, "bank.csv");
    writeFileSync(
      csv,
      "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0900\n",
    );
    expect(importBankCsv(db, company, csv).ok).toBe(true);
    const bankTx = db.query("SELECT id FROM bank_transactions LIMIT 1").get() as { id: number };
    db.close();

    const h = harness(registerInvoiceSettlementTools);
    const env = await h.call("invoice_settle_bank", {
      company,
      payload: {
        invoiceDocumentId: issued.documentId!,
        bankTransactionId: bankTx.id,
      },
      confirm: true,
    });
    expect(env.ok).toBe(true);
    const entryId = env.data?.entryId as number;
    expect(entryId).toBeGreaterThan(0);

    const attribution = attributionOf(company, entryId);
    expect(attribution?.created_by).toBe(EXPECTED_CREATED_BY);
    expect(attribution?.created_by).not.toBe("os-user-should-not-leak");
    expect(attribution?.created_by).not.toBe("system");
    expect(attribution?.created_by_program).toBe(FALLBACK_PROGRAM);
  });

  // A register-step tool (invoice_remind) writes NO journal entry — only an
  // invoice_reminders row + an audit_log row. The actor must still be the
  // agent. This pins the gap where withActor was a no-op because the core
  // register input type didn't declare/forward createdBy/createdByProgram, so
  // insertAuditLog fell back to process.env.USER for the audit_log.actor.
  test("invoice_remind (register-step) attributes the audit_log row to the agent, not the OS user", async () => {
    const company = tmpCompany("remind");
    companies.push(company);

    // Issue + post an overdue invoice (issued far in the past), so the reminder
    // precondition (posted + overdue) holds.
    const db = openDb(ensureCompanyDirs(company).db);
    const issued = issueInvoice(db, company, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    db.close();

    const h = harness(registerInvoiceReminderTools);
    const env = await h.call("invoice_remind", {
      company,
      documentId: issued.documentId!,
      date: "2026-05-30",
      confirm: true,
    });
    expect(env.ok).toBe(true);

    const actor = auditActorFor(company, "invoice_reminder_register");
    expect(actor).not.toBeNull();
    expect(actor!.startsWith(`${EXPECTED_CREATED_BY} `)).toBe(true);
    expect(actor).not.toContain("os-user-should-not-leak");
  });
});
