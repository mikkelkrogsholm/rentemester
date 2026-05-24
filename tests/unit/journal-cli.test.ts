// Tests: src/cli/journal.ts, src/cli.ts (journal CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";

describe("journal post CLI", () => {
  test("posts a valid journal entry against an ingested document", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journalcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "journal", "post", "--company", company, "--input", "examples/journal-entry.expense.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.entryNo).toContain("2026-");
  });

  test("rejects explicit actor overrides that are not allowlisted", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journalcli-actor-deny-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "journal", "post",
      "--company", company,
      "--input", "examples/journal-entry.expense.json",
      "--actor", "agent:freja",
      "--actor-via", "openclaw",
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    expect(exitCode).toBe(2);
    expect(stdout).toBe("");
    expect(stderr).toContain("actor 'agent:freja' is not in config/policy.yaml actor_allowlist");
  });

  test("threads explicit actor attribution into journal entries and audit log", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journalcli-actor-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    writeFileSync(join(companyPaths(company).config, "policy.yaml"), `company_policy:\n  country: DK\n  currency: DKK\n  allow_direct_sql_write: false\n  block_if_uncertain: true\nactor_allowlist:\n  agents:\n    - freja\n`);
    // #248: the seeded init user was overwritten above, so the derived OS
    // actor is no longer in the allowlist — the ingest must use the explicit
    // agent:freja actor (the only one this rewritten allowlist permits).
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json --actor agent:freja`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "journal", "post",
      "--company", company,
      "--input", "examples/journal-entry.expense.json",
      "--actor", "agent:freja",
      "--actor-via", "openclaw"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    const db = openDb(companyPaths(company).db);
    migrate(db);
    const entry = db.query("SELECT created_by, created_by_program FROM journal_entries ORDER BY id DESC LIMIT 1").get() as any;
    const audit = db.query("SELECT actor FROM audit_log WHERE event_type = 'journal_post' ORDER BY id DESC LIMIT 1").get() as any;
    db.close();

    rmSync(root, { recursive: true, force: true });
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    expect(entry).toEqual({ created_by: "agent:freja", created_by_program: "openclaw" });
    expect(audit.actor).toBe("agent:freja via openclaw");
  });

  test("fails closed for mutating commands when no actor can be inferred", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journalcli-actor-missing-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "journal", "post",
      "--company", company,
      "--input", "examples/journal-entry.expense.json",
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        USER: "",
        LOGNAME: "",
        OPENCLAW_AGENT: "",
        RENTEMESTER_AGENT: "",
        RENTEMESTER_USER: "",
        RENTEMESTER_ACTOR: "",
        RENTEMESTER_ACTOR_VIA: "",
      },
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    expect(exitCode).toBe(2);
    expect(stdout).toBe("");
    expect(stderr).toContain("actor required for mutations");
  });
});

describe("journal dry-run CLI", () => {
  test("previews an entry without writing it to the ledger", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journaldryrun-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const payloadPath = join(root, "entry.json");
    writeFileSync(
      payloadPath,
      JSON.stringify({
        transactionDate: "2026-05-16",
        text: "Owner contribution",
        lines: [
          { accountNo: "2000", debitAmount: 1000 },
          { accountNo: "5020", creditAmount: 1000 },
        ],
      }),
    );

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "journal", "dry-run", "--company", company, "--input", payloadPath],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.entryNo).toBe("2026-00001");
    expect(parsed.accountEffects).toHaveLength(2);

    // The dry run must not have written anything: the ledger is still empty.
    const db = openDb(companyPaths(company).db);
    const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
    db.close();
    rmSync(root, { recursive: true, force: true });
    expect(count.n).toBe(0);
  });
});
