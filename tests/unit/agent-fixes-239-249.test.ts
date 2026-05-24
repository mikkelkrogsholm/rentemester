// Tests: src/cli-meta.ts, src/cli/init.ts, src/cli/company.ts,
// src/cli-actor.ts, src/core/company.ts, src/core/ledger.ts,
// src/core/annual-report.ts — covers issues #239, #241, #242, #244, #248, #249.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { companyPaths } from "../../src/core/paths";

function run(args: string[], env?: Record<string, string>) {
  return Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: env ? { ...process.env, ...env } : process.env,
  });
}

describe("#239 — global usage does not mislabel file-writing commands as read-only", () => {
  test("init, company add and import contacts are NOT under the read-only heading", async () => {
    const proc = run(["--help"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);

    // Three groups now: read-only, setup (side-effecting), and ledger writes.
    const readIdx = stdout.indexOf("Læsekommandoer");
    const setupIdx = stdout.indexOf("Opsætningskommandoer");
    const writeIdx = stdout.indexOf("Skrivekommandoer");
    expect(readIdx).toBeGreaterThanOrEqual(0);
    expect(setupIdx).toBeGreaterThan(readIdx);
    expect(writeIdx).toBeGreaterThan(setupIdx);

    const readBlock = stdout.slice(readIdx, setupIdx);
    const setupBlock = stdout.slice(setupIdx, writeIdx);

    // The file/record-writing commands belong to the setup block, not read-only.
    for (const cmd of ["init", "company add", "import contacts"]) {
      expect(readBlock).not.toContain(`  ${cmd} `);
      expect(setupBlock).toContain(cmd);
    }
    // The setup heading must not present these commands as read-only —
    // the read-only heading itself does that, and these are NOT under it.
    expect(setupBlock).toContain("ikke read-only");
    expect(readBlock).toContain("read-only");
  });
});

describe("#241 — init/company add warn when no payment details are set", () => {
  // #241: the warning goes to stderr so JSON-consuming machines never see it
  // mixed into stdout, and so it survives `init > log.txt` redirection. Init
  // still succeeds (exit 0) — this is advisory, not fatal.
  test("init emits the missing-bank-details warning to stderr (not stdout)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-241-init-"));
    try {
      const company = join(root, "company");
      const proc = run(["init", "--company", company, "--format", "human"]);
      const stdout = await new Response(proc.stdout).text();
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(0);
      // Warning is on stderr…
      expect(stderr).toContain("ADVARSEL");
      expect(stderr).toContain("ingen betalingsoplysninger");
      expect(stderr).toContain("betalingsanvisning");
      expect(stderr).toContain("company set-profile");
      // …and never leaks into stdout, where the human onboarding block lives.
      expect(stdout).not.toContain("ADVARSEL");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #241: JSON output is for machines — stdout must stay parseable, so the
  // warning still routes to stderr even when --format json is used.
  test("init --format json keeps stdout parseable and still warns on stderr", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-241-init-json-warn-"));
    try {
      const company = join(root, "company");
      const proc = run(["init", "--company", company, "--format", "json"]);
      const stdout = await new Response(proc.stdout).text();
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(0);
      // stdout is a single clean JSON document.
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(true);
      expect(parsed.hasPaymentDetails).toBe(false);
      // The warning still appears, but only on stderr.
      expect(stderr).toContain("ADVARSEL");
      expect(stderr).toContain("ingen betalingsoplysninger");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("init does NOT warn when bank details are provided", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-241-init-bank-"));
    try {
      const company = join(root, "company");
      const proc = run([
        "init", "--company", company, "--format", "human",
        "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
      ]);
      const stdout = await new Response(proc.stdout).text();
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(0);
      expect(stdout).not.toContain("ADVARSEL — ingen betalingsoplysninger");
      // With bank details set, stderr stays clean of the warning.
      expect(stderr).not.toContain("ADVARSEL");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("init JSON output exposes hasPaymentDetails", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-241-init-json-"));
    try {
      const company = join(root, "company");
      const proc = run(["init", "--company", company, "--format", "json"]);
      const stdout = await new Response(proc.stdout).text();
      expect(await proc.exited).toBe(0);
      expect(JSON.parse(stdout).hasPaymentDetails).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("company add warns about missing bank details — on stderr, not stdout", async () => {
    const ws = mkdtempSync(join(tmpdir(), "rentemester-241-add-"));
    try {
      const proc = run(
        ["company", "add", "--name", "Acme ApS", "--format", "human"],
        { RENTEMESTER_WORKSPACE: ws, RENTEMESTER_COMPANY: "" },
      );
      const stdout = await new Response(proc.stdout).text();
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(0);
      // The warning is on stderr, mirroring `init`'s behavior.
      expect(stderr).toContain("ADVARSEL");
      expect(stderr).toContain("company set-profile");
      // …and is not duplicated into stdout (which carries the success line).
      expect(stdout).not.toContain("ADVARSEL");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#242 — Danish error messages", () => {
  test("report annual on a CVR-less company returns a Danish error", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-242-"));
    try {
      const company = join(root, "company");
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const proc = run([
        "report", "annual", "--company", company,
        "--from", "2025-01-01", "--to", "2025-12-31", "--format", "json",
      ]);
      const stdout = await new Response(proc.stdout).text();
      expect(await proc.exited).toBe(1);
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(false);
      const joined = JSON.stringify(parsed.errors);
      // The CVR error is Danish and uses the correct å spelling.
      expect(joined).toContain("CVR-nummer");
      expect(joined).toContain("årsrapport");
      // No English wording leaks through.
      expect(joined).not.toContain("company CVR is missing");
      expect(joined).not.toContain("arsrapport");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("#244 — inputNotes and --example accuracy", () => {
  test("expense book help explains --vat-treatment values", async () => {
    const proc = run(["expense", "book", "--help"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    expect(stdout).toContain("reverse_charge");
    expect(stdout).toContain("representation");
    expect(stdout).toContain("exempt");
  });

  test("asset write-off help explains --confirm yes and --threshold-source", async () => {
    const proc = run(["asset", "write-off", "--help"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    expect(stdout).toContain("--confirm yes");
    expect(stdout).toContain("--threshold-source");
  });

  test("invoice send help notes the SMTP transport is dry-run only", async () => {
    const proc = run(["invoice", "send", "--help"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);
    expect(stdout.toUpperCase()).toContain("DRY-RUN");
  });

  test("a command without an example rejects --example as an unknown flag", async () => {
    // `report trial-balance` has no examplePath, so --example must not be offered.
    const proc = run(["report", "trial-balance", "--example"]);
    const stderr = await new Response(proc.stderr).text();
    expect(await proc.exited).toBe(2);
    expect(stderr).toContain("Unknown flag --example");
    // The misleading "No example is registered" path is no longer reached.
    expect(stderr).not.toContain("No example is registered");
  });

  test("a command WITH an example still accepts --example", async () => {
    const proc = run(["invoice", "issue", "--example"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);
    expect(JSON.parse(stdout).invoiceType).toBe("full");
  });

  test("help for a command without an example does not list --example", async () => {
    const proc = run(["report", "trial-balance", "--help"]);
    const stdout = await new Response(proc.stdout).text();
    expect(await proc.exited).toBe(0);
    expect(stdout).not.toContain("--example");
  });
});

describe("#248 — actor allowlist consistency", () => {
  test("init seeds the onboarding OS user into the actor_allowlist", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-248-init-"));
    try {
      const company = join(root, "company");
      const proc = run(["init", "--company", company], { USER: "mikkel" });
      expect(await proc.exited).toBe(0);
      const policy = readFileSync(
        join(companyPaths(company).config, "policy.yaml"),
        "utf8",
      );
      // The derived OS user is now allowlisted, so a later explicit
      // --actor user:mikkel is accepted — consistent with the derived path.
      expect(policy).toContain("user:mikkel");
      expect(policy).toContain("user:ejer");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("an explicit --actor seeded at init works without hand-editing policy.yaml", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-248-explicit-"));
    try {
      const company = join(root, "company");
      // Onboard explicitly as user:freja.
      const initProc = run(["init", "--company", company, "--actor", "user:freja"]);
      expect(await initProc.exited).toBe(0);

      // Ingest a document (mutating) with the SAME explicit actor — accepted,
      // because init seeded it into the allowlist.
      const ingest = run([
        "documents", "ingest", "--company", company,
        "--file", "examples/vendor-invoice.txt",
        "--metadata", "examples/vendor-invoice.metadata.json",
        "--actor", "user:freja",
      ]);
      const stderr = await new Response(ingest.stderr).text();
      expect({ exitCode: await ingest.exited, stderr }).toEqual({
        exitCode: 0,
        stderr: "",
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("an unseeded explicit --actor is rejected with a hint on how to add it", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-248-reject-"));
    try {
      const company = join(root, "company");
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const proc = run([
        "documents", "ingest", "--company", company,
        "--file", "examples/vendor-invoice.txt",
        "--metadata", "examples/vendor-invoice.metadata.json",
        "--actor", "user:not-seeded",
      ]);
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(2);
      expect(stderr).toContain("is not in config/policy.yaml actor_allowlist");
      // The hint names the exact section and line to add.
      expect(stderr).toContain("actor_allowlist.users");
      expect(stderr).toContain("- user:not-seeded");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("#249 — chart of accounts includes owner's draw and a tax account", () => {
  test("seedAccounts seeds Privat hævning/indskud and a skattekonto", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-249-"));
    try {
      const db = openDb(join(dir, "ledger.sqlite"));
      migrate(db);
      seedAccounts(db);
      const rows = db
        .query("SELECT account_no, name, type, normal_balance FROM accounts")
        .all() as Array<{
          account_no: string;
          name: string;
          type: string;
          normal_balance: string;
        }>;
      const byNo = new Map(rows.map((r) => [r.account_no, r]));

      const draw = byNo.get("5010");
      expect(draw).toBeDefined();
      expect(draw!.name).toBe("Privat hævning");
      expect(draw!.type).toBe("equity");
      expect(draw!.normal_balance).toBe("debit");

      const contribution = byNo.get("5020");
      expect(contribution).toBeDefined();
      expect(contribution!.name).toBe("Privat indskud");
      expect(contribution!.type).toBe("equity");
      expect(contribution!.normal_balance).toBe("credit");

      const tax = byNo.get("7200");
      expect(tax).toBeDefined();
      expect(tax!.type).toBe("liability");
      expect(tax!.normal_balance).toBe("credit");
      expect(tax!.name.toLowerCase()).toContain("skat");

      db.close();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
