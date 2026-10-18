import type { Page } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
const evidencePath = "/tmp/rentemester-rust-reports/native-credit-evidence.json";

export async function checkNativeCredit(page: Page, base: string, reopened = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  let evidence: { invoiceId: number; number: string; pdf: string; credits: Array<{ id: number; bytes: string; gross: number; vat: number }> };
  if (!reopened) {
    const today = new Date().toISOString().slice(0, 10);
    const contacts = await (await page.request.get(`${url}/contacts`)).json();
    const customer = contacts.contacts.customers.find((c: { name: string }) => c.name === "Synthetic Customer");
    await page.goto(`${base}/companies/synthetic/fakturaer`);
    await page.getByRole("link", { name: "Udsted faktura", exact: true }).first().click();
    await page.getByLabel("Fakturadato", { exact: true }).fill(today);
    await page.getByLabel("Sælger CVR/moms", { exact: true }).fill("DK87654321");
    await page.getByLabel(/^Vælg kunde/).selectOption(String(customer.id));
    await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Synthetic cancelled service");
    await page.getByLabel("Linje 1 antal", { exact: true }).fill("1");
    await page.getByLabel("Linje 1 enhedspris", { exact: true }).fill("500,00");
    const [issued] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/invoices/issue") && r.request().method() === "POST"), page.getByRole("button", { name: "Udsted faktura", exact: true }).click()]);
    const result = await issued.json();
    if (!issued.ok() || result.invoice.grossAmount !== 625 || !result.invoice.invoiceNumber.endsWith("-0002")) throw new Error(`Second invoice failed: ${JSON.stringify(result)}`);
    await page.getByText(/udstedt\./).waitFor();
    evidence = { invoiceId: result.invoice.documentId, number: result.invoice.invoiceNumber, pdf: (await (await page.request.get(`${url}/invoices/${result.invoice.documentId}/pdf`)).body()).toString("base64"), credits: [] };
    await page.goto(`${base}/companies/synthetic/fakturaer`);
    const row = page.getByRole("row").filter({ hasText: evidence.number });
    await row.getByRole("button", { name: "Bogfør faktura", exact: true }).click();
    const post = page.getByRole("dialog");
    await post.getByLabel(/^Omsætningskonto/).selectOption("1000");
    await post.getByRole("button", { name: "Bogfør salget", exact: true }).click();
    await post.waitFor({ state: "hidden" });
    await row.getByText("Bogført", { exact: true }).waitFor();
    const excess = await page.request.post(`${url}/invoices/credit-note`, { headers, data: { invoiceDocumentId: evidence.invoiceId, issueDate: today, grossAmount: 626, reason: "Synthetic excess", confirm: true } });
    if (excess.status() !== 422) throw new Error("Over-credit was accepted");
    const missing = await page.request.post(`${url}/invoices/credit-note`, { headers, data: { invoiceDocumentId: evidence.invoiceId, issueDate: today, reason: "", confirm: true } });
    if (missing.status() !== 422) throw new Error("Credit without reason accepted");
    for (const example of [
      { input: "250,00", gross: 250, vat: 50, remaining: 375, income: 1300, outputVat: 325, reason: "Synthetic partial correction" },
      { input: "", gross: 375, vat: 75, remaining: 0, income: 1000, outputVat: 250, reason: "Synthetic final correction" },
    ]) {
      await row.getByRole("button", { name: "Kreditér", exact: true }).click();
      const dialog = page.getByRole("dialog");
      await dialog.getByLabel("Beløb inkl. moms (valgfrit)", { exact: true }).fill(example.input);
      await dialog.getByLabel("Begrundelse", { exact: true }).fill(example.reason);
      const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/invoices/credit-note") && r.request().method() === "POST"), dialog.getByRole("button", { name: "Udsted kreditnota", exact: true }).click()]);
      const credit = await response.json();
      if (!response.ok() || !credit.creditNote.creditNoteNumber.startsWith("CN-")) throw new Error(`Credit failed: ${JSON.stringify(credit)}`);
      await dialog.waitFor({ state: "hidden" });
      if (example.remaining) await row.getByRole("cell", { name: "375,00 kr.", exact: true }).waitFor();
      else await row.getByText("Krediteret", { exact: true }).waitFor();
      const original = await page.request.get(`${url}/documents/${credit.creditNote.documentId}/file`);
      const bytes = await original.body();
      const snapshot = JSON.parse(bytes.toString("utf8"));
      if (!original.ok() || snapshot.originalInvoiceDocumentId !== evidence.invoiceId || snapshot.reason !== example.reason || snapshot.grossAmount !== example.gross || snapshot.vatAmount !== example.vat || snapshot.remainingAfterThisCredit !== example.remaining) throw new Error("Credit snapshot lacks reference, reason or exact allocation");
      evidence.credits.push({ id: credit.creditNote.documentId, bytes: bytes.toString("base64"), gross: example.gross, vat: example.vat });
      const report = await (await page.request.get(`${url}/income-statement`)).json();
      const vat = await (await page.request.get(`${url}/vat`)).json();
      if (report.incomeStatement.totalIncome !== example.income || vat.vat.outputVat !== example.outputVat || vat.vat.vatReportErrors.length) throw new Error(`Partial credit did not reduce sales/VAT consistently: ${JSON.stringify(vat)}`);
    }
    writeFileSync(evidencePath, JSON.stringify(evidence));
    const repeated = await page.request.post(`${url}/invoices/credit-note`, { headers, data: { invoiceDocumentId: evidence.invoiceId, issueDate: today, reason: "Synthetic repeated full credit", confirm: true } });
    if (repeated.status() !== 422) throw new Error("Fully credited invoice credited twice");
  } else { evidence = JSON.parse(readFileSync(evidencePath, "utf8")); }
  const list = await (await page.request.get(`${url}/invoices`)).json();
  const invoice = list.invoices.invoices.find((i: { documentId: number }) => i.documentId === evidence.invoiceId);
  if (!invoice || invoice.status !== "credited" || invoice.openBalance !== 0 || invoice.creditableBalance !== 0) throw new Error("Credited invoice state not retained");
  for (const credit of evidence.credits) {
    const original = await page.request.get(`${url}/documents/${credit.id}/file`);
    if (!original.ok() || !(await original.body()).equals(Buffer.from(credit.bytes, "base64"))) throw new Error("Immutable credit note changed");
  }
  const original = await page.request.get(`${url}/invoices/${evidence.invoiceId}/pdf`);
  if (!original.ok() || !(await original.body()).equals(Buffer.from(evidence.pdf, "base64"))) throw new Error("Original invoice PDF changed during crediting");
  const report = await (await page.request.get(`${url}/income-statement`)).json();
  const vat = await (await page.request.get(`${url}/vat`)).json();
  if (report.incomeStatement.totalIncome !== (reopened ? 0 : 1000) || report.incomeStatement.result !== (reopened ? -1842.25 : -842.25) || vat.vat.outputVat !== (reopened ? 0 : 250) || vat.vat.payable !== (reopened ? -25 : 225) || vat.vat.vatReportErrors.length) throw new Error("Fully credited sale still changes revenue/VAT");
  const balance = await (await page.request.get(`${url}/trial-balance`)).json();
  if (!balance.trialBalance.balanced || balance.trialBalance.rows.find((a: { accountNo: string }) => a.accountNo === "1100")?.balance !== 0) throw new Error("Credited receivable remains in ledger");
  await page.goto(`${base}/companies/synthetic/fakturaer`);
  const row = page.getByRole("row").filter({ hasText: evidence.number });
  await row.getByText("Krediteret", { exact: true }).waitFor();
  if (await row.getByRole("button", { name: "Kreditér", exact: true }).count()) throw new Error("Fully credited invoice offers another credit");
  await page.screenshot({ path: "/tmp/rentemester-rust-credit-note.png", fullPage: true });
  console.log(reopened ? "PASS: partial/final credit notes, exact original snapshots, unchanged invoice PDF, credited status and correct receivable/sales/VAT persist after restart" : "PASS: real browser → issue/book invoice → partial credit → remaining full credit → exact reduction of receivable, sales and VAT; missing reason, excess and repeated credit refused");
}
