// Tests: src/cli/invoice.ts, src/cli.ts (invoice interest CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("invoice interest CLI", () => {
  test("posts a registered overdue late-interest claim to the ledger", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-post-cli-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice claim-interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "post-interest", "--company", company, "--invoice-number", "2026-0001"], {
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
    expect(parsed.entryId).toBeDefined();
    expect(parsed.accruedInterestAmount).toBe(0.35);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-BOOKKEEPING-001");
  });

  test("registers overdue late interest for an invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-register-cli-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "claim-interest", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-20", "--reference-rate", "2.2"], {
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
    expect(parsed.claimId).toBeDefined();
    expect(parsed.accruedInterestAmount).toBe(0.35);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-REGISTER-001");
  });

  test("calculates overdue late interest for an invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-cli-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "interest", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-20", "--reference-rate", "2.2"], {
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
    expect(parsed.annualInterestRatePercent).toBe(10.2);
    expect(parsed.accruedInterestAmount).toBe(0.35);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-001");
  });

  test("proposes and books a late-interest correction through the CLI after a back-dated payment", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-correction-cli-"));
    const company = join(root, "company");
    const firstPayment = join(root, "p1.json");
    const backdatedPayment = join(root, "p2.json");

    writeFileSync(firstPayment, JSON.stringify({ invoiceDocumentId: 1, paymentDate: "2026-05-20", amount: 1000, note: "Delbetaling" }, null, 2));
    // A SECOND payment recorded with a back-dated effective date (after due, before the claim).
    writeFileSync(backdatedPayment, JSON.stringify({ invoiceDocumentId: 1, paymentDate: "2026-06-16", amount: 200, note: "Bagud-dateret afdrag" }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${firstPayment}`.quiet();
    await Bun.$`bun run src/cli.ts invoice claim-interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();
    await Bun.$`bun run src/cli.ts invoice post-interest --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${backdatedPayment}`.quiet();

    const proposeProc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "interest-correction", "--company", company, "--invoice-number", "2026-0001"], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const proposeOut = await new Response(proposeProc.stdout).text();
    expect(await proposeProc.exited).toBe(0);
    const proposal = JSON.parse(proposeOut);
    expect(proposal.ok).toBe(true);
    expect(proposal.hasProposal).toBe(true);
    expect(proposal.overClaimedAmount).toBeGreaterThan(0);

    const postProc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "post-interest-correction", "--company", company, "--invoice-number", "2026-0001", "--reason", "Bagud-dateret betaling"], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const postOut = await new Response(postProc.stdout).text();
    const postExit = await postProc.exited;

    rmSync(root, { recursive: true, force: true });
    expect(postExit).toBe(0);
    const correction = JSON.parse(postOut);
    expect(correction.ok).toBe(true);
    expect(correction.correctedAmount).toBe(proposal.overClaimedAmount);
    expect(correction.entryId).toBeDefined();
  });

  test("a second `invoice interest` calculation carries the incremental fields through the CLI JSON envelope", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-incr-cli-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();
    // First claim covers the first 5 overdue days.
    await Bun.$`bun run src/cli.ts invoice claim-interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "interest", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-07-20", "--reference-rate", "2.2"], {
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
    // The incremental contract is carried end-to-end through the CLI JSON: the
    // amount is the 30-day increment since the last claim, not the full window.
    expect(parsed.accruedInterestAmount).toBe(2.10);
    expect(parsed.claimableDays).toBe(30);
    expect(parsed.interestFromDate).toBe("2026-06-20");
    expect(parsed.priorClaimedInterest).toBe(0.35);
    expect(parsed.totalInterestToDate).toBe(2.45);
  });
});
