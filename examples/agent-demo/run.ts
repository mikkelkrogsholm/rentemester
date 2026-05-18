#!/usr/bin/env bun
/**
 * Rentemester agent-demo — end-to-end loop over en måneds bilagsmail.
 *
 * Demoen viser Rentemesters thesis i praksis: "agenten handler, reglerne
 * afgør, ledgeren håndhæver". Vi taler udelukkende JSON-RPC til
 * `src/mcp/server.ts` — ingen genveje udenom envelope/confirm/audit.
 *
 * Modes:
 *  --mode rule-based   (default)  Deterministisk regelbase. Ingen API-keys.
 *  --mode claude                  Forsøger Anthropic API hvis ANTHROPIC_API_KEY
 *                                  er sat; ellers fall-back til rule-based.
 *
 * Flow:
 *   1. CLI init (frisk virksomhedsmappe)
 *   2. Spawn MCP-serveren + initialize-handshake
 *   3. bank_import (CSV med 7 transaktioner)
 *   4. For hvert bilag i inbox/:
 *        - documents_ingest med metadata
 *        - bank_suggest_matches → vælg højeste-confidence match
 *        - hvis confidence ≥ HIGH_CONFIDENCE_THRESHOLD: expense_book
 *        - ellers: kommentar i agentens log (bilaget bliver i exceptions-køen)
 *   5. exceptions_list, vat_report, audit_verify, system_healthcheck → opsummering
 *
 * Eksekverbar uden API-key, uden netværk.
 */

import { readFileSync, readdirSync, rmSync, existsSync } from "node:fs";
import { basename, extname, join, resolve } from "node:path";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

type Mode = "rule-based" | "claude";

function parseArgs(argv: string[]) {
  let company: string | null = null;
  let mode: Mode = "rule-based";
  let demoDir: string = new URL(".", import.meta.url).pathname;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--company") company = argv[++i] ?? null;
    else if (arg === "--mode") {
      const value = argv[++i];
      if (value === "rule-based" || value === "claude") mode = value;
      else throw new Error(`Unknown mode: ${value}`);
    } else if (arg === "--demo-dir") demoDir = argv[++i] ?? demoDir;
    else if (arg === "--help" || arg === "-h") {
      printUsage();
      process.exit(0);
    }
  }
  if (!company) {
    printUsage();
    throw new Error("--company <path> is required");
  }
  return { company: resolve(company), mode, demoDir: resolve(demoDir) };
}

function printUsage() {
  console.log(
    "Usage: bun examples/agent-demo/run.ts --company <path> [--mode rule-based|claude]\n" +
      "                                       [--demo-dir <path>]",
  );
}

// ---------------------------------------------------------------------------
// MCP-stdio klient (samme pattern som scripts/smoke-mcp.ts)
// ---------------------------------------------------------------------------

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id: number;
  result?: any;
  error?: { code: number; message: string; data?: unknown };
};

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;
const CLI_PATH = new URL("../../src/cli.ts", import.meta.url).pathname;
const SEED_VIES_PATH = new URL("../../scripts/seed-vies-validation.ts", import.meta.url).pathname;

class McpClient {
  private proc: ReturnType<typeof Bun.spawn>;
  private reader: ReadableStreamDefaultReader<Uint8Array>;
  private decoder = new TextDecoder();
  private buffer = "";
  private nextId = 1;

  constructor() {
    this.proc = Bun.spawn(["bun", SERVER_PATH], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    this.reader = this.proc.stdout.getReader();
  }

  async initialize(clientName: string) {
    const init = await this.request("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: clientName, version: "0.0.1" },
    });
    if (init.error) throw new Error(`initialize failed: ${JSON.stringify(init.error)}`);
    await this.notify("notifications/initialized");
    return init.result;
  }

  async listTools() {
    const res = await this.request("tools/list");
    return (res.result?.tools ?? []) as Array<{ name: string }>;
  }

  /** Kalder et tool og returnerer dets structured envelope. */
  async callTool(name: string, args: Record<string, unknown>) {
    const res = await this.request("tools/call", { name, arguments: args });
    if (res.error) {
      return { ok: false, errors: [res.error.message], data: undefined } as Envelope;
    }
    return (res.result?.structuredContent ?? { ok: false, errors: ["no structuredContent"] }) as Envelope;
  }

  async close() {
    try {
      this.proc.stdin.end();
    } catch {}
    this.proc.kill();
    await this.proc.exited;
  }

  private async request(method: string, params: Record<string, unknown> = {}): Promise<JsonRpcResponse> {
    const id = this.nextId++;
    const req = { jsonrpc: "2.0" as const, id, method, params };
    await this.proc.stdin.write(JSON.stringify(req) + "\n");
    await (this.proc.stdin as any).flush?.();
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) {
        const { value, done } = await this.reader.read();
        if (done) throw new Error(`stdout closed while waiting for id=${id} (${method})`);
        this.buffer += this.decoder.decode(value, { stream: true });
        continue;
      }
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (!line) continue;
      const parsed = JSON.parse(line) as JsonRpcResponse;
      if (parsed.id === id) return parsed;
    }
    throw new Error(`Timed out waiting for ${method}`);
  }

  private async notify(method: string, params: Record<string, unknown> = {}) {
    const note = { jsonrpc: "2.0" as const, method, params };
    await this.proc.stdin.write(JSON.stringify(note) + "\n");
    await (this.proc.stdin as any).flush?.();
  }
}

type Envelope = {
  ok: boolean;
  data?: any;
  errors: string[];
  appliedRules?: string[];
};

// ---------------------------------------------------------------------------
// Rule base — supplier-token → expense-konto + VAT-treatment
// ---------------------------------------------------------------------------

type Rule = {
  token: string;
  expenseAccount: string;
  vatTreatment?: "standard" | "reverse_charge" | "representation" | "exempt";
  label: string;
};

/**
 * Match'es på en lower-cased supplier name. Først match vinder.
 * Holdes bevidst kort — målet er at vise *at* en agent kan blive trænet/instrueret
 * til at træffe disse valg, ikke at have en udtømmende kontoplan-mapper.
 */
const SUPPLIER_RULES: Rule[] = [
  { token: "google", expenseAccount: "3000", vatTreatment: "standard", label: "Software og SaaS" },
  { token: "microsoft", expenseAccount: "3000", vatTreatment: "standard", label: "Software og SaaS" },
  { token: "openai", expenseAccount: "3010", vatTreatment: "reverse_charge", label: "AI-værktøjer" },
  { token: "anthropic", expenseAccount: "3010", vatTreatment: "reverse_charge", label: "AI-værktøjer" },
  { token: "amazon", expenseAccount: "3020", vatTreatment: "reverse_charge", label: "Hosting og cloud" },
  { token: "aws", expenseAccount: "3020", vatTreatment: "reverse_charge", label: "Hosting og cloud" },
  { token: "dsb", expenseAccount: "3050", vatTreatment: "standard", label: "Rejse og transport" },
  { token: "elgiganten", expenseAccount: "3120", vatTreatment: "standard", label: "Hardware og udstyr" },
];

const HIGH_CONFIDENCE_THRESHOLD = 0.65;

function pickRule(supplierName: string | undefined | null): Rule | null {
  if (!supplierName) return null;
  const lower = supplierName.toLowerCase();
  for (const rule of SUPPLIER_RULES) {
    if (lower.includes(rule.token)) return rule;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Inbox-walking
// ---------------------------------------------------------------------------

type InboxItem = {
  name: string;
  filePath: string;
  metadataPath: string;
  metadata: Record<string, any>;
};

function loadInbox(demoDir: string): InboxItem[] {
  const inboxDir = join(demoDir, "inbox");
  const metaDir = join(demoDir, "metadata");
  if (!existsSync(inboxDir)) throw new Error(`inbox directory missing: ${inboxDir}`);
  const items: InboxItem[] = [];
  for (const file of readdirSync(inboxDir).sort()) {
    if (file.startsWith(".")) continue;
    const ext = extname(file);
    if (ext !== ".txt" && ext !== ".pdf" && ext !== ".json") continue;
    const stem = basename(file, ext);
    const metadataPath = join(metaDir, `${stem}.json`);
    if (!existsSync(metadataPath)) throw new Error(`Missing metadata for ${file}: ${metadataPath}`);
    const metadata = JSON.parse(readFileSync(metadataPath, "utf8"));
    items.push({
      name: file,
      filePath: join(inboxDir, file),
      metadataPath,
      metadata,
    });
  }
  return items;
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

function fmtAmount(n: number, ccy = "DKK") {
  const sign = n < 0 ? "-" : "";
  const abs = Math.abs(n);
  return `${sign}${abs.toLocaleString("da-DK", { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${ccy}`;
}

function bullet(symbol: string, text: string) {
  console.log(`  ${symbol} ${text}`);
}

function section(title: string) {
  console.log(`\n— ${title} —`);
}

function header(text: string) {
  console.log(`\n${text}`);
  console.log("=".repeat(text.length));
}

// ---------------------------------------------------------------------------
// Agent steps
// ---------------------------------------------------------------------------

async function ensureCompanyInitialized(company: string) {
  if (existsSync(company)) {
    rmSync(company, { recursive: true, force: true });
  }
  const init = Bun.spawn(["bun", CLI_PATH, "init", "--company", company], {
    stdout: "pipe",
    stderr: "pipe",
  });
  await init.exited;
  if (init.exitCode !== 0) {
    const stderr = await new Response(init.stderr).text();
    throw new Error(`CLI init failed (exit ${init.exitCode}): ${stderr}`);
  }
}

/**
 * Pre-seeder VIES-validering for EU-leverandører (ikke-DK VAT IDs) i inbox.
 *
 * Vi gør det ikke via det rigtige VIES-MCP-tool fordi demoen skal kunne køre
 * uden netværk; det produktive flow ville være `customer_validate_vat`.
 */
async function preSeedViesValidations(company: string, inbox: InboxItem[]) {
  const eu = new Set<string>();
  for (const item of inbox) {
    const vat = String(item.metadata?.sender?.vatOrCvr ?? "").trim();
    if (!vat) continue;
    const prefix = vat.slice(0, 2).toUpperCase();
    if (prefix === "DK" || !/^[A-Z]{2}/.test(prefix)) continue;
    eu.add(vat);
  }
  for (const vat of eu) {
    const proc = Bun.spawn(["bun", SEED_VIES_PATH, company, vat], {
      stdout: "pipe",
      stderr: "pipe",
    });
    await proc.exited;
    if (proc.exitCode !== 0) {
      const stderr = await new Response(proc.stderr).text();
      throw new Error(`VIES-seed failed for ${vat}: ${stderr}`);
    }
  }
  return eu.size;
}

type Summary = {
  documentsIngested: number;
  documentIngestErrors: number;
  bankImported: number;
  expensesBooked: number;
  exceptionsOpen: number;
  vatSalesTotal: number;
  vatPurchaseTotal: number;
  auditEntries: number;
  auditOk: boolean;
  healthOk: boolean;
  elapsedMs: number;
};

async function runDemo(args: { company: string; mode: Mode; demoDir: string }): Promise<Summary> {
  const t0 = Date.now();
  const { company, mode, demoDir } = args;

  header("Rentemester agent-demo");
  console.log(`mode:        ${mode}`);
  console.log(`company:     ${company}`);
  console.log(`demo-dir:    ${demoDir}`);

  if (mode === "claude" && !process.env.ANTHROPIC_API_KEY) {
    console.log(
      "\nBemærk: --mode claude angivet men ANTHROPIC_API_KEY mangler.\n" +
        "Falder tilbage til rule-based — adfærden er identisk for denne demo.",
    );
  }

  section("Initialiserer frisk virksomhedsmappe");
  await ensureCompanyInitialized(company);
  bullet("✓", `company init OK (${company})`);

  section("Spawner MCP-server");
  const client = new McpClient();
  let summary: Summary = {
    documentsIngested: 0,
    documentIngestErrors: 0,
    bankImported: 0,
    expensesBooked: 0,
    exceptionsOpen: 0,
    vatSalesTotal: 0,
    vatPurchaseTotal: 0,
    auditEntries: 0,
    auditOk: false,
    healthOk: false,
    elapsedMs: 0,
  };

  try {
    await client.initialize("rentemester-agent-demo");
    const tools = await client.listTools();
    bullet("✓", `MCP klar — ${tools.length} tools registered`);

    // -------- 1. bank_import --------
    section("Importerer bank-CSV");
    const bankCsvPath = join(demoDir, "bank.csv");
    const bankImport = await client.callTool("bank_import", {
      company,
      csvPath: bankCsvPath,
      confirm: true,
    });
    if (!bankImport.ok) throw new Error(`bank_import failed: ${bankImport.errors.join("; ")}`);
    const bankInserted = Number(bankImport.data?.inserted ?? bankImport.data?.imported ?? 0);
    summary.bankImported = bankInserted;
    bullet("✓", `${bankInserted} banktransaktioner importeret`);

    // -------- 2. documents_ingest pr. inbox-fil --------
    section("Læser inbox og ingester bilag");
    const inbox = loadInbox(demoDir);
    const seeded = await preSeedViesValidations(company, inbox);
    if (seeded > 0) bullet("✓", `${seeded} EU-leverandør(er) VIES-validated (offline-seed)`);
    const ingested: Array<{ item: InboxItem; documentId: number }> = [];
    for (const item of inbox) {
      const res = await client.callTool("documents_ingest", {
        company,
        filePath: item.filePath,
        metadata: item.metadata,
        confirm: true,
      });
      if (!res.ok) {
        summary.documentIngestErrors++;
        bullet("✗", `${item.name} — ingest blokeret: ${res.errors.join("; ")}`);
        continue;
      }
      summary.documentsIngested++;
      const documentId = Number(res.data?.documentId);
      const documentNo = String(res.data?.documentNo ?? "?");
      ingested.push({ item, documentId });
      bullet(
        "✓",
        `${item.name} → ${documentNo} (id=${documentId}, ${fmtAmount(
          Number(item.metadata.amountIncVat ?? 0),
          String(item.metadata.currency ?? "DKK"),
        )})`,
      );
    }

    // -------- 3. bank_suggest_matches + expense_book --------
    section("Foreslår og bogfører matches");
    const suggestions = await client.callTool("bank_suggest_matches", { company, max: 3 });
    if (!suggestions.ok) {
      throw new Error(`bank_suggest_matches failed: ${suggestions.errors.join("; ")}`);
    }
    const rows: Array<{
      bankTransactionId: number;
      date: string;
      text: string;
      amount: number;
      suggestions: Array<{
        kind: string;
        documentId: number;
        supplierName?: string | null;
        confidence: number;
        reasons: string[];
      }>;
    }> = suggestions.data?.rows ?? [];

    type Decision = {
      bankTransactionId: number;
      documentId: number;
      supplier: string;
      rule: Rule;
      amount: number;
    };
    const decisions: Decision[] = [];
    const skipped: Array<{ bankTxId: number; text: string; reason: string }> = [];

    for (const row of rows) {
      const best = row.suggestions
        .filter((s) => s.kind === "purchase_sale")
        .sort((a, b) => b.confidence - a.confidence)[0];
      if (!best) {
        skipped.push({
          bankTxId: row.bankTransactionId,
          text: row.text,
          reason: "ingen høj-confidence match foreslået",
        });
        continue;
      }
      if (best.confidence < HIGH_CONFIDENCE_THRESHOLD) {
        skipped.push({
          bankTxId: row.bankTransactionId,
          text: row.text,
          reason: `confidence ${best.confidence} < threshold ${HIGH_CONFIDENCE_THRESHOLD}`,
        });
        continue;
      }
      const rule = pickRule(best.supplierName);
      if (!rule) {
        skipped.push({
          bankTxId: row.bankTransactionId,
          text: row.text,
          reason: `ingen kontoregel for leverandør '${best.supplierName ?? "(ukendt)"}'`,
        });
        continue;
      }
      decisions.push({
        bankTransactionId: row.bankTransactionId,
        documentId: best.documentId,
        supplier: best.supplierName ?? "(ukendt)",
        rule,
        amount: row.amount,
      });
    }

    for (const d of decisions) {
      const res = await client.callTool("expense_book", {
        company,
        documentId: d.documentId,
        bankTransactionId: d.bankTransactionId,
        expenseAccount: d.rule.expenseAccount,
        vatTreatment: d.rule.vatTreatment,
        confirm: true,
      });
      if (!res.ok) {
        bullet(
          "✗",
          `expense_book afvist (doc=${d.documentId}, bank=${d.bankTransactionId}): ${res.errors.join("; ")}`,
        );
        // Lad det havne i exception-køen — agenten må stoppe.
        continue;
      }
      summary.expensesBooked++;
      bullet(
        "✓",
        `Bogført ${d.supplier} ${fmtAmount(d.amount, "DKK")} → konto ${d.rule.expenseAccount} (${d.rule.label}, VAT=${d.rule.vatTreatment})`,
      );
    }

    if (skipped.length > 0) {
      console.log("");
      bullet("…", `${skipped.length} banktransaktion(er) sprunget over:`);
      for (const s of skipped) {
        console.log(`    · bank-tx ${s.bankTxId} "${s.text}" — ${s.reason}`);
      }
    }

    // -------- 4. exceptions_list --------
    section("Exceptions-kø");
    const exceptionsResp = await client.callTool("exceptions_list", { company, status: "open" });
    const openExceptions: Array<{ id: number; type: string; message: string; severity: string }> =
      exceptionsResp.data?.exceptions ?? exceptionsResp.data?.rows ?? [];
    summary.exceptionsOpen = openExceptions.length;
    if (openExceptions.length === 0) {
      bullet("✓", "Ingen åbne exceptions");
    } else {
      for (const ex of openExceptions) {
        bullet(
          "!",
          `[${ex.severity ?? "?"}] #${ex.id} ${ex.type}: ${ex.message}`,
        );
      }
    }

    // -------- 5. vat_report --------
    section("Momsrapport (2026-05)");
    const vat = await client.callTool("vat_report", {
      company,
      from: "2026-05-01",
      to: "2026-05-31",
    });
    if (vat.ok) {
      summary.vatSalesTotal = Number(vat.data?.outputVat ?? 0);
      summary.vatPurchaseTotal = Number(vat.data?.inputVat ?? 0);
      bullet(
        "✓",
        `udgående moms ${fmtAmount(summary.vatSalesTotal)}, indgående moms ${fmtAmount(summary.vatPurchaseTotal)}, netto ${fmtAmount(summary.vatSalesTotal - summary.vatPurchaseTotal)}`,
      );
    } else {
      bullet("✗", `vat_report fejl: ${vat.errors.join("; ")}`);
    }

    // -------- 6. audit_verify --------
    section("Audit chain");
    const audit = await client.callTool("audit_verify", { company });
    summary.auditOk = Boolean(audit.ok);
    summary.auditEntries = Number(audit.data?.entries ?? 0);
    bullet(
      audit.ok ? "✓" : "✗",
      `hash-kæde ${audit.ok ? "intakt" : "BRUDT"} (${summary.auditEntries} entries)`,
    );

    // -------- 7. system_healthcheck --------
    section("System healthcheck");
    const health = await client.callTool("system_healthcheck", { company });
    summary.healthOk = Boolean(health.ok);
    bullet(health.ok ? "✓" : "✗", health.ok ? "alle kerne-filer findes" : `mangler: ${health.errors.join("; ")}`);
  } finally {
    await client.close();
  }

  summary.elapsedMs = Date.now() - t0;

  // -------- Final summary --------
  header("=== Rentemester agent-demo, kørsel afsluttet ===");
  console.log(`  • ${summary.documentsIngested} bilag ingested${summary.documentIngestErrors ? ` (${summary.documentIngestErrors} fejlet)` : ""}`);
  console.log(`  • ${summary.bankImported} bank-transaktioner importeret`);
  console.log(`  • ${summary.expensesBooked} udgifter bogført automatisk`);
  console.log(`  • ${summary.exceptionsOpen} i exception queue`);
  console.log(`  • Audit-chain: ${summary.auditOk ? "OK" : "BRUDT"} (${summary.auditEntries} entries)`);
  console.log(
    `  • Næste momsangivelse: Q2 2026, udgående moms ${fmtAmount(summary.vatSalesTotal)}, indgående moms ${fmtAmount(summary.vatPurchaseTotal)}`,
  );
  console.log(`  • Tid brugt: ${(summary.elapsedMs / 1000).toFixed(1)} sekunder`);
  console.log("");

  return summary;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const args = parseArgs(process.argv.slice(2));
const summary = await runDemo(args);

// Exit non-zero hvis noget kritisk fejlede.
if (!summary.auditOk || !summary.healthOk) process.exit(2);
process.exit(0);
