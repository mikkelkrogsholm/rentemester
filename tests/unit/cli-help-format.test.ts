// Tests: src/cli-meta.ts, src/cli.ts (CLI help formatting)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("CLI help, examples, and human formatting", () => {
  test("prints per-command help for invoice issue", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    expect(stdout).toContain("rentemester invoice issue --company <path> --input <file.json>");
    expect(stdout).toContain("invoiceType");
    expect(stdout).toContain("rentemester invoice issue --example > faktura.json");
  });

  test("prints a valid example payload for invoice issue", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--example"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.invoiceType).toBe("full");
    expect(parsed.currency).toBe("DKK");
  });

  test("suggests the right flag name for command-local typos", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--companies", "/tmp/company", "--input", "examples/full-invoice.dk.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect(exitCode).toBe(2);
    expect(stdout).toBe("");
    expect(stderr).toContain("Unknown flag --companies for invoice issue. Did you mean --company?");
  });

  test("keeps JSON as the default for piped success output", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "validate", "--input", "examples/full-invoice.dk.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    expect(JSON.parse(stdout).ok).toBe(true);
  });

  test("renders human success output when requested", async () => {
    // #250: `invoice validate` has a dedicated Danish human renderer — a valid
    // payload states the verdict plainly, with no raw JSON.
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "validate", "--input", "examples/full-invoice.dk.json", "--format", "human"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    expect(stdout).toContain("Fakturaen er gyldig og kan udstedes.");
    expect(stdout).not.toContain('"ok"');
  });

  test("renders human error output when requested", async () => {
    // #250: an invalid payload renders the dedicated Danish verdict on stderr
    // with every concrete rejection reason, and exits 1.
    const dir = mkdtempSync(join(tmpdir(), "rentemester-cli-human-"));
    const file = join(dir, "bad-invoice.json");
    writeFileSync(file, JSON.stringify({ invoiceType: "full", currency: "DKK" }, null, 2));

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "validate", "--input", file, "--format", "human"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(dir);
    expect(exitCode).toBe(1);
    expect(stdout).toBe("");
    expect(stderr).toContain("Fakturaen er IKKE gyldig");
    expect(stderr).toContain("issueDate must be present in YYYY-MM-DD format");
    expect(stderr).not.toContain('"errors"');
  });

  // #222: `invoice issue --help` must state vatRate as a fraction, never a
  // percent — the payload field totals.vatRate is 0.25, not 25.
  test("invoice issue help states vatRate is a fraction, not a percent", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("vatRate er en BRØK");
    expect(stdout).toContain("0.25");
    expect(stdout).not.toMatch(/vatRate er en procent/);
  });

  // #225: command metadata must render Danish characters correctly — no
  // arsrapport/regnskabsaar/ledelsespategning mojibake.
  test("report annual help renders Danish characters correctly", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "report", "annual", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("årsrapport");
    expect(stdout).toContain("regnskabsår");
    expect(stdout).toContain("ledelsespåtegning");
    expect(stdout).not.toContain("arsrapport");
    expect(stdout).not.toContain("regnskabsaar");
    expect(stdout).not.toContain("ledelsespategning");
  });

  // #227: `period close` must document the closed/reported contract.
  // #247: it must point to the controlled `period reopen` recovery path.
  test("period close help documents the closed-vs-reported contract", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "period", "close", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    expect(stdout).toContain("Standard: closed");
    expect(stdout).toMatch(/reported/);
    // The dead-end "edit the database directly" advice is gone; the help now
    // points at the controlled, audit-logged reopen path.
    expect(stdout).not.toMatch(/redigere ledger-databasen/i);
    expect(stdout).toContain("period reopen");
  });

  // #247: `period reopen` help must document the controlled, audit-logged
  // recovery path and the actor + reason requirements.
  test("period reopen help documents the controlled recovery path", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "period", "reopen", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    expect(stdout).toContain("--reason");
    expect(stdout).toMatch(/revisionssporet/i);
    expect(stdout).toMatch(/audit-log/i);
    expect(stdout).toMatch(/reported.*kan IKKE åbnes igen/i);
  });

  // #231: asset commands must carry inputNotes.
  test("asset register and asset depreciate help carry inputNotes", async () => {
    for (const sub of ["register", "depreciate"]) {
      const proc = Bun.spawn(["bun", "run", "src/cli.ts", "asset", sub, "--help"], {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
      });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      expect(stdout).toContain("Inputnoter:");
    }
  });

  // #297: `system restore-backup --help` documents --public-key in its
  // Inputnoter — previously the flag appeared in "Tilladte flags" but only
  // --verify-key was explained, leaving the asymmetric-signature flag a
  // mystery.
  test("system restore-backup help documents --public-key", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "restore-backup", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    // The Inputnoter block now explains --public-key, not just --verify-key.
    expect(stdout).toMatch(/--public-key/);
    expect(stdout).toMatch(/--verify-key/);
    // It is described as the ed25519 / asymmetric-signature key.
    expect(stdout.toLowerCase()).toContain("ed25519");
    // And points the reader at the backup-security doc.
    expect(stdout).toContain("docs/backup-security.md");
  });

  // #289: `init --help` must document the new --vat-period flag so the owner
  // can actually act on the "afregner du måneds- eller halvårsmoms"-advice.
  test("init help documents the --vat-period flag", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "init", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Inputnoter:");
    expect(stdout).toContain("--vat-period");
    expect(stdout).toMatch(/month\|quarter\|half-year/);
  });

  // #231: recurring-invoice create and opening-balance post advertise
  // --example; the registered example files must exist and parse.
  test("recurring-invoice create --example emits a parseable template", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "recurring-invoice", "create", "--example"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout);
    expect(parsed.interval).toBe("monthly");
    expect(parsed.invoice).toBeDefined();
  });

  test("opening-balance post --example emits a balanced primobalance", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "opening-balance", "post", "--example"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout);
    const debit = parsed.lines.reduce((s: number, l: any) => s + (l.debitAmount ?? 0), 0);
    const credit = parsed.lines.reduce((s: number, l: any) => s + (l.creditAmount ?? 0), 0);
    expect(debit).toBe(credit);
  });

  // #231: bank import must document its CSV format.
  test("bank import help documents the CSV columns and sign convention", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "bank", "import", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("transaction_date");
    expect(stdout).toMatch(/POSITIVT bel/);
  });

  // #231: `accounts list` must honour --json / --format json instead of
  // always printing a console.table.
  test("accounts list honours --format json", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-accounts-json-"));
    const company = join(dir, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const proc = Bun.spawn(
        ["bun", "run", "src/cli.ts", "accounts", "list", "--company", company, "--format", "json"],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(true);
      expect(Array.isArray(parsed.rows)).toBe(true);
      expect(parsed.rows.length).toBeGreaterThan(0);
      expect(parsed.rows[0]).toHaveProperty("account_no");
    } finally {
      cleanupDir(dir);
    }
  });

  // #262: when --example prints only a --metadata payload (not a complete
  // call), the help must say so — an agent must not pipe the example in as
  // if it were the whole input.
  test("documents ingest help flags that --example is only the metadata payload", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Eksempel:");
    expect(stdout).toContain("--metadata-payloaden");
    expect(stdout).toMatch(/ikke et komplet kald/i);
  });

  test("mail-intake ingest and imap-intake poll help flag the metadata-only example", async () => {
    for (const [cmd, sub] of [["mail-intake", "ingest"], ["imap-intake", "poll"]]) {
      const proc = Bun.spawn(["bun", "run", "src/cli.ts", cmd, sub, "--help"], {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
      });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      expect(stdout).toMatch(/ikke et komplet kald/i);
    }
  });

  // #262: vat momsangivelse / vat filing require a closed vat_quarter period;
  // the help must name the error form and the period close --kind vat_quarter
  // recovery path.
  test("vat momsangivelse help documents the closed-period precondition and fix", async () => {
    for (const sub of ["momsangivelse", "filing"]) {
      const proc = Bun.spawn(["bun", "run", "src/cli.ts", "vat", sub, "--help"], {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
      });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      expect(stdout).toContain("Inputnoter:");
      expect(stdout).toMatch(/is not closed/);
      expect(stdout).toContain("period close");
      expect(stdout).toContain("--kind vat_quarter");
    }
  });

  // #231: the top-level usage must group read/write commands and list --actor.
  test("global usage groups commands and lists the actor flags", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "--help"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("Læsekommandoer");
    expect(stdout).toContain("Skrivekommandoer");
    expect(stdout).toContain("--actor");
    expect(stdout).toContain("--actor-via");
  });
});
