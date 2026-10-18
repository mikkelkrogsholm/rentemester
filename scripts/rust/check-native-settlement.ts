import type { Page } from "@playwright/test";

export async function checkNativeSettlement(page: Page, base: string, reopened = false) {
  const url = `${base}/api/companies/synthetic`;
  const invoices = await (await page.request.get(`${url}/invoices`)).json();
  const invoice = invoices.invoices.invoices[0];
  if (!invoice) throw new Error("Settlement invoice missing");
  if (!reopened) {
    const today = new Date().toISOString().slice(0, 10);
    await page.goto(`${base}/companies/synthetic/bank`);
    await page.getByRole("button", { name: "Importér kontoudtog", exact: true }).click();
    const dialog = page.getByRole("dialog");
    const csv = `date;text;amount;currency;reference\n${today};Synthetic customer part one;500;DKK;synthetic-receipt-one\n${today};Synthetic customer part two;750;DKK;synthetic-receipt-two\n`;
    await dialog.getByLabel("CSV-fil", { exact: true }).setInputFiles({ name: "synthetic-customer-receipts.csv", mimeType: "text/csv", buffer: Buffer.from(csv) });
    await dialog.getByLabel(/^Bankkonto \(valgfri\)/).fill("synthetic-operating-bank");
    await dialog.getByRole("button", { name: "Importér", exact: true }).click();
    await dialog.getByText(/2 transaktioner importeret/).waitFor();
    await dialog.getByRole("button", { name: "Luk", exact: true }).click();
    for (const [reference, remaining] of [["synthetic-receipt-one", 750], ["synthetic-receipt-two", 0]] as const) {
      await page.goto(`${base}/companies/synthetic/fakturaer`);
      const row = page.getByRole("row").filter({ hasText: invoice.invoiceNo });
      await row.getByRole("button", { name: "Afstem", exact: true }).click();
      const confirm = page.getByRole("dialog");
      await confirm.getByLabel("Bankreference", { exact: true }).fill(reference);
      const [posted] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/invoices/settle") && r.request().method() === "POST"), confirm.getByRole("button", { name: "Afstem faktura", exact: true }).click()]);
      const result = await posted.json();
      if (!posted.ok() || result.settlement.openBalance !== remaining || result.settlement.claimAmount !== 0) throw new Error(`Wrong invoice settlement: ${JSON.stringify(result)}`);
      await confirm.waitFor({ state: "hidden" });
      if (remaining) await row.getByRole("cell", { name: "750,00 kr.", exact: true }).waitFor();
      else await row.getByText("Betalt", { exact: true }).waitFor();
    }
    const duplicate = await page.request.post(`${url}/invoices/settle`, { headers: { origin: base, "content-type": "application/json" }, data: { invoiceDocumentId: invoice.documentId, bankTransactionReference: "synthetic-receipt-one", confirm: true } });
    if (duplicate.status() !== 422) throw new Error("Customer receipt settled twice");
  }
  const current = await (await page.request.get(`${url}/invoices`)).json();
  if (current.invoices.invoices[0].status !== "paid" || current.invoices.invoices[0].openBalance !== 0 || current.invoices.totalOpen !== 0) throw new Error("Invoice remains outstanding");
  const contacts = await (await page.request.get(`${url}/contacts`)).json();
  const customer = contacts.contacts.customers.find((c: { name: string }) => c.name === "Synthetic Customer");
  if (!customer || customer.openBalance !== 0 || customer.openInvoiceCount !== 0) throw new Error("Customer receivable remains outstanding");
  const bank = await (await page.request.get(`${url}/bank`)).json();
  if (bank.bank.transactions.length !== 6 || bank.bank.matchedCount !== 6 || bank.bank.unmatchedCount !== 0 || bank.bank.bookedBalance !== -617.25) throw new Error(`Incorrect bank after receipts: ${JSON.stringify(bank)}`);
  const report = await (await page.request.get(`${url}/income-statement`)).json();
  if (report.incomeStatement.totalIncome !== 1000 || report.incomeStatement.result !== -842.25) throw new Error("Receipt booked revenue again");
  const vat = await (await page.request.get(`${url}/vat`)).json();
  if (vat.vat.outputVat !== 250 || vat.vat.payable !== 225 || vat.vat.vatReportErrors.length) throw new Error("Receipt booked VAT again");
  const balance = await (await page.request.get(`${url}/trial-balance`)).json();
  const debtor = balance.trialBalance.rows.find((a: { accountNo: string }) => a.accountNo === "1100");
  if (!balance.trialBalance.balanced || debtor?.balance !== 0) throw new Error("Invoice is marked paid but ledger receivable is not cleared");
  const journal = await (await page.request.get(`${url}/journal`)).json();
  const entries = journal.journal.entries.filter((e: { documentId: number }) => e.documentId === invoice.documentId);
  const receipts = entries.filter((e: { lines: unknown[] }) => e.lines.length === 2);
  if (entries.length !== 3 || receipts.length !== 2 || receipts.some((e: { lines: Array<{ accountNo: string }> }) => e.lines.some(l => !["1100", "1800"].includes(l.accountNo)))) throw new Error("Receipt journals include extra revenue/VAT or duplicates");
  await page.goto(`${base}/companies/synthetic/fakturaer`);
  const row = page.getByRole("row").filter({ hasText: invoice.invoiceNo });
  await row.getByText("Betalt", { exact: true }).waitFor();
  if (await row.getByRole("button", { name: "Afstem", exact: true }).count()) throw new Error("Fully paid invoice still offers another settlement");
  await page.screenshot({ path: "/tmp/rentemester-rust-settlement.png", fullPage: true });
  console.log(reopened ? "PASS: two receipts, paid invoice, zero customer/ledger receivable, six bank reconciliations and unchanged sales/VAT persist after restart" : "PASS: real browser → import two customer receipts → partial then full settlement → paid invoice and cleared receivable; no duplicate receipt or repeated sales/VAT");
}
