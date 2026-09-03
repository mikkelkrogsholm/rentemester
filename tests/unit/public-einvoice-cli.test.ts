// Tests: src/cli/invoice.ts, src/cli.ts (public e-invoice CLI)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { Database } from "bun:sqlite";
import { exportPublicEInvoiceOioUbl } from "../../src/core/public-einvoice";

describe("public e-invoice CLI", () => {
  test("exports a deterministic public-recipient preview artifact", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-einvoice-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "public-invoice.json");
    const outPath = join(root, "public-preview.xml");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = Bun.spawn([
      "bun", "run", "src/cli.ts", "customer", "create", "--company", company,
      "--name", "Aarhus Kommune", "--address", "Rådhuspladsen 2, 8000 Aarhus C", "--ean", "5790000000001"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const createdStdout = await new Response(created.stdout).text();
    const createdExitCode = await created.exited;
    expect(createdExitCode).toBe(0);
    const customerId = JSON.parse(createdStdout).customerId;

    const issue = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", invoiceInput, "--customer-id", String(customerId)
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const issueStdout = await new Response(issue.stdout).text();
    const issueExitCode = await issue.exited;
    expect(issueExitCode).toBe(0);
    const issued = JSON.parse(issueStdout);

    const exportRun = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "export-public", "--company", company, "--invoice-number", issued.invoiceNumber, "--out", outPath
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const exportStdout = await new Response(exportRun.stdout).text();
    const exportStderr = await new Response(exportRun.stderr).text();
    const exportExitCode = await exportRun.exited;
    const firstXml = readFileSync(outPath, "utf8");

    const rerun = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "export-public", "--company", company, "--invoice-number", issued.invoiceNumber, "--out", outPath
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const rerunStdout = await new Response(rerun.stdout).text();
    const rerunExitCode = await rerun.exited;
    const secondXml = readFileSync(outPath, "utf8");

    expect({ exportExitCode, exportStderr }).toEqual({ exportExitCode: 0, exportStderr: "" });
    expect(existsSync(outPath)).toBe(true);
    expect(JSON.parse(exportStdout).ok).toBe(true);
    expect(JSON.parse(exportStdout).sha256).toBe(JSON.parse(rerunStdout).sha256);
    expect(rerunExitCode).toBe(0);
    expect(firstXml).toBe(secondXml);
    expect(firstXml).toContain("<EanNumber>5790000000001</EanNumber>");
    rmSync(root, { recursive: true, force: true });
  });

  test("exports a deterministic public-recipient OIOUBL handoff artifact", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-oioubl-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "public-invoice.json");
    const outPath = join(root, "public-oioubl.xml");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      dueDate: "2026-06-19",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = Bun.spawn([
      "bun", "run", "src/cli.ts", "customer", "create", "--company", company,
      "--name", "Aarhus Kommune", "--address", "Rådhuspladsen 2, 8000 Aarhus C", "--ean", "5790000000001"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const createdStdout = await new Response(created.stdout).text();
    const createdExitCode = await created.exited;
    expect(createdExitCode).toBe(0);
    const customerId = JSON.parse(createdStdout).customerId;

    const issue = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", invoiceInput, "--customer-id", String(customerId)
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const issueStdout = await new Response(issue.stdout).text();
    const issueExitCode = await issue.exited;
    expect(issueExitCode).toBe(0);
    const issued = JSON.parse(issueStdout);

    const exportRun = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "export-public-oioubl", "--company", company, "--invoice-number", issued.invoiceNumber, "--out", outPath
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const exportStdout = await new Response(exportRun.stdout).text();
    const exportStderr = await new Response(exportRun.stderr).text();
    const exportExitCode = await exportRun.exited;
    const firstXml = readFileSync(outPath, "utf8");

    const rerun = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "export-public-oioubl", "--company", company, "--invoice-number", issued.invoiceNumber, "--out", outPath
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const rerunStdout = await new Response(rerun.stdout).text();
    const rerunExitCode = await rerun.exited;
    const secondXml = readFileSync(outPath, "utf8");

    expect({ exportExitCode, exportStderr }).toEqual({ exportExitCode: 0, exportStderr: "" });
    expect(existsSync(outPath)).toBe(true);
    expect(JSON.parse(exportStdout).ok).toBe(true);
    expect(JSON.parse(exportStdout).sha256).toBe(JSON.parse(rerunStdout).sha256);
    expect(rerunExitCode).toBe(0);
    expect(firstXml).toBe(secondXml);
    expect(firstXml).toContain(
      "<cbc:CustomizationID>OIOUBL-2.02</cbc:CustomizationID>",
    );
    expect(firstXml).toContain('<cbc:EndpointID schemeID="GLN">5790000000001</cbc:EndpointID>');
    expect(firstXml).toContain('<cbc:TaxAmount currencyID="DKK">250.00</cbc:TaxAmount>');
    expect(firstXml).toContain('<cbc:TaxExclusiveAmount currencyID="DKK">250.00</cbc:TaxExclusiveAmount>');
    expect(firstXml).toContain('<cbc:TaxInclusiveAmount currencyID="DKK">1250.00</cbc:TaxInclusiveAmount>');
    rmSync(root, { recursive: true, force: true });
  });

  test("refuses a legacy OIOUBL payload with contradictory tax totals", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-oioubl-tax-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "public-invoice.json");
    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full", vatTreatment: "standard", issueDate: "2026-05-20", dueDate: "2026-06-19",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 }, currency: "DKK",
    }));
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = Bun.spawn(["bun", "run", "src/cli.ts", "customer", "create", "--company", company, "--name", "Aarhus Kommune", "--address", "Rådhuspladsen 2, 8000 Aarhus C", "--ean", "5790000000001"], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const customerId = JSON.parse(await new Response(created.stdout).text()).customerId;
    expect(await created.exited).toBe(0);
    const issue = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", invoiceInput, "--customer-id", String(customerId)], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const issued = JSON.parse(await new Response(issue.stdout).text());
    expect(await issue.exited).toBe(0);

    const db = new Database(join(company, "data", "ledger.sqlite"));
    const legacyPayload = JSON.parse((db.query("SELECT payload_json FROM documents WHERE id = ?").get(issued.documentId) as { payload_json: string }).payload_json);
    legacyPayload.totals = { ...legacyPayload.totals, vatAmount: 200, grossAmount: 1200 };
    // Simulate a row written before the append-only document guard existed.
    db.exec("DROP TRIGGER documents_no_update_issued_invoice");
    db.query("UPDATE documents SET payload_json = ? WHERE id = ?").run(JSON.stringify(legacyPayload), issued.documentId);
    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId });
    db.close();

    expect(exported.ok).toBe(false);
    expect(exported.errors).toContain(`invoice ${issued.invoiceNumber} totals.vatAmount must equal rounded OIOUBL line VAT amounts (250)`);
    expect(exported.errors).toContain(`invoice ${issued.invoiceNumber} totals.grossAmount must equal rounded OIOUBL line totals (1250)`);
    rmSync(root, { recursive: true, force: true });
  });

  test("submits a deterministic, idempotent PEPPOL submission envelope", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-submit-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "public-invoice.json");
    const apConfig = join(root, "access-point.json");
    const outPath = join(root, "peppol-submission.json");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      dueDate: "2026-06-19",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    }, null, 2));
    // Access-point config lives OUTSIDE core bookkeeping state — passed in as a file.
    writeFileSync(apConfig, JSON.stringify({
      accessPointId: "ap-nemhandel-test",
      endpointUrl: "https://access-point.example.dk/peppol",
      senderEndpointId: "0184:DK12345678"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = Bun.spawn([
      "bun", "run", "src/cli.ts", "customer", "create", "--company", company,
      "--name", "Aarhus Kommune", "--address", "Rådhuspladsen 2, 8000 Aarhus C", "--ean", "5790000000001"
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const createdStdout = await new Response(created.stdout).text();
    expect(await created.exited).toBe(0);
    const customerId = JSON.parse(createdStdout).customerId;

    const issue = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", invoiceInput, "--customer-id", String(customerId)
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const issueStdout = await new Response(issue.stdout).text();
    expect(await issue.exited).toBe(0);
    const issued = JSON.parse(issueStdout);

    const submit = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "submit-public-peppol", "--company", company,
      "--invoice-number", issued.invoiceNumber, "--access-point", apConfig, "--out", outPath
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const submitStdout = await new Response(submit.stdout).text();
    const submitStderr = await new Response(submit.stderr).text();
    const submitExit = await submit.exited;

    const resubmit = Bun.spawn([
      "bun", "run", "src/cli.ts", "invoice", "submit-public-peppol", "--company", company,
      "--invoice-number", issued.invoiceNumber, "--access-point", apConfig, "--out", outPath
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const resubmitStdout = await new Response(resubmit.stdout).text();
    const resubmitExit = await resubmit.exited;

    expect({ submitExit, submitStderr }).toEqual({ submitExit: 0, submitStderr: "" });
    expect(resubmitExit).toBe(0);
    const firstResult = JSON.parse(submitStdout);
    const secondResult = JSON.parse(resubmitStdout);
    expect(firstResult.ok).toBe(true);
    expect(firstResult.duplicate).toBe(false);
    expect(secondResult.ok).toBe(true);
    expect(secondResult.duplicate).toBe(true);
    expect(firstResult.idempotencyKey).toBe(secondResult.idempotencyKey);
    expect(existsSync(outPath)).toBe(true);
    expect(readFileSync(outPath, "utf8")).toContain("ap-nemhandel-test");
    rmSync(root, { recursive: true, force: true });
  });
});
