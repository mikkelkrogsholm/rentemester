import type { Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { writeFileSync, readFileSync } from "node:fs";

export async function checkNativeSales(page: Page, base: string, reopened = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const today = new Date().toISOString().slice(0, 10);
  if (!reopened) {
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const a of [
      { no: "1100", name: "Synthetic customer receivables", type: "asset", normal: "debit" },
      { no: "1000", name: "Synthetic domestic sales", type: "income", normal: "credit" },
      { no: "4200", name: "Synthetic output VAT", type: "vat", normal: "credit" },
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(a.no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(a.name);
      await form.getByLabel(/^Kontotype/).selectOption(a.type);
      await form.getByLabel(/^Normal saldo/).selectOption(a.normal);
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("cell", { name: a.name, exact: true }).waitFor();
    }
    for (const [role, no] of [["debtors", "1100"], ["output_vat", "4200"]]) {
      const form = page.getByRole("form", { name: "Bekræft kontorolle", exact: true });
      await form.getByLabel(/^Kontorolle/).selectOption(role);
      await form.getByLabel(/^Konto til rollen/).selectOption(no);
      await form.getByRole("button", { name: "Bekræft kontorolle", exact: true }).click();
      await page.getByText(`Konto ${no} (version 1)`, { exact: true }).waitFor();
    }
    await page.goto(`${base}/companies/synthetic/kontakter`);
    await page.getByRole("button", { name: "Tilføj kunde", exact: true }).first().click();
    const contact = page.getByRole("dialog");
    await contact.getByLabel("Navn", { exact: true }).fill("Synthetic Customer");
    await contact.getByLabel("Adresse", { exact: true }).fill("Synthetic Buyer Street 7");
    await contact.getByLabel(/^CVR \/ moms-nr\./).fill("DK12345678");
    await contact.getByRole("button", { name: "Opret", exact: true }).click();
    await contact.waitFor({ state: "hidden" });
    await page.getByText("Synthetic Customer", { exact: true }).waitFor();
    const contacts = await (await page.request.get(`${url}/contacts`)).json();
    const customer = contacts.contacts.customers.find((c: { name: string }) => c.name === "Synthetic Customer");
    if (!customer || customer.openBalance !== 0) throw new Error("Created customer missing or has invented balance");
    await page.goto(`${base}/companies/synthetic/fakturaer`);
    await page.getByRole("link", { name: "Udsted faktura", exact: true }).first().click();
    await page.getByLabel("Fakturadato", { exact: true }).fill(today);
    await page.getByLabel("Sælger CVR/moms", { exact: true }).fill("DK87654321");
    await page.getByLabel(/^Vælg kunde/).selectOption(String(customer.id));
    await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Synthetic consulting service");
    await page.getByLabel("Linje 1 antal", { exact: true }).fill("2");
    await page.getByLabel("Linje 1 enhedspris", { exact: true }).fill("500,00");
    const [preview] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/invoices/preview") && r.request().method() === "POST"), page.getByRole("button", { name: "Forhåndsvis", exact: true }).click()]);
    if (!preview.ok() || preview.headers()["content-type"] !== "application/pdf") throw new Error(`Preview failed: HTTP ${preview.status()}`);
    const previewLink = page.getByRole("link", { name: "Åbn forhåndsvisning", exact: true });
    await previewLink.waitFor();
    const href = await previewLink.getAttribute("href");
    if (!href?.startsWith("blob:")) throw new Error("Browser did not receive a preview blob");
    // Chromium's PDF popup can hide the network body from Playwright; inspect
    // the actual blob the application received and offers to the user.
    const previewBytes = Buffer.from(await page.evaluate(async href => Array.from(new Uint8Array(await (await fetch(href)).arrayBuffer())), href));
    if (!previewBytes.subarray(0, 5).equals(Buffer.from("%PDF-")) || previewBytes.length !== Number(preview.headers()["content-length"])) throw new Error("Browser preview PDF is incomplete");
    const before = await (await page.request.get(`${url}/invoices`)).json();
    if (before.invoices.invoices.length !== 0) throw new Error("Preview issued an invoice");
    const [issued] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/invoices/issue") && r.request().method() === "POST"), page.getByRole("button", { name: "Udsted faktura", exact: true }).click()]);
    const result = await issued.json();
    if (!issued.ok() || result.invoice.grossAmount !== 1250 || result.invoice.netAmount !== 1000 || result.invoice.vatAmount !== 250 || !result.invoice.invoiceNumber.endsWith("-0001")) throw new Error(`Incorrect issued invoice: ${JSON.stringify(result)}`);
    await page.getByText(/udstedt\./).waitFor();
    await page.goto(`${base}/companies/synthetic/fakturaer`);
    const row = page.getByRole("row").filter({ hasText: result.invoice.invoiceNumber });
    await row.getByText("Udstedt – afventer bogføring", { exact: true }).waitFor();
    const duplicate = await page.request.post(`${url}/invoices/issue`, { headers, data: { issueDate: today, customerId: customer.id, invoiceNumber: result.invoice.invoiceNumber, seller: { vatOrCvr: "DK87654321" }, lines: [{ description: "Synthetic duplicate", quantity: 1, unitPriceExVat: 100 }] } });
    if (duplicate.status() !== 422) throw new Error("Invoice number reused");
    await row.getByRole("button", { name: "Bogfør faktura", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel(/^Omsætningskonto/).selectOption("1000");
    await dialog.getByRole("button", { name: "Bogfør salget", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
    await row.getByText("Bogført", { exact: true }).waitFor();
    const rebook = await page.request.post(`${url}/invoices/post`, { headers, data: { confirm: true, invoiceDocumentId: result.invoice.documentId, revenueAccountNo: "1000" } });
    if (rebook.status() !== 422) throw new Error("Invoice posted twice");
    console.log("Browser: chart + roles → customer → nonmutating invoice preview → immutable issued invoice → explicit sales posting");
  }
  await page.goto(`${base}/companies/synthetic/fakturaer`);
  await page.getByRole("heading", { name: "Fakturaer", exact: true }).waitFor();
  const invoices = await (await page.request.get(`${url}/invoices`)).json();
  const rows = invoices.invoices.invoices;
  if (rows.length !== 1 || rows[0].grossAmount !== 1250 || rows[0].openBalance !== 1250 || rows[0].posted !== true) throw new Error(`Incorrect sales invoice state: ${JSON.stringify(invoices)}`);
  const pdf = await page.request.get(`${url}/invoices/${rows[0].documentId}/pdf`);
  if (!pdf.ok()) throw new Error("Issued invoice PDF missing");
  const path = "/tmp/rentemester-rust-reports/native-sales-invoice.pdf";
  const bytes = await pdf.body();
  if (reopened && !readFileSync(path).equals(bytes)) throw new Error("Issued invoice PDF changed after restart");
  writeFileSync(path, bytes);
  const text = execFileSync("pdftotext", ["-layout", path, "-"], { encoding: "utf8" });
  for (const term of [rows[0].invoiceNo, "Synthetic Customer", "Synthetic Buyer Street 7", "DK87654321", "Synthetic consulting service", "1000,00", "250,00", "1250,00"]) if (!text.includes(term)) throw new Error(`Invoice PDF missing ${term}: ${text}`);
  const report = await (await page.request.get(`${url}/income-statement`)).json();
  if (report.incomeStatement.totalIncome !== 1000 || report.incomeStatement.result !== -842.25) throw new Error("Sales not reflected in profit");
  const vat = await (await page.request.get(`${url}/vat`)).json();
  if (vat.vat.outputVat !== 250 || vat.vat.inputVat !== 25 || vat.vat.payable !== 225 || vat.vat.vatReportErrors.length) throw new Error(`Sales VAT not reconciled: ${JSON.stringify(vat)}`);
  const contacts = await (await page.request.get(`${url}/contacts`)).json();
  const customer = contacts.contacts.customers.find((c: { name: string }) => c.name === "Synthetic Customer");
  if (!customer || customer.openBalance !== 1250 || customer.openInvoiceCount !== 1) throw new Error("Customer receivable not retained");
  await page.goto(`${base}/companies/synthetic/resultatopgorelse`);
  await page.getByText("Synthetic domestic sales", { exact: true }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-sales.png", fullPage: true });
  console.log(reopened ? "PASS: customer, issued invoice, exact original PDF, one sales posting, receivable, revenue and VAT persist after restart" : "PASS: real browser customer → invoice + PDF → sales posting → correct receivable, revenue, VAT and profit; duplicate numbering and posting refused");
}
