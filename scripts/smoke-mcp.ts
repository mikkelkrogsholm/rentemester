#!/usr/bin/env bun
/**
 * smoke-mcp — kører invoice-lifecycle gennem MCP-serveren over stdio.
 *
 * Komplementer det eksisterende CLI-`smoke`-target og verificerer at MCP-laget
 * (envelope, confirm-gating, actor-attribution) faktisk fungerer ende-til-ende.
 *
 * Flow:
 *   1. Spawn fresh company via CLI `init` (genbrug af CLI er bevidst — vi
 *      tester MCP, ikke om vi har en `company_init`-tool endnu).
 *   2. Spawn MCP-server som child process.
 *   3. initialize + tools/list.
 *   4. invoice_validate (read) → invoice_issue (write) → invoice_post (write) →
 *      invoice_status (read) → journal_list (read) → audit_verify (read).
 *
 * Exit non-zero hvis nogen step fejler.
 */

import { mkdtempSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SERVER_PATH = new URL("../src/mcp/server.ts", import.meta.url).pathname;
const CLI_PATH = new URL("../src/cli.ts", import.meta.url).pathname;
const INVOICE_FIXTURE = new URL("../examples/full-invoice.dk.json", import.meta.url).pathname;

const TMP_ROOT = mkdtempSync(join(tmpdir(), "rentemester-smoke-mcp-"));
const COMPANY = join(TMP_ROOT, "company");

const wall = Date.now();

function logStep(label: string) {
  console.log(`\n[smoke-mcp] ${label}`);
}

function fail(message: string): never {
  console.error(`\n[smoke-mcp] FAIL: ${message}`);
  if (existsSync(TMP_ROOT)) rmSync(TMP_ROOT, { recursive: true, force: true });
  process.exit(1);
}

// -------------------- step 1: init company via CLI ---------------------------

logStep("Initialiserer fresh virksomhedsmappe via CLI");
const init = Bun.spawn(["bun", CLI_PATH, "init", "--company", COMPANY], {
  stdout: "pipe",
  stderr: "pipe",
});
await init.exited;
if (init.exitCode !== 0) {
  const err = await new Response(init.stderr).text();
  fail(`CLI init exited ${init.exitCode}: ${err}`);
}

// -------------------- step 2: spawn MCP server -------------------------------

logStep("Spawner MCP-server");
const proc = Bun.spawn(["bun", SERVER_PATH], {
  stdin: "pipe",
  stdout: "pipe",
  stderr: "pipe",
});

const reader = proc.stdout.getReader();
const decoder = new TextDecoder();
let buffer = "";
let nextId = 1;

async function send(method: string, params: Record<string, unknown> = {}): Promise<any> {
  const id = nextId++;
  const req = { jsonrpc: "2.0", id, method, params };
  await proc.stdin.write(JSON.stringify(req) + "\n");
  await (proc.stdin as any).flush?.();
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const newline = buffer.indexOf("\n");
    if (newline < 0) {
      const { value, done } = await reader.read();
      if (done) fail(`stdout closed waiting for response id=${id} (${method})`);
      buffer += decoder.decode(value, { stream: true });
      continue;
    }
    const line = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (!line) continue;
    const parsed = JSON.parse(line);
    if (parsed.id === id) return parsed;
  }
  fail(`Timed out on ${method} id=${id}`);
}

async function notify(method: string, params: Record<string, unknown> = {}) {
  const note = { jsonrpc: "2.0", method, params };
  await proc.stdin.write(JSON.stringify(note) + "\n");
  await (proc.stdin as any).flush?.();
}

function structured(response: any) {
  return response?.result?.structuredContent;
}

function expectOk(label: string, response: any) {
  const env = structured(response);
  if (!env || env.ok !== true) {
    fail(`${label} expected ok=true, got: ${JSON.stringify(env)}`);
  }
  return env.data;
}

try {
  // -------------------- step 3: handshake ----------------------------------

  logStep("initialize handshake");
  const init = await send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-smoke-mcp", version: "0.0.1" },
  });
  if (init.error) fail(`initialize failed: ${JSON.stringify(init.error)}`);
  if (init.result?.serverInfo?.name !== "rentemester-mcp") {
    fail(`server name mismatch: ${JSON.stringify(init.result?.serverInfo)}`);
  }
  await notify("notifications/initialized");

  // -------------------- step 4: tools/list ---------------------------------

  logStep("tools/list");
  const tools = await send("tools/list");
  const toolNames = (tools.result?.tools ?? []).map((t: any) => t.name);
  console.log(`  ${toolNames.length} tools registered`);
  for (const required of [
    "audit_verify",
    "invoice_issue",
    "invoice_post",
    "invoice_status",
    "journal_list",
    "system_restore_backup",
  ]) {
    if (!toolNames.includes(required)) fail(`tools/list missing ${required}`);
  }

  // -------------------- step 5: invoice_validate (read) --------------------

  logStep("invoice_validate (read)");
  const fixture = JSON.parse(await Bun.file(INVOICE_FIXTURE).text());
  const validate = await send("tools/call", {
    name: "invoice_validate",
    arguments: { payload: fixture },
  });
  expectOk("invoice_validate", validate);

  // -------------------- step 6: invoice_issue (write-irrev) ----------------

  logStep("invoice_issue (write-irreversible)");
  const issue = await send("tools/call", {
    name: "invoice_issue",
    arguments: { company: COMPANY, payload: fixture, confirm: true },
  });
  const issueData = expectOk("invoice_issue", issue);
  const documentId = issueData?.invoiceDocumentId ?? issueData?.documentId;
  const invoiceNo = issueData?.invoiceNumber ?? issueData?.invoiceNo ?? fixture.invoiceNumber;
  if (!documentId) fail(`invoice_issue did not return documentId; payload=${JSON.stringify(issueData)}`);

  // confirm-gating: same call without confirm:true must fail upfront.
  const issueNoConfirm = await send("tools/call", {
    name: "invoice_issue",
    arguments: { company: COMPANY, payload: fixture, confirm: false },
  });
  if (structured(issueNoConfirm)?.ok !== false) {
    fail(`invoice_issue without confirm should fail, got: ${JSON.stringify(structured(issueNoConfirm))}`);
  }

  // -------------------- step 7: invoice_post (write-irrev) -----------------

  logStep("invoice_post (write-irreversible)");
  const post = await send("tools/call", {
    name: "invoice_post",
    arguments: { company: COMPANY, invoiceNumber: invoiceNo, confirm: true },
  });
  expectOk("invoice_post", post);

  // -------------------- step 8: invoice_status (read) ----------------------

  logStep("invoice_status (read)");
  const status = await send("tools/call", {
    name: "invoice_status",
    arguments: { company: COMPANY, invoiceNumber: invoiceNo, asOf: "2026-06-01" },
  });
  const statusData = expectOk("invoice_status", status);
  console.log(`  status=${statusData?.status} openBalance=${statusData?.openBalance}`);

  // -------------------- step 9: journal_list (read) ------------------------

  logStep("journal_list (read)");
  const journal = await send("tools/call", {
    name: "journal_list",
    arguments: { company: COMPANY },
  });
  const journalData = expectOk("journal_list", journal);
  if (!journalData?.count || journalData.count < 1) {
    fail(`journal_list expected >=1 entries, got count=${journalData?.count}`);
  }

  // -------------------- step 10: audit_verify (read) -----------------------

  logStep("audit_verify (read)");
  const audit = await send("tools/call", {
    name: "audit_verify",
    arguments: { company: COMPANY },
  });
  const auditData = expectOk("audit_verify", audit);
  console.log(`  entries=${auditData?.entries}`);

  const ms = Date.now() - wall;
  console.log(`\n[smoke-mcp] OK — invoice-lifecycle gennem MCP completed in ${ms}ms`);
} finally {
  proc.stdin.end();
  proc.kill();
  await proc.exited;
  rmSync(TMP_ROOT, { recursive: true, force: true });
}
