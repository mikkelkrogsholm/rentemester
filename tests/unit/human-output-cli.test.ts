// Tests: src/cli-format.ts, src/cli/vat.ts, src/cli/invoice.ts (#211 human output)
//
// The read/report commands render Danish kroner-og-øre text under
// `--format human`, while `--format json` stays byte-stable for agents.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { formatKroner } from "../../src/cli-format";

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

describe("formatKroner", () => {
  test("renders DKK amounts as Danish kroner-og-øre", () => {
    expect(formatKroner(38.3)).toBe("38,30 kr.");
    expect(formatKroner(0)).toBe("0,00 kr.");
    expect(formatKroner(1234.5)).toBe("1.234,50 kr.");
    expect(formatKroner(-250)).toBe("-250,00 kr.");
    expect(formatKroner(1234567.89)).toBe("1.234.567,89 kr.");
  });

  test("renders missing or non-finite input as an em dash", () => {
    expect(formatKroner(null)).toBe("—");
    expect(formatKroner(undefined)).toBe("—");
    expect(formatKroner(Number.NaN)).toBe("—");
  });
});

describe("vat report human output (#211)", () => {
  test("renders the VAT report in Danish kroner-og-øre", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vat-human-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const human = await runCli([
      "vat", "report", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "vat", "report", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // Danish prose, no raw JSON field names.
    expect(human.stdout).toContain("Momsrapport for perioden 2026-05-01 til 2026-05-31");
    expect(human.stdout).not.toContain("netVatPayable");
    expect(human.stdout).not.toContain("outputVat");
    expect(human.stdout).not.toContain("{");
    // This period has 250,00 kr. input VAT and no output VAT, so the company
    // has money to its credit.
    expect(human.stdout).toContain("Du har 250,00 kr. til gode i moms for perioden.");
    expect(human.stdout).toContain("Købsmoms (indgående moms):");
    expect(human.stdout).toContain("250,00 kr.");

    // The json path stays byte-stable: exactly the JSON payload.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.inputVat).toBe(250);
    expect(parsed.netVatPayable).toBe(-250);
  });
});

describe("invoice status human output (#211)", () => {
  test("renders the invoice status in Danish kroner-og-øre", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-human-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();

    const human = await runCli([
      "invoice", "status", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "invoice", "status", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    expect(human.stdout).toContain("Status for faktura 2026-0001");
    expect(human.stdout).not.toContain("openBalance");
    expect(human.stdout).not.toContain("isOverdue");
    expect(human.stdout).not.toContain("{");
    expect(human.stdout).toContain("Fakturaen er forfalden — 5 dage over forfaldsdato.");
    expect(human.stdout).toContain("Åben saldo:");
    expect(human.stdout).toMatch(/\d+,\d{2} kr\./);

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.effectiveDueDate).toBe("2026-06-15");
    expect(parsed.isOverdue).toBe(true);
    expect(parsed.overdueDays).toBe(5);
  });
});

describe("profit-loss human output (#225)", () => {
  test("the period-result line ends with a single period, not 'kr..'", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-pl-human-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const human = await runCli([
      "report", "profit-loss", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stdout).toContain("Periodens resultat:");
    // The double-period bug: "... kr.." must never appear.
    expect(human.stdout).not.toContain("kr..");
    // The result line is "... <amount> kr." with exactly one trailing period.
    expect(human.stdout).toMatch(/Periodens resultat: (overskud|underskud) på [\d.]+,\d{2} kr\.(\n|$)/);
  });
});

describe("vat momsangivelse human output (#235, #236)", () => {
  test("renders the SKAT rubrikker and filing deadline in Danish", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-momsangivelse-human-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --status closed`.quiet();

    const human = await runCli([
      "vat", "momsangivelse", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "vat", "filing", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // Real readable Danish output — not a blank screen.
    expect(human.stdout).toContain("Momsangivelse for perioden 2026-05-01 til 2026-05-31");
    expect(human.stdout).toContain("Rubrikker til TastSelv Erhverv:");
    expect(human.stdout).toContain("Salgsmoms:");
    expect(human.stdout).toContain("Købsmoms:");
    expect(human.stdout).toContain("Momstilsvar:");
    expect(human.stdout).toContain("250,00 kr.");
    // No raw JSON field names / structure leak through. ("momstilsvar" is a
    // real Danish word and is allowed in prose; the camelCase JSON keys and
    // braces are what must not appear.)
    expect(human.stdout).not.toContain("momsAfVarekobUdland");
    expect(human.stdout).not.toContain("periodStatus");
    expect(human.stdout).not.toContain("{");
    // The SKAT filing/payment deadline — 1st of the third month after the
    // period ends (May → August). (#236)
    expect(human.stdout).toContain("SKAT-frist for indberetning og betaling: 2026-08-01");

    // The json path stays byte-stable and now carries filingDeadline.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.rubrikker.momstilsvar).toBe(-250);
    expect(parsed.filingDeadline).toBe("2026-08-01");
  });
});

describe("vat report shows the SKAT deadline (#236)", () => {
  test("a quarterly VAT report carries the filing/payment deadline", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vat-deadline-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const human = await runCli([
      "vat", "report", "--company", company,
      "--from", "2026-04-01", "--to", "2026-06-30",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    // Q2 ends 30-06 → deadline 1 September, not the period-end date.
    expect(human.stdout).toContain("SKAT-frist for indberetning og betaling: 2026-09-01");
  });
});

describe("report annual human output (#235)", () => {
  test("renders the income statement, balance and management statement", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-annual-human-"));
    const company = join(root, "company");
    const journalDir = mkdtempSync(join(tmpdir(), "rentemester-annual-human-j-"));

    await Bun.$`bun run src/cli.ts init --company ${company} --cvr DK12345678`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

    const open = join(journalDir, "open.json");
    const sale = join(journalDir, "sale.json");
    writeFileSync(open, JSON.stringify({
      transactionDate: "2025-01-02",
      text: "Indskud",
      lines: [
        { accountNo: "2000", debitAmount: 50000 },
        { accountNo: "5000", creditAmount: 50000 },
      ],
    }));
    writeFileSync(sale, JSON.stringify({
      transactionDate: "2025-06-15",
      text: "Konsulentsalg",
      documentId: 1,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    }));
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input ${open}`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input ${sale}`.quiet();
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2025-01-01 --to 2025-12-31 --kind fiscal_year`.quiet();

    const human = await runCli([
      "report", "annual", "--company", company,
      "--from", "2025-01-01", "--to", "2025-12-31",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "report", "annual", "--company", company,
      "--from", "2025-01-01", "--to", "2025-12-31",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });
    rmSync(journalDir, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // The owner sees real figures, not a blank "✔" screen.
    expect(human.stdout).toContain("Årsrapport (regnskabsklasse B) for regnskabsåret 2025-01-01 til 2025-12-31");
    expect(human.stdout).toContain("Resultatopgørelse");
    expect(human.stdout).toContain("Årets resultat: overskud på 1.000,00 kr.");
    expect(human.stdout).toContain("Balance pr. 2025-12-31");
    expect(human.stdout).toContain("Ledelsespåtegning:");
    expect(human.stdout).not.toContain("aretsResultat");
    expect(human.stdout).not.toContain("{");

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.aretsResultat).toBe(1000);
  });
});

describe("system backup-status human output (#240)", () => {
  test("states the backup duty is met and when the next is due", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-backup-status-human-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:09:00.000Z`.quiet();

    const human = await runCli([
      "system", "backup-status", "--company", company,
      "--as-of", "2026-05-17T02:10:00.000Z",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    expect(human.stdout).toContain("Backup-status pr.");
    // Plainly states whether the duty is met — not just "Latest Backup Id".
    expect(human.stdout).toContain("Backup-pligten er opfyldt");
    expect(human.stdout).toContain("ugentlig");
    expect(human.stdout).not.toContain("Latest Backup Id");
    expect(human.stdout).not.toContain("{");
  });

  test("states the duty is NOT met when no backup has been taken", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-backup-status-due-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();

    const human = await runCli([
      "system", "backup-status", "--company", company,
      "--as-of", "2026-05-17T02:10:00.000Z",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    // backupDue makes ok:false, so exit 1 — but the output is still a useful
    // status, not the generic "fejlede uden en specifik fejlbesked" template.
    expect(human.exitCode).toBe(1);
    const out = human.stdout + human.stderr;
    expect(out).toContain("Backup-pligten er IKKE opfyldt");
    expect(out).toContain("Tag en backup nu");
    expect(out).not.toContain("fejlede uden en specifik fejlbesked");
  });
});

describe("invoice interest human output (#250)", () => {
  test("renders the morarente figures in Danish, not a blank description", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-interest-human-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment",
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const human = await runCli([
      "invoice", "interest", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20", "--reference-rate", "2.2",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "invoice", "interest", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20", "--reference-rate", "2.2",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // Real readable Danish output — the namesake interest feature must not be blank.
    expect(human.stdout).toContain("Morarente for faktura 2026-0001");
    // The actual figures: overdue window, statutory rate and computed amount.
    expect(human.stdout).toContain("Referencesats (Nationalbanken): 2,2 %");
    expect(human.stdout).toContain("Morarentesats (reference + 8 %): 10,2 %");
    expect(human.stdout).toContain("Påløbet morarente:");
    expect(human.stdout).toContain("0,35 kr.");
    expect(human.stdout).toContain("Antal forfaldne dage:");
    // No raw JSON field names / structure leaks through.
    expect(human.stdout).not.toContain("accruedInterestAmount");
    expect(human.stdout).not.toContain("annualInterestRatePercent");
    expect(human.stdout).not.toContain("{");

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.annualInterestRatePercent).toBe(10.2);
    expect(parsed.accruedInterestAmount).toBe(0.35);
  });

  test("a second claim renders the incremental breakdown (days since last claim, already-claimed, cumulative)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-interest-human-incr-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment",
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();
    // First claim covers the first 5 overdue days (→ 0,35 kr).
    await Bun.$`bun run src/cli.ts invoice claim-interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();

    const human = await runCli([
      "invoice", "interest", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-07-20", "--reference-rate", "2.2",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // The headline pairs the incremental amount with the incremental days, not
    // the full overdue window, and the breakdown shows already-claimed + total.
    expect(human.stdout).toContain("Ny morarente for de 30 dage siden sidste krav");
    expect(human.stdout).toContain("Dage i dette krav (siden 2026-06-20): 30");
    expect(human.stdout).toContain("Allerede registreret morarente: 0,35 kr.");
    expect(human.stdout).toContain("Morarente i alt til dato:");
    expect(human.stdout).toContain("2,45 kr.");
  });
});

describe("invoice compensation human output (#250)", () => {
  test("renders the compensation assessment in Danish with a clear reason", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-compensation-human-"));
    const company = join(root, "company");
    const paymentInput = join(root, "partial-payment.json");

    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment",
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const human = await runCli([
      "invoice", "compensation", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20",
      "--format", "human",
    ]);
    const jsonRun = await runCli([
      "invoice", "compensation", "--company", company,
      "--invoice-number", "2026-0001", "--as-of", "2026-06-20",
      "--format", "json",
    ]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    expect(human.stdout).toContain("Kompensation for sen betaling — faktura 2026-0001");
    // The actual verdict and figures, not a blank description.
    expect(human.stdout).toContain("Berettiget:");
    expect(human.stdout).toContain("Kompensationsbeløb:");
    expect(human.stdout).toContain("Antal forfaldne dage:");
    expect(human.stdout).not.toContain("compensationAmountDkk");
    expect(human.stdout).not.toContain("isCommercialTransaction");
    expect(human.stdout).not.toContain("{");

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(typeof parsed.eligible).toBe("boolean");
    expect(typeof parsed.reason).toBe("string");
  });
});

describe("invoice validate human output (#250)", () => {
  test("renders a valid verdict in Danish", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-validate-human-ok-"));
    const file = join(dir, "invoice.json");
    writeFileSync(file, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0100",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring" }],
      totals: { netAmount: 400, vatRate: 0.25, vatAmount: 100, grossAmount: 500 },
      currency: "DKK",
    }, null, 2));

    const human = await runCli(["invoice", "validate", "--input", file, "--format", "human"]);
    const jsonRun = await runCli(["invoice", "validate", "--input", file, "--format", "json"]);

    rmSync(dir, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    expect(human.stdout).toContain("Fakturavalidering");
    expect(human.stdout).toContain("Fakturaen er gyldig og kan udstedes.");
    expect(human.stdout).toContain("Fakturatype:");
    expect(human.stdout).toContain("Momsbehandling:");
    expect(human.stdout).not.toContain("appliedRules");
    expect(human.stdout).not.toContain("{");

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
  });

  test("renders an invalid verdict with every concrete rejection reason", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-validate-human-bad-"));
    const file = join(dir, "invoice.json");
    // Missing seller details and gross amount — several rejection reasons.
    writeFileSync(file, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      seller: {},
      buyer: {},
      lines: [{ description: "Bogføring" }],
      totals: {},
      currency: "DKK",
    }, null, 2));

    const human = await runCli(["invoice", "validate", "--input", file, "--format", "human"]);
    const jsonRun = await runCli(["invoice", "validate", "--input", file, "--format", "json"]);

    rmSync(dir, { recursive: true, force: true });

    // An invalid payload is a business rejection — exit 1 — but the output is a
    // useful verdict, not the generic "fejlede uden en specifik fejlbesked".
    expect(human.exitCode).toBe(1);
    const out = human.stdout + human.stderr;
    expect(out).toContain("Fakturaen er IKKE gyldig");
    expect(out).toContain("seller.name is required");
    expect(out).toContain("totals.grossAmount is required");
    expect(out).not.toContain("fejlede uden en specifik fejlbesked");
    expect(out).not.toContain("{");

    // The json path stays byte-stable and still carries the raw errors array.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(false);
    expect(Array.isArray(parsed.errors)).toBe(true);
  });
});

describe("accounts list human output (#246)", () => {
  test("shows account numbers and no raw array-index column", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-accounts-list-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const human = await runCli(["accounts", "list", "--company", company, "--format", "human"]);

    rmSync(root, { recursive: true, force: true });

    expect(human.exitCode).toBe(0);
    expect(human.stdout).toContain("Kontoplan");
    expect(human.stdout).toContain("Kontonr.");
    expect(human.stdout).toContain("1200");
    // console.table's "(index)" column with 0,1,2… is gone.
    expect(human.stdout).not.toContain("(index)");
    const headerLine = human.stdout.split("\n").find((l) => l.includes("Kontonr.")) ?? "";
    // The first column is the account number — the header must not begin with
    // a numeric index column.
    expect(headerLine.trimStart().startsWith("Kontonr.")).toBe(true);
  });
});

describe("invoice create human output (#266, #268)", () => {
  async function createInvoice(format: "human" | "json") {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-out-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const out = await runCli([
      "invoice", "create", "--company", company,
      "--issue-date", "2026-05-21",
      "--line", "Bogføring og momsafstemning|2|800;Rådgivning|1|500",
      "--vat-rate", "25",
      "--buyer-name", "Kunde A/S",
      "--buyer-address", "Købervej 9, 8000 Aarhus C",
      "--seller-name", "Rentemester ApS",
      "--seller-address", "Testvej 1, 2100 København Ø",
      "--seller-vat", "DK12345678",
      "--format", format,
    ]);
    rmSync(root, { recursive: true, force: true });
    return out;
  }

  async function createInvoiceHuman() {
    // Each `invoice create` consumes an invoice number, so the human and JSON
    // runs must use SEPARATE fresh companies to both get 2026-0001.
    const [human, jsonRun] = await Promise.all([
      createInvoice("human"),
      createInvoice("json"),
    ]);
    return { human, jsonRun };
  }

  test("#268 — renders a short Danish heading, Danish labels and the figures", async () => {
    const { human, jsonRun } = await createInvoiceHuman();

    expect(human.exitCode).toBe(0);
    expect(human.stderr).toBe("");
    // A short Danish heading — NOT the long command description.
    expect(human.stdout).toContain("Faktura 2026-0001 er udstedt");
    expect(human.stdout).not.toContain(
      "Udsteder en kundefaktura uden at du selv skriver JSON",
    );
    // Danish field labels — never the English humanized keys.
    expect(human.stdout).not.toContain("Invoice Number");
    expect(human.stdout).not.toContain("Document Id");
    expect(human.stdout).toContain("Fakturanummer:");
    // The figures that matter: net, VAT, gross.
    expect(human.stdout).toContain("Nettobeløb (ekskl. moms):");
    expect(human.stdout).toContain("Momsbeløb:");
    expect(human.stdout).toContain("Fakturabeløb (inkl. moms):");
    expect(human.stdout).toContain("2.100,00 kr.");
    expect(human.stdout).toContain("525,00 kr.");
    expect(human.stdout).toContain("2.625,00 kr.");
    // No raw JSON structure leaks through.
    expect(human.stdout).not.toContain("{");
    expect(human.stdout).not.toContain("grossAmount");

    // The json path stays byte-stable.
    const parsed = JSON.parse(jsonRun.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.invoiceNumber).toBe("2026-0001");
    expect(parsed.computed.netAmount).toBe(2100);
    expect(parsed.computed.vatAmount).toBe(525);
    expect(parsed.computed.grossAmount).toBe(2625);
  });

  test("#266 — states the invoice is NOT yet posted and that `invoice post` is required", async () => {
    const { human } = await createInvoiceHuman();

    expect(human.exitCode).toBe(0);
    // The gap must be impossible to miss: issued but not booked.
    expect(human.stdout).toContain("IKKE bogført");
    expect(human.stdout).toContain("invoice post 2026-0001");
  });
});

describe("momsangivelse prerequisite guidance (#227)", () => {
  test("an un-closed VAT period failure guides the owner to 'period close'", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-momsangivelse-guide-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const human = await runCli([
      "vat", "momsangivelse", "--company", company,
      "--from", "2026-04-01", "--to", "2026-06-30",
      "--format", "human",
    ]);

    rmSync(root, { recursive: true, force: true });

    // A momsangivelse on an open period fails as a business error.
    expect(human.exitCode).toBe(1);
    // The terse core message is still there, plus actionable guidance.
    expect(human.stderr).toContain("Sådan kommer du videre:");
    expect(human.stderr).toContain("rentemester period close");
    expect(human.stderr).toContain("--kind vat_quarter");
  });
});
