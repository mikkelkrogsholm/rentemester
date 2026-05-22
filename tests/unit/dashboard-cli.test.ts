// Tests: src/cli/dashboard.ts, src/cli.ts (dashboard CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, existsSync, statSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("dashboard CLI", () => {
  test("renders a dashboard HTML file from an initialized company", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-"));
    const company = join(root, "company");
    const outPath = join(root, "dashboard.html");

    const init = await runCli(["init", "--company", company, "--cvr", "12345678"]);
    expect({ exitCode: init.exitCode, stderr: init.stderr }).toEqual({ exitCode: 0, stderr: "" });

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", outPath,
      "--as-of", "2026-05-17",
    ]);
    expect({ exitCode: dash.exitCode, stderr: dash.stderr }).toEqual({ exitCode: 0, stderr: "" });

    expect(existsSync(outPath)).toBe(true);
    const size = statSync(outPath).size;
    expect(size).toBeGreaterThan(1024);

    const html = readFileSync(outPath, "utf8");
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html).toContain('<html lang="da">');
    expect(html).toContain("Rentemester company");
    expect(html).toContain("CVR DK12345678");
    expect(html).toContain('<header class="header">');
    expect(html).toContain('<section class="metrics">');
    expect(html).toContain("Næste deadline");
    expect(html).toContain("Åbne fakturaer");
    expect(html).toContain("Seneste aktivitet");
    expect(html).toContain("Backup-status");
    expect(html).toContain("Audit-chain");
    expect(html.trimEnd().endsWith("</html>")).toBe(true);

    rmSync(root, { recursive: true, force: true });
  });

  test("errors when --out is missing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-noout-"));
    const company = join(root, "company");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--as-of", "2026-05-17",
    ]);
    expect(dash.exitCode).toBe(2);
    expect(dash.stderr).toContain("--out");

    rmSync(root, { recursive: true, force: true });
  });

  test("errors on invalid --as-of format", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-asof-"));
    const company = join(root, "company");
    const out = join(root, "dashboard.html");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", out,
      "--as-of", "17-05-2026",
    ]);
    expect(dash.exitCode).toBe(2);
    expect(dash.stderr).toContain("YYYY-MM-DD");

    rmSync(root, { recursive: true, force: true });
  });

  // #281: with booked Q1 2026 output VAT still unreported, the dashboard's
  // "Næste deadline" box must surface Q1 2026 and its real payable amount —
  // not the empty current calendar quarter the as-of date falls in.
  test("next-VAT-deadline shows the earliest unreported quarter with activity", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-vat-"));
    const company = join(root, "company");
    const out = join(root, "dashboard.html");

    const init = await runCli(["init", "--company", company, "--cvr", "12345678"]);
    expect(init.exitCode).toBe(0);

    // A supporting document dated in Q1 2026.
    const metadata = join(root, "doc.metadata.json");
    const docFile = join(root, "doc.txt");
    writeFileSync(docFile, "Q1-salgsbilag\n");
    writeFileSync(
      metadata,
      JSON.stringify({
        source: "email",
        issueDate: "2026-02-15",
        invoiceNo: "Q1-0001",
        deliveryDescription: "Salg i Q1",
        amountIncVat: 27000,
        currency: "DKK",
        sender: { name: "Kunde ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
        recipient: { name: "Rentemester ApS", address: "Vej 2", vatOrCvr: "DK12345678" },
        vatAmount: 5400,
        paymentDetails: "Bankoverførsel",
      }),
    );
    const ingest = await runCli([
      "documents", "ingest",
      "--company", company,
      "--file", docFile,
      "--metadata", metadata,
    ]);
    expect({ exitCode: ingest.exitCode, stderr: ingest.stderr }).toEqual({ exitCode: 0, stderr: "" });

    // A Q1 2026 sale: 21.600 net + 5.400 output VAT (account 1200).
    const journal = join(root, "sale.json");
    writeFileSync(
      journal,
      JSON.stringify({
        transactionDate: "2026-02-15",
        text: "Salg Q1 2026",
        documentId: 1,
        lines: [
          { accountNo: "2000", debitAmount: 27000, text: "Bankindbetaling" },
          { accountNo: "1000", creditAmount: 21600, vatCode: "DK_SALE_25", text: "Omsætning" },
          { accountNo: "1200", creditAmount: 5400, text: "Udgående moms" },
        ],
      }),
    );
    const post = await runCli([
      "journal", "post",
      "--company", company,
      "--input", journal,
    ]);
    expect({ exitCode: post.exitCode, stderr: post.stderr }).toEqual({ exitCode: 0, stderr: "" });

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", out,
      "--as-of", "2026-05-22",
    ]);
    expect({ exitCode: dash.exitCode, stderr: dash.stderr }).toEqual({ exitCode: 0, stderr: "" });

    const html = readFileSync(out, "utf8");
    // The deadline box names Q1 2026, its real SKAT deadline and the real
    // 5.400 kr payable — never the empty Q2 with 0,00 kr. #314: amounts
    // use the canonical `formatKronerDa` (regular-space " kr." suffix).
    expect(html).toContain("Q1 2026");
    expect(html).toContain("2026-06-01");
    expect(html).toContain("5.400,00 kr.");
    expect(html).not.toContain("Q2 2026");
    // The "Næste deadline" card carries the real payable, not 0,00.
    const cardMatch = /<div class="deadline-card">[\s\S]*?<\/div>\s*<\/div>\s*<\/div>/.exec(html);
    expect(cardMatch).not.toBeNull();
    expect(cardMatch![0]).toMatch(/amount-lg">5\.400,00 kr\.</);

    rmSync(root, { recursive: true, force: true });
  });

  test("render wall-clock under 100ms for fresh company", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-timing-"));
    const company = join(root, "company");
    const out = join(root, "dashboard.html");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", out,
      "--as-of", "2026-05-17",
      "--format", "json",
    ]);
    expect(dash.exitCode).toBe(0);

    // CLI emits a JSON result via emitResult; extract renderMs from the JSON.
    // JSON output is wrapped in human-readable header; we grep for the value.
    const match = /"renderMs"\s*:\s*([0-9.]+)/.exec(dash.stdout);
    expect(match, `renderMs missing in CLI output:\n${dash.stdout}`).not.toBeNull();
    const renderMs = Number(match![1]);
    expect(renderMs).toBeLessThan(100);

    rmSync(root, { recursive: true, force: true });
  });
});
