import type { Page } from "@playwright/test";

export async function checkNativeCompanySetup(page: Page, base: string, reopened = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const today = new Date().toISOString().slice(0, 10);
  if (!reopened) {
    await page.goto(`${base}/companies/synthetic/manage`);
    await page.getByRole("heading", { name: "Virksomhedsoplysninger", exact: true }).waitFor().catch(async error => { console.error(await page.locator("body").innerText()); console.error(await (await page.request.get(`${url}/company`)).text()); throw error; });
    const profile = page.getByRole("form", { name: "Rediger stamdata" });
    await profile.getByLabel("Adresse", { exact: true }).fill("Synthetic Street 1");
    await profile.getByLabel("Postnummer", { exact: true }).fill("1000");
    await profile.getByLabel("By", { exact: true }).fill("Synthetic City");
    await profile.getByLabel(/^Momsperiode/).selectOption("quarter");
    await profile.getByRole("button", { name: "Gem stamdata", exact: true }).click();
    await page.getByText("Stamdata opdateret.", { exact: true }).waitFor();
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const account of [
      { no: "4100", name: "Synthetic deductible VAT", type: "vat", code: "" },
      { no: "3100", name: "Synthetic domestic purchases", type: "expense", code: "DK_PURCHASE_25" },
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(account.no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(account.name);
      await form.getByLabel(/^Kontotype/).selectOption(account.type);
      await form.getByLabel(/^Normal saldo/).selectOption("debit");
      await form.getByLabel("Standard momskode (valgfri)", { exact: true }).fill(account.code);
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("cell", { name: account.name, exact: true }).waitFor();
    }
    const role = page.getByRole("form", { name: "Bekræft kontorolle", exact: true });
    await role.getByLabel(/^Kontorolle/).selectOption("input_vat");
    await role.getByLabel(/^Konto til rollen/).selectOption("4100");
    await role.getByRole("button", { name: "Bekræft kontorolle", exact: true }).click();
    await page.getByRole("row").filter({ hasText: "Købsmoms" }).getByText(/Konto 4100/).waitFor();
    console.log("Browser: saved VAT registration and company profile → created chart accounts → explicitly confirmed input VAT role");
    const incompatible = await page.request.post(`${url}/account-roles`, { headers, data: { role: "input_vat", accountNo: "1800", confirm: true } });
    if (incompatible.status() !== 422) throw new Error("Incompatible VAT role accepted");
    const duplicate = await page.request.post(`${url}/accounts`, { headers, data: { accountNo: "3100", name: "Overwrite", type: "expense", normalBalance: "debit" } });
    if (duplicate.status() !== 422) throw new Error("Existing chart account overwritten");
    await page.goto(`${base}/companies/synthetic/bank`);
    await page.getByRole("button", { name: "Importér kontoudtog", exact: true }).click();
    const bank = page.getByRole("dialog");
    await bank.getByLabel("CSV-fil", { exact: true }).setInputFiles({ name: "synthetic-domestic-bank.csv", mimeType: "text/csv", buffer: Buffer.from(`date;text;amount;currency\n${today};Synthetic domestic VAT payment;-125;DKK\n`) });
    await bank.getByLabel(/^Bankkonto \(valgfri\)/).fill("synthetic-operating-bank");
    await bank.getByRole("button", { name: "Importér", exact: true }).click();
    await bank.getByText(/1 transaktion importeret/).waitFor();
    await bank.getByRole("button", { name: "Luk", exact: true }).click();
    await page.goto(`${base}/companies/synthetic/bilag`);
    await page.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
    const upload = page.getByRole("dialog");
    await upload.getByLabel("Bilagsfil", { exact: true }).setInputFiles({ name: "synthetic-domestic-vat.txt", mimeType: "text/plain", buffer: Buffer.from("Synthetic domestic invoice SYN-VAT-1; net DKK 100; VAT DKK 25; gross DKK 125\n") });
    for (const [label, value] of Object.entries({ "Bilagsdato": today, "Fakturanr. (valgfri)": "SYN-VAT-1", "Beløb inkl. moms": "125,00", "Momsbeløb": "25,00", "Beskrivelse af leverance": "Synthetic office supplies", "Afsender": "Synthetic Supplier", "Afsender CVR/moms": "DK12345678", "Afsenderadresse": "Synthetic Supplier Street 2", "Modtager": "Synthetic Company", "Modtager CVR/moms": "DK87654321", "Modtageradresse": "Synthetic Street 1" })) {
      await upload.getByLabel(label, { exact: true }).fill(value);
    }
    await upload.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
    await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor().catch(async error => { console.error(await upload.innerText()); throw error; });
    await upload.getByRole("button", { name: "Luk", exact: true }).click();
    const list = await (await page.request.get(`${url}/documents`)).json();
    const document = list.documents.documents.find((d: { filename: string }) => d.filename === "synthetic-domestic-vat.txt");
    if (!document) throw new Error("Domestic invoice missing");
    const options = await (await page.request.get(`${url}/documents/${document.id}/booking-options`)).json();
    const payment = options.options.unmatchedOutgoingBank.find((p: { text: string }) => p.text === "Synthetic domestic VAT payment");
    if (!payment) throw new Error("Domestic payment missing");
    await page.getByRole("row").filter({ hasText: document.documentNo }).getByRole("link", { name: "Bogfør bilag", exact: true }).click();
    await page.getByLabel(/^Udgiftskonto/).selectOption("3100");
    await page.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).selectOption(String(payment.id));
    const [posted] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/documents/book-expense") && r.request().method() === "POST"), page.getByRole("button", { name: "Bogfør", exact: true }).click()]);
    const result = await posted.json();
    if (!posted.ok() || result.booking.netAmountDkk !== 100 || result.booking.vatAmountDkk !== 25 || result.booking.grossAmountDkk !== 125) throw new Error(`Wrong domestic VAT booking: ${JSON.stringify(result)}`);
    await page.getByText(/Bilaget blev bogført som journalpost/).waitFor();
    const deregister = await page.request.patch(`${url}/company`, { headers, data: { address: "Should roll back", vatPeriodType: null } });
    if (deregister.status() !== 422) throw new Error("Open VAT activity allowed deregistration");
  }
  const company = await (await page.request.get(`${url}/company`)).json();
  if (company.company.vatPeriodType !== "quarter" || company.company.address !== "Synthetic Street 1") throw new Error("Company settings lost or refused update partially applied");
  const chart = await (await page.request.get(`${url}/accounts`)).json();
  const inputVat = chart.accounts.accountRoles.resolutions.find((r: { role: string }) => r.role === "input_vat");
  if (!inputVat.ok || inputVat.accountNo !== "4100" || !chart.accounts.accounts.some((a: { accountNo: string; defaultVatCode: string }) => a.accountNo === "3100" && a.defaultVatCode === "DK_PURCHASE_25")) throw new Error("Chart setup not retained");
  const bank = await (await page.request.get(`${url}/bank`)).json();
  if (bank.bank.transactions.length !== (reopened ? 6 : 4) || bank.bank.matchedCount !== (reopened ? 6 : 4) || bank.bank.unmatchedCount !== 0 || bank.bank.bookedBalance !== (reopened ? -617.25 : -1867.25)) throw new Error(`Incorrect bank balance after deductible VAT: ${JSON.stringify(bank)}`);
  await page.goto(`${base}/companies/synthetic/resultatopgorelse`);
  await page.getByRole("heading", { name: "Resultatopgørelse", exact: true }).waitFor();
  await page.getByText("Synthetic domestic purchases", { exact: true }).waitFor();
  const report = await (await page.request.get(`${url}/income-statement`)).json();
  if (report.incomeStatement.result !== (reopened ? -842.25 : -1842.25) || report.incomeStatement.totalExpense !== 1842.25) throw new Error("Deductible VAT was incorrectly expensed");
  await page.goto(`${base}/companies/synthetic/balance`);
  const balanceResponse = await page.request.get(`${url}/trial-balance`);
  const balance = await balanceResponse.json();
  const vat = balance.trialBalance?.rows.find((r: { accountNo: string }) => r.accountNo === "4100");
  if (!balanceResponse.ok() || !balance.trialBalance.balanced || vat?.debit !== 25 || vat?.credit !== 0 || vat?.balance !== 25) throw new Error(`Incorrect VAT asset: ${JSON.stringify(balance)}`);
  await page.getByRole("heading", { name: "Balance", exact: true }).waitFor();
  await page.getByText("Synthetic deductible VAT", { exact: true }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-company-setup.png", fullPage: true });
  console.log(reopened ? "PASS: company settings, confirmed VAT role, chart accounts and deductible purchase persist after restart; reconciled bank payments and correct profit" : "PASS: real browser VAT setup → create chart accounts → confirm input VAT → import payment → invoice → deductible purchase → correct profit; incompatible role, overwrite and open-VAT deregistration refused");
}
