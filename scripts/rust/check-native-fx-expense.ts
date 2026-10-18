import type { Page } from "@playwright/test";
const cases = [
  { name: "derived", payment: "Synthetic EUR settled in DKK", dkk: 746.25, rate: 7.4625, source: "derived_dkk_settlement" },
  { name: "imported", payment: "Synthetic EUR imported FX", dkk: 746, rate: 7.46, source: "imported_bank" },
];
export async function checkNativeFxExpense(page: Page, base: string, reopened = false) {
  const headers = { origin: base, "content-type": "application/json" };
  const today = new Date().toISOString().slice(0, 10);
  if (!reopened) {
    await page.goto(`${base}/companies/synthetic/bank`);
    await page.getByRole("button", { name: "Importér kontoudtog", exact: true }).click();
    const dialog = page.getByRole("dialog");
    const csv = `date;text;amount;currency;amount_dkk;fx_rate_to_dkk\n${today};${cases[0].payment};-746,25;DKK;;\n${today};${cases[1].payment};-100;EUR;-746;7,46\n`;
    await dialog.getByLabel("CSV-fil", { exact: true }).setInputFiles({ name: "synthetic-fx-bank.csv", mimeType: "text/csv", buffer: Buffer.from(csv) });
    await dialog.getByLabel(/^Bankkonto \(valgfri\)/).fill("synthetic-operating-bank");
    await dialog.getByRole("button", { name: "Importér", exact: true }).click();
    await dialog.getByText(/2 transaktioner importeret/).waitFor().catch(async (error) => { console.error(await dialog.innerText()); throw error; });
    await dialog.getByRole("button", { name: "Luk", exact: true }).click();
    const invalid = await page.request.post(`${base}/api/companies/synthetic/bank/import`, { headers, data: { confirm: true, account: "synthetic-operating-bank", csvContent: `date;text;amount;currency;amount_dkk;fx_rate_to_dkk\n${today};Synthetic invalid quote;-100;EUR;-745;7.46\n` } });
    if (invalid.status() !== 422) throw new Error("Inconsistent foreign bank FX was imported");
    for (const example of cases) {
      await page.goto(`${base}/companies/synthetic/bilag`);
      await page.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
      const upload = page.getByRole("dialog");
      await upload.getByLabel(/^Bilagstype/).selectOption("cash_register_receipt");
      await upload.getByLabel("Bilagsfil", { exact: true }).setInputFiles({ name: `synthetic-fx-${example.name}.txt`, mimeType: "text/plain", buffer: Buffer.from(`Synthetic EUR receipt ${example.name}\nGross EUR 100, local foreign tax EUR 20\n`) });
      await upload.getByLabel("Kilde", { exact: true }).fill("Synthetic foreign receipt");
      await upload.getByLabel("Bilagsdato (valgfri)", { exact: true }).fill(today);
      await upload.getByLabel("Beløb inkl. moms (valgfri)", { exact: true }).fill("100,00");
      await upload.getByLabel("Momsbeløb (valgfri)", { exact: true }).fill("20,00");
      await upload.getByLabel("Valuta", { exact: true }).fill("EUR");
      await upload.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
      await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor();
      await upload.getByRole("button", { name: "Luk", exact: true }).click();
      const list = await (await page.request.get(`${base}/api/companies/synthetic/documents`)).json();
      const document = list.documents.documents.find((doc: { filename: string }) => doc.filename === `synthetic-fx-${example.name}.txt`);
      if (!document) throw new Error("Foreign voucher not retained");
      const response = await (await page.request.get(`${base}/api/companies/synthetic/documents/${document.id}/booking-options`)).json();
      const payment = response.options.unmatchedOutgoingBank.find((row: { text: string }) => row.text === example.payment);
      if (!payment) throw new Error("Foreign payment not offered for matching");
      await page.getByRole("row").filter({ hasText: document.documentNo }).getByRole("link", { name: "Bogfør bilag", exact: true }).click();
      await page.getByLabel(/^Udgiftskonto/).selectOption("3000");
      await page.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).selectOption(String(payment.id));
      await page.getByLabel(/^Moms-behandling/).selectOption("non_deductible");
      const [posted] = await Promise.all([page.waitForResponse((reply) => reply.url().endsWith("/documents/book-expense") && reply.request().method() === "POST"), page.getByRole("button", { name: "Bogfør", exact: true }).click()]);
      const result = await posted.json();
      if (!posted.ok() || result.booking.grossAmountForeign !== 100 || result.booking.grossAmountDkk !== example.dkk || result.booking.fxRateToDkk !== example.rate || result.booking.fxRateSource !== example.source || result.booking.vatAmountDkk !== 0) throw new Error(`Incorrect FX booking: ${JSON.stringify(result)}`);
      await page.getByText(/Bilaget blev bogført som journalpost/).waitFor();
      console.log(`Browser: EUR receipt → ${example.source} → ${example.dkk} DKK posted, no foreign VAT deducted`);
    }
  }
  const list = await (await page.request.get(`${base}/api/companies/synthetic/documents`)).json();
  const journal = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
  for (const example of cases) {
    const document = list.documents.documents.find((doc: { filename: string }) => doc.filename === `synthetic-fx-${example.name}.txt`);
    const entries = journal.journal.entries.filter((entry: { documentId: number }) => entry.documentId === document?.id);
    if (!document || document.amountIncVat !== 100 || document.currency !== "EUR" || entries.length !== 1 || entries[0].total !== example.dkk) throw new Error("Persisted foreign original or DKK posting changed");
  }
  const bank = await (await page.request.get(`${base}/api/companies/synthetic/bank`)).json();
  if (bank.bank.transactions.length !== 3 || bank.bank.matchedCount !== 3 || bank.bank.unmatchedCount !== 0 || bank.bank.bookedBalance !== -1742.25) throw new Error(`Incorrect persisted FX bank balance: ${JSON.stringify(bank.bank)}`);
  await page.goto(`${base}/companies/synthetic/resultatopgorelse`);
  await page.getByRole("heading", { name: "Resultatopgørelse", exact: true }).waitFor();
  const report = await (await page.request.get(`${base}/api/companies/synthetic/income-statement`)).json();
  if (report.incomeStatement.result !== -1742.25 || report.incomeStatement.totalExpense !== 1742.25) throw new Error("Foreign expense did not appear at its DKK settlement value");
  await page.screenshot({ path: "/tmp/rentemester-rust-fx-expense.png", fullPage: true });
  console.log(reopened ? "PASS: both FX paths, original EUR vouchers, DKK postings, all three reconciliations and profit persist after restart" : "PASS: real browser foreign vouchers + bank FX or derived DKK settlement → expense → reconciliation → correct DKK profit");
}
