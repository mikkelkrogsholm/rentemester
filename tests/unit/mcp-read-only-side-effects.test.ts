import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Database } from "bun:sqlite";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { getCachedCvrLookup } from "../../src/core/cvr";
import { companyPaths } from "../../src/core/paths";
import { listWorkspaceCompanies } from "../../src/core/workspace";
import { runMcpReadOnlyTool, withCompanyDb, withCompanyDbConfirmed, withCompanyReadOnlyDb } from "../../src/mcp/tool-runtime";
import { deriveMcpActor } from "../../src/mcp/actor";
import { successEnvelope } from "../../src/mcp/envelope";
import { lockGuardServer } from "../../src/mcp/registry";
import { registerPortfolioTools } from "../../src/mcp/tools/portfolio";
import { registerCvrTools } from "../../src/mcp/tools/cvr";

function treeDigest(root: string): string {
  const hash = createHash("sha256");
  const visit = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(dir, entry.name);
      hash.update(`${entry.isDirectory() ? "d" : "f"}:${relative(root, path)}\0`);
      if (entry.isDirectory()) visit(path); else hash.update(readFileSync(path));
    }
  };
  visit(root);
  return hash.digest("hex");
}

const server = new McpServer({ name: "readonly-contract", version: "0" });

function checkpointFixture(path:string) { const db=new Database(path); db.run("PRAGMA wal_checkpoint(TRUNCATE)"); db.close(); }

describe("MCP company read-only opening contract (#586)", () => {
  test("concurrent confirmed writes do not grant read callbacks a writable handle", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-mcp-opening-isolation-"));
    let releaseWrite!: () => void;
    let writeOpened!: () => void;
    const opened = new Promise<void>(resolve => { writeOpened = resolve; });
    const release = new Promise<void>(resolve => { releaseWrite = resolve; });
    let writeResult: Promise<unknown> | undefined;
    try {
      const reader = createCompany(workspace, { name: "Synthetic Reader", slug: "reader" });
      const writer = createCompany(workspace, { name: "Synthetic Writer", slug: "writer", onboardingActor: deriveMcpActor(server.server.getClientVersion()).createdBy });
      checkpointFixture(companyPaths(reader.companyRoot).db);
      const before = treeDigest(reader.companyRoot);
      const inspectRead = ({ db, args }: { db: Database; args: { company: string } }) => {
        expect(args.company).toBe(reader.companyRoot);
        expect(() => db.exec("CREATE TABLE forbidden_read_write (id INTEGER)")).toThrow();
        return successEnvelope({ readOnly: true });
      };
      const read = withCompanyDb(server, inspectRead);
      const explicitRead = withCompanyReadOnlyDb(inspectRead);
      const write = withCompanyDbConfirmed<{ company: string; confirm?: boolean }>(server, "synthetic_write", async ({ db }) => {
        db.exec("CREATE TABLE permitted_write (id INTEGER)");
        writeOpened();
        await release;
        // Even inside the write's async context, the registration boundary is read-only.
        expect((await runMcpReadOnlyTool(() => read({ company: reader.companyRoot }))).structuredContent).toMatchObject({ ok: true });
        expect((await explicitRead({ company: reader.companyRoot })).structuredContent).toMatchObject({ ok: true });
        return successEnvelope({ written: true });
      });
      writeResult = write({ company: writer.companyRoot, confirm: true });
      await opened;
      expect((await read({ company: reader.companyRoot })).structuredContent).toMatchObject({ ok: true });
      releaseWrite();
      expect((await writeResult as { structuredContent: unknown }).structuredContent).toMatchObject({ ok: true });
      expect(treeDigest(reader.companyRoot)).toBe(before);
    } finally {
      releaseWrite();
      await writeResult;
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("both read wrappers close handles and redact asynchronous handler failures", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-mcp-read-failure-"));
    try {
      const created = createCompany(workspace, { name: "Synthetic Read Failure", slug: "read-failure" });
      checkpointFixture(companyPaths(created.companyRoot).db);
      const before = treeDigest(created.companyRoot);
      let handle: Database | undefined;
      const fail = async ({ db }: { db: Database }) => {
        handle = db;
        await Promise.resolve();
        throw new Error(`synthetic failure at ${created.companyRoot}/secret`);
      };
      for (const call of [withCompanyDb(server, fail), withCompanyReadOnlyDb(fail)]) {
        const result = await call({ company: created.companyRoot });
        expect(result.structuredContent).toMatchObject({ ok: false, errors: ["synthetic failure at <path>"] });
        expect(() => handle!.query("SELECT 1").get()).toThrow();
        expect(treeDigest(created.companyRoot)).toBe(before);
      }
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("default shared runtime is snapshot-only for missing, uninitialised and pending ledgers", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-readonly-"));
    try {
      const missing = join(workspace, "missing");
      const uninitialised = join(workspace, "uninitialised");
      mkdirSync(uninitialised);
      writeFileSync(join(uninitialised, ".keep"), "synthetic");
      const created = createCompany(workspace, { name: "Synthetic ApS", slug: "synthetic" });
      const dbPath = companyPaths(created.companyRoot).db;
      const writable = openDb(dbPath);
      try { writable.exec("DELETE FROM schema_migrations WHERE id = (SELECT MAX(id) FROM schema_migrations)"); }
      finally { writable.close(); }
      checkpointFixture(dbPath);

      const call = withCompanyDb<{ company: string }>(server, ({ db }) => successEnvelope({ tables: Number((db.query("SELECT COUNT(*) AS count FROM sqlite_master").get() as { count: number }).count) }));
      const companiesBefore = listWorkspaceCompanies(workspace);
      for (const root of [workspace, created.companyRoot]) {
        const before = treeDigest(root);
        const target = root === workspace ? missing : created.companyRoot;
        const result = await call({ company: target });
        expect((result.structuredContent as { ok: boolean }).ok).toBe(false);
        expect(treeDigest(root)).toBe(before);
      }
      const beforeUninitialised = treeDigest(uninitialised);
      const result = await call({ company: uninitialised });
      expect((result.structuredContent as { ok: boolean }).ok).toBe(false);
      expect(treeDigest(uninitialised)).toBe(beforeUninitialised);
      expect(listWorkspaceCompanies(workspace)).toEqual(companiesBefore);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("registration rejects a company-scoped read handler outside the strict runtime", () => {
    const guarded = lockGuardServer(new McpServer({ name: "unsafe-read-registration", version: "0" }));
    expect(() => (guarded.registerTool as any)(
      "unsafe_read",
      { inputSchema: { company: {} }, annotations: { readOnlyHint: true } },
      async () => undefined,
    )).toThrow(
      "read-only MCP tool unsafe_read must use the strict company read runtime",
    );
  });

  test("explicit snapshot runtime preserves an initialized ledger byte-for-byte", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-readonly-current-"));
    try {
      const created = createCompany(workspace, { name: "Current ApS", slug: "current" });
      checkpointFixture(companyPaths(created.companyRoot).db);
      const before = treeDigest(created.companyRoot);
      const call = withCompanyReadOnlyDb<{ company: string }>(({ db }) => successEnvelope({ companyCount: (db.query("SELECT COUNT(*) AS count FROM companies").get() as { count: number }).count }));
      const result = await call({ company: created.companyRoot });
      expect((result.structuredContent as { ok: boolean }).ok).toBe(true);
      expect(treeDigest(created.companyRoot)).toBe(before);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("portfolio overview is a snapshot-only fan-out", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-portfolio-readonly-"));
    try {
      const created=createCompany(workspace, { name: "Portfolio ApS", slug: "portfolio", cvr: "DK90000103" });
      checkpointFixture(companyPaths(created.companyRoot).db);
      const portfolio = new McpServer({ name: "portfolio-readonly", version: "0" });
      registerPortfolioTools(portfolio);
      const handler = ((portfolio as any)._registeredTools as Record<string, { handler: Function }>).portfolio_overview.handler;
      const before = treeDigest(workspace);
      const result = await handler({ workspace, asOf: "2026-01-01" }, { signal: new AbortController().signal });
      expect(result.structuredContent.ok).toBe(true);
      expect(treeDigest(workspace)).toBe(before);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("cache-writing CVR lookup is a confirmed write and succeeds on a fresh provider result", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-cvr-write-"));
    const previousFetch = globalThis.fetch;
    const previousAgent = process.env.RENTEMESTER_MCP_AGENT;
    const previousUsername = process.env.CVR_USERNAME;
    const previousPassword = process.env.CVR_PASSWORD;
    try {
      const created = createCompany(workspace, { name: "CVR Cache ApS", slug: "cvr-cache" });
      process.env.RENTEMESTER_MCP_AGENT = "rentemester-bookkeeper";
      process.env.CVR_USERNAME = "synthetic-user";
      process.env.CVR_PASSWORD = "synthetic-password";
      globalThis.fetch = async () => Response.json({
        hits: { total: 1, hits: [{ _source: { Vrvirksomhed: { cvrNummer: 12345678, navne: [{ navn: "Provider Result ApS", periode: { gyldigTil: null } }], virksomhedMetadata: { nyesteNavn: { navn: "Provider Result ApS" } } } } }] },
      });
      const cvrServer = new McpServer({ name: "cvr-write", version: "0" });
      registerCvrTools(cvrServer);
      const tool = ((cvrServer as any)._registeredTools as Record<string, { annotations: any; inputSchema: any; handler: Function }>).cvr_lookup;
      expect(tool.annotations.readOnlyHint).toBe(false);
      expect(tool.inputSchema.shape.confirm).toBeDefined();
      const rejected = await tool.handler({ company: created.companyRoot, cvr: "12345678" });
      expect(rejected.structuredContent.ok).toBe(false);
      const accepted = await tool.handler({ company: created.companyRoot, cvr: "12345678", confirm: true });
      expect(accepted.structuredContent.ok).toBe(true);
      const db = openDb(companyPaths(created.companyRoot).db);
      try { expect(getCachedCvrLookup(db, "12345678")?.company.name).toBe("Provider Result ApS"); }
      finally { db.close(); }
    } finally {
      globalThis.fetch = previousFetch;
      if (previousUsername === undefined) delete process.env.CVR_USERNAME; else process.env.CVR_USERNAME = previousUsername;
      if (previousPassword === undefined) delete process.env.CVR_PASSWORD; else process.env.CVR_PASSWORD = previousPassword;
      if (previousAgent === undefined) delete process.env.RENTEMESTER_MCP_AGENT; else process.env.RENTEMESTER_MCP_AGENT = previousAgent;
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
