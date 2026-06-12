// Tests: src/mcp/server.ts, src/mcp/tools (MCP server end-to-end)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";

/**
 * Integration-test for MCP-server-scaffolden (#77).
 *
 * Spawner `bun src/mcp/server.ts` som child process, kører handshake
 * over stdio og verificerer at:
 *   1. `tools/list` returnerer både `audit_verify` og `journal_post`
 *   2. `tools/call audit_verify` på en fresh virksomhedsmappe svarer med
 *      envelope `{ ok: true, data: { entries: 0 }, errors: [] }`
 *
 * Bemærk: vi skriver MCP JSON-RPC manuelt på stdin/stdout for at
 * undgå at læne os op ad SDK-client-koden — vi tester at *serveren*
 * taler protokollen korrekt, ikke at SDK'en er konsistent med sig
 * selv.
 */

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id?: number;
  result?: any;
  error?: { code: number; message: string };
};

class StdioMcpClient {
  private proc: ReturnType<typeof Bun.spawn>;
  private stdoutReader: ReadableStreamDefaultReader<Uint8Array>;
  private decoder = new TextDecoder();
  private buffer = "";
  private nextId = 1;

  constructor() {
    this.proc = Bun.spawn(["bun", SERVER_PATH], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    this.stdoutReader = this.proc.stdout.getReader();
  }

  async send(method: string, params?: Record<string, unknown>): Promise<JsonRpcResponse> {
    const id = this.nextId++;
    const request = { jsonrpc: "2.0", id, method, params: params ?? {} };
    await this.proc.stdin.write(JSON.stringify(request) + "\n");
    await (this.proc.stdin as any).flush?.();
    return this.readResponse(id);
  }

  async notify(method: string, params?: Record<string, unknown>): Promise<void> {
    const note = { jsonrpc: "2.0", method, params: params ?? {} };
    await this.proc.stdin.write(JSON.stringify(note) + "\n");
    await (this.proc.stdin as any).flush?.();
  }

  private async readResponse(expectedId: number): Promise<JsonRpcResponse> {
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const newlineIdx = this.buffer.indexOf("\n");
      if (newlineIdx === -1) {
        const { value, done } = await this.stdoutReader.read();
        if (done) throw new Error("MCP server closed stdout before responding");
        this.buffer += this.decoder.decode(value, { stream: true });
        continue;
      }
      const line = this.buffer.slice(0, newlineIdx).trim();
      this.buffer = this.buffer.slice(newlineIdx + 1);
      if (!line) continue;
      const parsed: JsonRpcResponse = JSON.parse(line);
      if (parsed.id === expectedId) return parsed;
      // Anden notification/response — ignorér og fortsæt.
    }
    throw new Error(`Timed out waiting for MCP response id=${expectedId}`);
  }

  async close(): Promise<void> {
    try {
      this.proc.stdin.end();
    } catch {}
    try {
      this.stdoutReader.releaseLock();
    } catch {}
    this.proc.kill();
    await this.proc.exited;
  }
}

let companyRoot: string;
let client: StdioMcpClient;
let initResult: any;

beforeAll(async () => {
  companyRoot = mkdtempSync(join(tmpdir(), "mcp-test-company-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.close();

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-test-harness", version: "0.0.1" },
  });
  expect(initResponse.error).toBeUndefined();
  expect(initResponse.result?.serverInfo?.name).toBe("rentemester-mcp");
  initResult = initResponse.result;
  await client.notify("notifications/initialized");
});

afterAll(async () => {
  await client.close();
  if (companyRoot && existsSync(companyRoot)) {
    rmSync(companyRoot, { recursive: true, force: true });
  }
});

describe("MCP server scaffold", () => {
  test("initialize returns a non-empty instructions string orienting the agent", () => {
    // #203 — the server must hand the agent an orienting prose block,
    // not `instructions: null`.
    const instructions = initResult?.instructions;
    expect(typeof instructions).toBe("string");
    expect(instructions.length).toBeGreaterThan(200);
    // It must mention the load-bearing conventions an agent needs.
    expect(instructions).toContain("confirm");
    expect(instructions).toContain("company");
    expect(instructions).toContain("confirmText");
    // ...and point at the full contract.
    expect(instructions).toContain("docs/mcp-agent-contract.md");
  });

  test("#245 — instructions warn that a schema-invalid payload yields a raw -32602", () => {
    // An agent reading only `instructions` must not assume every error is the
    // { ok, errors[] } envelope: a schema-invalid payload is rejected by the
    // SDK before the handler and comes back as a raw JSON-RPC -32602 error.
    const instructions: string = initResult?.instructions ?? "";
    expect(instructions).toContain("-32602");
    // It must say this reply has no structuredContent / envelope.
    expect(instructions).toContain("structuredContent");
  });

  test("lists audit_verify and journal_post via tools/list", async () => {
    const response = await client.send("tools/list");
    expect(response.error).toBeUndefined();
    const names = (response.result?.tools ?? []).map((tool: any) => tool.name);
    expect(names).toContain("audit_verify");
    expect(names).toContain("journal_post");
  });

  test("audit_verify on a fresh company returns ok envelope with zero entries", async () => {
    const response = await client.send("tools/call", {
      name: "audit_verify",
      arguments: { company: companyRoot },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured).toBeDefined();
    expect(structured.ok).toBe(true);
    expect(structured.errors).toEqual([]);
    expect(structured.data?.entries).toBe(0);
    expect(response.result?.isError).toBe(false);
  });

  test("journal_post without confirm:true returns envelope error", async () => {
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: companyRoot,
        payload: {
          transactionDate: "2026-05-18",
          text: "Should be rejected",
          lines: [
            { accountNo: "2000", debitAmount: 100 },
            { accountNo: "1000", creditAmount: 100 },
          ],
        },
        confirm: false,
      },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.some((message: string) => message.includes("confirm: true required"))).toBe(true);
  });

  test("the opt-in backup lock blocks a bookkeeping write tool over MCP but exempts system_backup", async () => {
    const lockedRoot = mkdtempSync(join(tmpdir(), "mcp-lock-company-"));
    try {
      const paths = ensureCompanyDirs(lockedRoot);
      const db = openDb(paths.db);
      migrate(db);
      seedAccounts(db);
      // Old bookkeeping activity with no backup -> a weekly backup is overdue.
      db.run(
        "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
        "2026-01-01",
        "2026-01-01",
        "Old activity",
        100,
        "mcp-lock-ref",
        "mcp-lock-batch",
        "mcp-lock-hash",
        "mcp-lock-tx",
      );
      db.close();

      const lockResp = await client.send("tools/call", {
        name: "system_backup_lock",
        arguments: { company: lockedRoot, enforced: true, graceDays: 0, confirm: true },
      });
      expect(lockResp.result?.structuredContent?.ok).toBe(true);

      // A bookkeeping write tool is refused — the lock holds on the agent surface.
      const blocked = await client.send("tools/call", {
        name: "journal_post",
        arguments: {
          company: lockedRoot,
          payload: {
            transactionDate: "2026-05-18",
            text: "Should be locked out",
            lines: [
              { accountNo: "2000", debitAmount: 100 },
              { accountNo: "1000", creditAmount: 100 },
            ],
          },
          confirm: true,
        },
      });
      const blockedStructured = blocked.result?.structuredContent;
      expect(blockedStructured?.ok).toBe(false);
      expect(blockedStructured?.errors?.some((m: string) => m.includes("låst"))).toBe(true);
      // #375 — stable machine-readable marker so agents don't have to parse
      // free-form Danish text to detect the backup lock.
      expect(blockedStructured?.code).toBe("BACKUP_LOCKED");
      // #375 — the recovery path must be named explicitly. system_backup_status
      // is the first stop (read-only diagnosis) before the agent has to commit
      // to a write (system_backup / system_backup_place).
      expect(
        blockedStructured?.errors?.some((m: string) => m.includes("system_backup_status")),
      ).toBe(true);

      // system_backup is exempt — backing up is the way out of the lock.
      const backup = await client.send("tools/call", {
        name: "system_backup",
        arguments: { company: lockedRoot, at: "2026-05-21T10:00:00.000Z", confirm: true },
      });
      expect(backup.result?.structuredContent?.ok).toBe(true);
    } finally {
      rmSync(lockedRoot, { recursive: true, force: true });
    }
  });
});

describe("MCP tools full surface (#78)", () => {
  // Sammenligner mod den fulde tool-surface defineret i
  // docs/mcp-tool-surface.md. Hvis et tool-navn flytter eller forsvinder
  // skal denne liste opdateres bevidst.
  const EXPECTED_TOOLS = [
    // read
    "accounts_list",
    "audit_verify",
    "bank_list",
    "bank_suggest_matches",
    "customer_list",
    "customer_validate_vat",
    "documents_list",
    "exceptions_list",
    "invoice_compensation_calc",
    "invoice_find",
    "invoice_interest_calc",
    "invoice_list",
    "invoice_overdue",
    "invoice_status",
    "invoice_validate",
    "journal_dry_run",
    "journal_list",
    "period_list",
    "reconcile_bank",
    "retention_status",
    "system_backup_status",
    "system_healthcheck",
    "vat_report",
    "vendor_list",
    // write-reversible
    "bank_import",
    "customer_create",
    "documents_ingest",
    "exception_resolve",
    "vendor_create",
    // write-irreversible
    "expense_book",
    "invoice_apply_payment",
    "invoice_claim_compensation",
    "invoice_claim_interest",
    "invoice_credit_note",
    "invoice_issue",
    "invoice_post",
    "invoice_post_compensation",
    "invoice_post_interest",
    "invoice_post_reminder",
    "invoice_refund_bank",
    "invoice_remind",
    "invoice_render",
    "invoice_settle_bank",
    "invoice_settle_claim_bank",
    "invoice_write_off_bad_debt",
    "journal_post",
    "journal_reverse",
    "period_close",
    "system_backup",
    "system_export_authority",
    "vat_post_eu_service_purchase",
    "vat_post_representation_purchase",
    // destructive
    "system_restore_backup",
  ] as const;

  test("tools/list returns all expected tools from the surface spec", async () => {
    const response = await client.send("tools/list");
    expect(response.error).toBeUndefined();
    const names = new Set((response.result?.tools ?? []).map((tool: any) => tool.name));
    for (const expected of EXPECTED_TOOLS) {
      expect(names.has(expected)).toBe(true);
    }
    expect(names.size).toBeGreaterThanOrEqual(EXPECTED_TOOLS.length);
  });

  test("accounts_list returns seeded chart of accounts", async () => {
    const response = await client.send("tools/call", {
      name: "accounts_list",
      arguments: { company: companyRoot },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(Array.isArray(structured?.data?.accounts)).toBe(true);
    expect(structured?.data?.count).toBeGreaterThan(0);
  });

  test("bank_list on a fresh company returns empty result set", async () => {
    const response = await client.send("tools/call", {
      name: "bank_list",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(Array.isArray(structured?.data?.rows)).toBe(true);
  });

  test("invoice_list on a fresh company returns an empty list with ok=true", async () => {
    const response = await client.send("tools/call", {
      name: "invoice_list",
      arguments: { company: companyRoot, status: "all" },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.count).toBe(0);
  });

  test("journal_list returns zero entries on a fresh company", async () => {
    const response = await client.send("tools/call", {
      name: "journal_list",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.count).toBe(0);
  });

  test("exceptions_list returns empty list with ok=true", async () => {
    const response = await client.send("tools/call", {
      name: "exceptions_list",
      arguments: { company: companyRoot, status: "open" },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.count).toBe(0);
  });

  test("vat_report on a fresh company returns ok envelope with zero amounts", async () => {
    const response = await client.send("tools/call", {
      name: "vat_report",
      arguments: { company: companyRoot, from: "2026-01-01", to: "2026-12-31" },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(typeof structured?.data).toBe("object");
  });

  test("system_healthcheck reports OK for fresh company root", async () => {
    const response = await client.send("tools/call", {
      name: "system_healthcheck",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.ok).toBe(true);
  });

  test("retention_status returns ok envelope", async () => {
    const response = await client.send("tools/call", {
      name: "retention_status",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
  });

  test("period_list returns zero periods on a fresh company", async () => {
    const response = await client.send("tools/call", {
      name: "period_list",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.count).toBe(0);
  });

  test("customer_create without confirm:true returns envelope error", async () => {
    const response = await client.send("tools/call", {
      name: "customer_create",
      arguments: {
        company: companyRoot,
        input: { name: "Test ApS" },
        confirm: false,
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.some((m: string) => m.includes("confirm: true required"))).toBe(true);
  });

  test("invoice_issue without confirm:true returns envelope error", async () => {
    // payload only needs to satisfy the typed input schema structurally; the
    // confirm:false gate fires in the handler regardless of payload validity.
    const response = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: {
        company: companyRoot,
        payload: { invoiceType: "full", invoiceNumber: "X" },
        confirm: false,
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.[0]).toContain("confirm: true required");
  });

  test("system_restore_backup rejects missing confirmText", async () => {
    const response = await client.send("tools/call", {
      name: "system_restore_backup",
      arguments: {
        backupDir: "/tmp/does-not-exist",
        targetCompany: "/tmp/does-not-exist-target",
        confirm: true,
        confirmText: "wrong",
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.[0]).toContain("confirmText must match");
  });

  test("system_restore_backup rejects missing confirm:true", async () => {
    const response = await client.send("tools/call", {
      name: "system_restore_backup",
      arguments: {
        backupDir: "/tmp/x",
        targetCompany: "/tmp/y",
        confirm: false,
        confirmText: "RESTORE /tmp/y",
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.[0]).toContain("confirm: true required");
  });

  test("#307 — system_restore_backup with confirm:true but omitted confirmText returns the envelope, not -32602", async () => {
    // confirmText is schema-optional: an omitted confirmText must reach the
    // handler and yield the normal { ok:false, errors:[...] } envelope —
    // exactly like a confirmText MISMATCH — never a raw -32602 input error.
    const response = await client.send("tools/call", {
      name: "system_restore_backup",
      arguments: {
        backupDir: "/tmp/does-not-exist",
        targetCompany: "/tmp/does-not-exist-target",
        confirm: true,
      },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured, "expected an envelope, got a raw -32602").toBeDefined();
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.[0]).toContain("confirmText must match");
  });

  test("#306 — system_restore_backup accepts a publicKey (ed25519) parameter", async () => {
    // The ed25519 public key must be reachable from MCP, mirroring the CLI's
    // --public-key. Supplying it on a non-existent backup still surfaces a
    // normal envelope (no schema rejection of the publicKey field).
    const response = await client.send("tools/call", {
      name: "system_restore_backup",
      arguments: {
        backupDir: "/tmp/does-not-exist",
        targetCompany: "/tmp/does-not-exist-target",
        publicKey: "/tmp/does-not-exist.pub",
        confirm: true,
        confirmText: "RESTORE /tmp/does-not-exist-target",
      },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured, "publicKey must be an accepted schema field").toBeDefined();
    expect(structured?.ok).toBe(false);
  });

  test("missing company path error does not leak the absolute path to the caller", async () => {
    const secretPath = join(tmpdir(), "rentemester-secret-host-dir-abc123");
    const response = await client.send("tools/call", {
      name: "accounts_list",
      arguments: { company: secretPath },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.length).toBeGreaterThan(0);
    const joined = (structured?.errors ?? []).join(" | ");
    expect(joined).not.toContain(secretPath);
    expect(joined).not.toContain(tmpdir());
  });

  test("#228 — audit_verify on a missing company does not leak the absolute host path", async () => {
    // audit_verify previously hand-rolled its own existsSync check and
    // returned `company path does not exist: <absolute path>` — disclosing
    // the host layout. It now routes through withCompanyDb, which redacts.
    const secretPath = join(tmpdir(), "rentemester-secret-audit-dir-228");
    const response = await client.send("tools/call", {
      name: "audit_verify",
      arguments: { company: secretPath },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.length).toBeGreaterThan(0);
    const joined = (structured?.errors ?? []).join(" | ");
    expect(joined).not.toContain(secretPath);
    expect(joined).not.toContain(tmpdir());
  });

  test("#228 — journal_post on a missing company does not leak the absolute host path", async () => {
    // journal_post likewise hand-rolled its existsSync check and leaked the
    // path. It now routes through withCompanyDbConfirmed, which redacts.
    const secretPath = join(tmpdir(), "rentemester-secret-journal-dir-228");
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: secretPath,
        payload: {
          transactionDate: "2026-05-18",
          text: "Should be redacted",
          lines: [
            { accountNo: "2000", debitAmount: 100 },
            { accountNo: "1000", creditAmount: 100 },
          ],
        },
        confirm: true,
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.length).toBeGreaterThan(0);
    const joined = (structured?.errors ?? []).join(" | ");
    expect(joined).not.toContain(secretPath);
    expect(joined).not.toContain(tmpdir());
  });

  test("#229 — missing confirm + a schema error returns the documented raw -32602 form", async () => {
    // The agent contract documents this case: when `confirm` is omitted AND
    // the payload also has a schema error (here an empty lines[]), the SDK's
    // input validation rejects the call before the handler runs — so the
    // reply is a raw -32602 with isError:true and NO structuredContent.
    // An agent must branch on this shape; this test pins the contract.
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: companyRoot,
        payload: {
          transactionDate: "2026-05-18",
          text: "schema-invalid + confirm omitted",
          lines: [], // schema error: at least one line is required
        },
        // confirm intentionally omitted
      },
    });
    // No structured envelope: isError true, structuredContent absent.
    expect(response.result?.isError).toBe(true);
    expect(response.result?.structuredContent).toBeUndefined();
    // The cause is human-readable in the text content and names the -32602
    // form an agent is told (in docs/mcp-agent-contract.md) to detect.
    const text: string = response.result?.content?.[0]?.text ?? "";
    expect(text).toContain("-32602");
    expect(text).toContain("Input validation error");
    expect(text).toContain("journal_post");
  });

  test("customer_create with confirm:true persists the customer", async () => {
    const response = await client.send("tools/call", {
      name: "customer_create",
      arguments: {
        company: companyRoot,
        input: { name: "MCP Test Customer" },
        confirm: true,
      },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(typeof structured?.data?.customerId).toBe("number");

    const list = await client.send("tools/call", {
      name: "customer_list",
      arguments: { company: companyRoot },
    });
    const listed = list.result?.structuredContent;
    expect(listed?.ok).toBe(true);
    expect(listed?.data?.count).toBeGreaterThanOrEqual(1);
  });
});
