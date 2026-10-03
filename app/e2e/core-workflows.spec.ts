import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { COMPANY_SLUG, mockApi, type MockResponse } from "./fixtures";
import core from "./data/core.json" with { type: "json" };
import type { DocumentRow } from "../src/lib/types";

declare global { interface Window { chartFontDraws?: boolean[]; } }

test("native confirmation owns focus, makes background inert, and returns focus after Escape", async ({ page }) => {
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/kontakter?year=2026`);
  const trigger = page.getByRole("button", { name: "Slet Kunde A/S", exact: true });
  await trigger.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Annullér", exact: true })).toBeFocused();
  expect(await dialog.evaluate((element) => element instanceof HTMLDialogElement && element.matches(":modal"))).toBe(true);
  expect(await dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  await page.locator("main h1, main h2").first().evaluate((element) => { (element as HTMLElement).tabIndex = 0; (element as HTMLElement).focus(); });
  expect(await dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  for (let index = 0; index < 6; index++) {
    await page.keyboard.press("Tab");
    expect(await dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  }
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  fixture.assertComplete();
});

test("confirmation sends one explicit mutation, stays open while busy, and shows server rejection", async ({ page }) => {
  const fixture = await mockApi(page, { overrides: {
    "DELETE /api/companies/acme-aps/customers/1": { delayMs: 500, status: 409, body: { ok: false, code: "business_rejection", errors: ["Kunden har bogførte fakturaer."] } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/kontakter?year=2026`);
  await page.getByRole("button", { name: "Slet Kunde A/S", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Slet", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("alert")).toContainText("Kunden har bogførte fakturaer");
  const writes = fixture.calls.filter((call) => call.method !== "GET");
  expect(writes).toHaveLength(1);
  expect(writes[0].body).toEqual({ confirm: true });
  await expect(page.locator("main")).toContainText("Kunde A/S");
  fixture.assertComplete();
});

for (const width of [320, 390, 768, 1024, 1440]) {
  test(`daily document layout fits ${width}px with compact context`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 844 });
    const fixture = await mockApi(page);
    await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
    await expect(page.getByRole("main")).toContainText("DOC-2026-000001");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    if (width < 640) {
      const title = page.getByRole("heading", { level: 1 });
      await expect(title).toHaveCount(1);
      await expect(title).toContainText("Bilag");
      expect((await title.boundingBox())?.y).toBeLessThan(350);
    }
    await page.screenshot({ path: testInfo.outputPath(`documents-${width}.png`), fullPage: true });
    fixture.assertComplete();
  });
}

test("a year change hides old bank data immediately and a late response cannot replace the newest year", async ({ page }) => {
  let releaseLate: () => void = () => {};
  const late = new Promise<void>((resolve) => { releaseLate = resolve; });
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/bank": async (route) => {
      const year = new URL(route.request().url()).searchParams.get("year") ?? "2026";
      if (year === "2025") await late;
      return { body: { ok: true, bank: { ...core.bank, selectedYear: year, archived: year === "2025", transactions: [{ ...core.bank.transactions[0], text: `Syntetisk bankpost ${year}` }] } } };
    },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/bank?year=2026`);
  await expect(page.getByRole("main")).toContainText("Syntetisk bankpost 2026");
  const incoming = page.waitForRequest((request) => request.url().includes("/bank?year=2025"));
  await page.getByRole("combobox", { name: "Vælg regnskabsår" }).selectOption("2025");
  await incoming;
  await expect(page.getByText("Syntetisk bankpost 2026", { exact: true })).toHaveCount(0);
  await page.getByRole("combobox", { name: "Vælg regnskabsår" }).selectOption("2026");
  await expect(page.getByRole("main")).toContainText("Syntetisk bankpost 2026");
  releaseLate();
  await expect(page.getByRole("combobox", { name: "Vælg regnskabsår" })).toHaveValue("2026");
  await expect(page.getByText("Syntetisk bankpost 2025", { exact: true })).toHaveCount(0);
  fixture.assertComplete();
});

test("invoice creation is a page and protects changed inputs during navigation", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/ny?year=2026`);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Ny faktura");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await page.locator(".workflow-form").evaluate(element => getComputedStyle(element.parentElement!).maxHeight)).toBe("none");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Syntetisk rådgivning");
  await page.getByRole("link", { name: "Tilbage til fakturaer", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Forlad siden?" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Annullér", exact: true }).click();
  await expect(page).toHaveURL(/\/fakturaer\/ny\?year=2026/);
  await expect(page.getByLabel("Linje 1 beskrivelse", { exact: true })).toHaveValue("Syntetisk rådgivning");
  await page.getByRole("link", { name: "Tilbage til fakturaer", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Forlad siden", exact: true }).click();
  await expect(page).toHaveURL(/\/fakturaer\?year=2026/);
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("invoice server validation preserves the form and parses Danish amount once", async ({ page }) => {
  const fixture = await mockApi(page, { overrides: {
    "POST /api/companies/acme-aps/invoices/issue": { status: 422, body: { ok: false, code: "validation_failed", errors: ["Fakturadatoen ligger i en låst periode."] } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/ny?year=2026`);
  await page.getByLabel("Fakturadato", { exact: true }).fill("2026-02-01");
  await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Syntetisk rådgivning");
  await page.getByLabel("Linje 1 antal", { exact: true }).fill("2");
  await page.getByLabel("Linje 1 enhedspris", { exact: true }).fill("1.234,56");
  await page.getByRole("button", { name: "Udsted faktura", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Fakturadatoen ligger i en låst periode");
  await expect(page.getByLabel("Linje 1 beskrivelse", { exact: true })).toHaveValue("Syntetisk rådgivning");
  await expect(page.getByLabel("Linje 1 enhedspris", { exact: true })).toHaveValue("1.234,56");
  const writes = fixture.calls.filter((call) => call.method !== "GET");
  expect(writes).toHaveLength(1);
  expect(writes[0].body).toMatchObject({ issueDate: "2026-02-01", lines: [{ description: "Syntetisk rådgivning", quantity: 2, unitPriceExVat: 1234.56 }] });
  fixture.assertComplete();
});

test("an uncertain invoice issue remains blocked after reload and reconciliation never repeats the write", async ({ page }) => {
  let committed = false;
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/invoices": async () => ({ body: { ok: true, invoices: { ...core.invoices, invoices: [...core.invoices.invoices, ...(committed ? [{ ...core.invoices.invoices[0], documentId: 99, invoiceNo: "2026-0099", invoiceDate: "2026-02-01", customerName: "Syntetisk kunde", grossAmount: 1250 }] : [])] } } }),
    "POST /api/companies/acme-aps/invoices/issue": async () => { committed = true; return { status: 500, body: { ok: false, code: "internal", errors: ["Syntetisk afbrudt svar efter behandling."] } }; },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/ny?year=2026`);
  await page.getByLabel("Fakturadato", { exact: true }).fill("2026-02-01");
  await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Syntetisk rådgivning");
  await page.getByLabel("Linje 1 antal", { exact: true }).fill("1");
  await page.getByLabel("Linje 1 enhedspris", { exact: true }).fill("1000");
  await page.getByRole("button", { name: "Udsted faktura", exact: true }).click();
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  await expect(page.getByLabel("Linje 1 beskrivelse", { exact: true })).toHaveValue("Syntetisk rådgivning");
  page.once("dialog", (dialog) => void dialog.accept());
  await page.reload();
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Kontrollér status", exact: true }).click();
  await expect(page.getByRole("status", { name: "Fakturastatus fra serveren" })).toContainText("2026-0099");
  await expect(page.getByRole("status", { name: "Fakturastatus fra serveren" })).toContainText("Syntetisk kunde");
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Resultatet er afklaret", exact: true }).click();
  await page.getByRole("dialog", { name: "Frigiv efter kontrol" }).getByRole("button", { name: "Jeg har kontrolleret resultatet" }).click();
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeEnabled();
  expect(fixture.calls.filter((call) => call.method === "POST")).toHaveLength(1);
  fixture.assertComplete();
});

test("a company reader can inspect budgets and recurring invoices without write controls", async ({ page }) => {
  const fixture = await mockApi(page, { profile: "hosted", role: "reader" });
  await page.goto(`/companies/${COMPANY_SLUG}/budget?year=2026`);
  const cells = page.getByRole("textbox", { name: /^Budget for konto / });
  await expect(cells.first()).toBeVisible();
  for (const cell of await cells.all()) await expect(cell).toHaveAttribute("readonly", "");
  await page.goto(`/companies/${COMPANY_SLUG}/faktura-skabeloner?year=2026`);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Faktura-skabeloner");
  await expect(page.getByRole("button", { name: "Deaktivér", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Generér", exact: true })).toHaveCount(0);
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("unavailable session storage blocks invoice writes before an attempt and after reload", async ({ page }) => {
  await page.addInitScript(() => {
    Storage.prototype.setItem = () => { throw new DOMException("Synthetic storage denial", "SecurityError"); };
  });
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/ny?year=2026`);
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  await expect(page.getByRole("alert")).toContainText("browseren ikke kan gemme");
  await expect(page.getByRole("button", { name: "Resultatet er afklaret", exact: true })).toBeDisabled();
  await page.reload();
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Kontrollér browserens lager", exact: true }).click();
  await expect(page.getByRole("button", { name: "Udsted faktura", exact: true })).toBeDisabled();
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("document booking remains explicit and keeps account and bank choices after rejection", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const voucher = { ...core.documents.documents[0], journalEntryId: null, journalEntryNo: null, journalEntryText: null, journalEntryTotal: null };
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/documents": { body: { ok: true, documents: { ...core.documents, documents: [voucher], linkedCount: 0, unlinkedCount: 1 } } },
    "GET /api/companies/acme-aps/documents/1/booking-options": { body: { ok: true, options: { document: { ...voucher, vatAmount: 250, purchaseVatLines: [{ classification: "dk_purchase_25", netAmount: 1000, vatAmount: 250 }] }, expenseAccounts: [{ accountNo: "3000", name: "Kontorartikler", defaultVatCode: "DK_PURCHASE_25" }], unmatchedOutgoingBank: [{ id: 1, date: "2026-02-01", text: "Syntetisk køb", amount: -1250, currency: "DKK", amountDkk: null, fxRateToDkk: null, reference: null }] } } },
    "GET /api/companies/acme-aps/documents/1/vat-preflight": { body: { ok: true, preflight: { ok: true, derivedRegion: "DK", requiredValidation: null, cache: { reused: true, freshUntil: null }, applyWouldCallProvider: false, errors: [], exception: null } } },
    "POST /api/companies/acme-aps/documents/book-expense": { status: 409, body: { ok: false, code: "period_locked", errors: ["Perioden er låst. Bilaget blev ikke bogført."] } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag/1/bogfoer?year=2026`);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Bogfør bilag");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByRole("combobox", { name: "Udgiftskonto", exact: true }).selectOption("3000");
  await expect(page.getByRole("combobox", { name: "Banktransaktion (uafstemt, udgående)", exact: true })).toHaveValue("1");
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  await page.getByRole("button", { name: "Bogfør", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Perioden er låst");
  await expect(page.getByRole("combobox", { name: "Udgiftskonto", exact: true })).toHaveValue("3000");
  await expect(page.getByRole("combobox", { name: "Banktransaktion (uafstemt, udgående)", exact: true })).toHaveValue("1");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  const writes = fixture.calls.filter((call) => call.method !== "GET");
  expect(writes).toHaveLength(1);
  expect(writes[0].body).toMatchObject({ documentId: 1, bankTransactionId: 1, expenseAccountNo: "3000", confirm: true });
  fixture.assertComplete();
});

test("uncertain document booking shows authoritative postings when checking status without another write", async ({ page }) => {
  let committed = false;
  const voucher = { ...core.documents.documents[0], journalEntryId: null, journalEntryNo: null, journalEntryText: null, journalEntryTotal: null };
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/documents": async () => ({ body: { ok: true, documents: { ...core.documents, documents: [{ ...voucher, ...(committed ? { journalEntryId: 42, journalEntryNo: "J-2026-0042", journalEntryText: "Syntetisk bogført køb" } : {}) }] } } }),
    "GET /api/companies/acme-aps/documents/1/booking-options": { body: { ok: true, options: { document: { ...voucher, vatAmount: 250 }, expenseAccounts: [{ accountNo: "3000", name: "Kontorartikler", defaultVatCode: "DK_PURCHASE_25" }], unmatchedOutgoingBank: [{ id: 1, date: "2026-02-01", text: "Syntetisk køb", amount: -1250, currency: "DKK", amountDkk: null, fxRateToDkk: null, reference: null }] } } },
    "GET /api/companies/acme-aps/documents/1/vat-preflight": { body: { ok: true, preflight: { ok: true, derivedRegion: "DK", requiredValidation: null, cache: { reused: true, freshUntil: null }, applyWouldCallProvider: false, errors: [], exception: null } } },
    "POST /api/companies/acme-aps/documents/book-expense": async () => { committed = true; return { status: 500, body: { ok: false, code: "internal", errors: ["Syntetisk afbrudt svar efter bogføring."] } }; },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag/1/bogfoer?year=2026`);
  await page.getByRole("combobox", { name: "Udgiftskonto", exact: true }).selectOption("3000");
  await page.getByRole("button", { name: "Bogfør", exact: true }).click();
  await expect(page.getByRole("button", { name: "Bogfør", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Kontrollér status", exact: true }).click();
  await expect(page.getByRole("status", { name: "Bilagsstatus fra serveren" })).toContainText("J-2026-0042");
  await expect(page.getByRole("status", { name: "Bilagsstatus fra serveren" })).toContainText("Syntetisk bogført køb");
  await expect(page.getByRole("button", { name: "Bogfør", exact: true })).toBeDisabled();
  expect(fixture.calls.filter((call) => call.method === "POST")).toHaveLength(1);
  fixture.assertComplete();
});

test("mobile navigation opens a modal menu, navigates, and closes", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  const menu = page.getByRole("button", { name: /Menu|Åbn navigation/ });
  await expect(menu).toBeVisible();
  await menu.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("link", { name: "Bank", exact: true }).click();
  await expect(page).toHaveURL(/\/bank\?year=2026/);
  await expect(page.getByRole("main")).toContainText("Gebyr");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  fixture.assertComplete();
});

test("production assets request no external fonts or other network resources", async ({ page }) => {
  const fixture = await mockApi(page);
  const external: string[] = [];
  const failedAssets: string[] = [];
  page.on("request", (request) => { if (/^https?:/.test(request.url()) && new URL(request.url()).hostname !== "127.0.0.1") external.push(request.url()); });
  page.on("response", (response) => { if (response.status() >= 400 && !response.url().includes("/api/")) failedAssets.push(response.url()); });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  await expect(page.getByRole("main")).toContainText("DOC-2026-000001");
  await page.evaluate(() => document.fonts.ready);
  expect(external).toEqual([]);
  expect(failedAssets).toEqual([]);
  fixture.assertComplete();
});

for (const path of ["/", `/companies/${COMPANY_SLUG}/bilag?year=2026`, `/companies/${COMPANY_SLUG}/kontakter?year=2026`, `/companies/${COMPANY_SLUG}/fakturaer?year=2026`]) {
  test(`core page ${path} has no WCAG AA axe violations`, async ({ page }) => {
    const fixture = await mockApi(page);
    await page.goto(path);
    await expect(page.getByRole("main")).toContainText("Acme ApS");
    const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"]).analyze();
    expect(results.violations.map(({ id, nodes }) => ({ id, targets: nodes.map((node) => node.target) }))).toEqual([]);
    fixture.assertComplete();
  });
}

async function fillInvoice(page: Page) {
  await page.getByLabel("Fakturadato", { exact: true }).fill("2026-02-01");
  await page.getByRole("combobox", { name: "Vælg kunde", exact: true }).selectOption("1");
  await page.getByLabel("Linje 1 beskrivelse", { exact: true }).fill("Syntetisk rådgivning");
  await page.getByLabel("Linje 1 antal", { exact: true }).fill("3");
  await page.getByLabel("Linje 1 enhedspris", { exact: true }).fill("2.000,00");
}

function bookingReadFixtures(voucher: DocumentRow): Record<string, MockResponse> {
  return {
    "GET /api/companies/acme-aps/documents/1/booking-options": { body: { ok: true, options: { document: { ...voucher, vatAmount: 250, purchaseVatLines: [{ classification: "dk_purchase_25", netAmount: 1000, vatAmount: 250 }] }, expenseAccounts: [{ accountNo: "3000", name: "Kontorartikler", defaultVatCode: "DK_PURCHASE_25" }], unmatchedOutgoingBank: [{ id: 1, date: "2026-02-01", text: "Syntetisk køb", amount: -1250, currency: "DKK", amountDkk: null, fxRateToDkk: null, reference: null }] } } },
    "GET /api/companies/acme-aps/documents/1/vat-preflight": { body: { ok: true, preflight: { ok: true, derivedRegion: "DK", requiredValidation: null, cache: { reused: true, freshUntil: null }, applyWouldCallProvider: false, errors: [], exception: null } } },
  };
}

test("invoice detail deep link identifies the document and restores all list filters", async ({ page }) => {
  const fixture = await mockApi(page);
  const returnTo = `/companies/${COMPANY_SLUG}/fakturaer?year=2026&q=Beta&status=overdue&from=2026-01-01&to=2026-12-31&sort=amount&dir=desc&page=1&pageSize=25`;
  const query = new URLSearchParams({ year: "2026", returnTo });
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/2?${query}`);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Faktura 2026-00002");
  await expect(page.getByRole("main")).toContainText("Beta ApS");
  await expect(page.getByRole("link", { name: "Hent PDF", exact: true })).toHaveAttribute("href", "/api/companies/acme-aps/invoices/2/pdf");
  await expect(page.getByRole("link", { name: "2026-00001", exact: true })).toHaveCount(0);
  const back = page.getByRole("link", { name: "Tilbage til fakturaer", exact: true });
  await expect(back).toHaveAttribute("href", returnTo);
  await back.click();
  await expect(page).toHaveURL(new URL(returnTo, page.url()).href);
  await expect(page.getByRole("searchbox", { name: "Søg", exact: true })).toHaveValue("Beta");
  await expect(page.getByRole("combobox", { name: "Status", exact: true })).toHaveValue("overdue");
  await expect(page.getByRole("link", { name: "2026-00002", exact: true })).toBeVisible();
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("document detail shows every posting association and returns to the filtered list", async ({ page }) => {
  const original = core.documents.documents[0];
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/documents": { body: { ok: true, documents: { ...core.documents, linkedCount: 2, documents: [original, { ...original, journalEntryId: 3, journalEntryNo: "B-2026-0003", journalEntryText: "Syntetisk supplerende postering", journalEntryTotal: 500, voucherRef: "2" }] } } },
  } });
  const returnTo = `/companies/${COMPANY_SLUG}/bilag?year=2026&q=Leverand%C3%B8r&status=booked&type=purchase_sale&party=all&sort=amount&dir=asc&page=1&pageSize=25`;
  const query = new URLSearchParams({ year: "2026", returnTo });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag/1?${query}`);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Bilag DOC-2026-000001");
  await expect(page.getByText("Køb af kontorartikler", { exact: true })).toBeVisible();
  await expect(page.getByText("Syntetisk supplerende postering", { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Åbn bilag", exact: true })).toHaveAttribute("href", "/api/companies/acme-aps/documents/1/file");
  const back = page.getByRole("link", { name: "Tilbage til bilag", exact: true });
  await expect(back).toHaveAttribute("href", returnTo);
  await back.click();
  await expect(page).toHaveURL(new URL(returnTo, page.url()).href);
  await expect(page.getByRole("searchbox", { name: "Søg", exact: true })).toHaveValue("Leverandør");
  await expect(page.getByRole("link", { name: "DOC-2026-000001", exact: true })).toHaveCount(1);
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("successful invoice issue shows the server receipt and leaves cleanly with list context", async ({ page }) => {
  let issued = false;
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/invoices": async () => ({ body: { ok: true, invoices: { ...core.invoices, invoices: issued ? [...core.invoices.invoices, { ...core.invoices.invoices[1], documentId: 3, invoiceNo: "2026-00003", customerName: "Kunde A/S", grossAmount: 7500, openBalance: 7500, status: "open", overdueDays: 0 }] : core.invoices.invoices } } }),
    "POST /api/companies/acme-aps/invoices/issue": async () => {
      issued = true;
      return { status: 201, body: { ok: true, invoice: { documentId: 3, invoiceNumber: "2026-00003", netAmount: 6000, vatRate: 0.25, vatAmount: 1500, grossAmount: 7500, lines: [{ description: "Syntetisk rådgivning", quantity: 3, unitPriceExVat: 2000, lineTotalExVat: 6000 }] } } };
    },
  } });
  const returnTo = `/companies/${COMPANY_SLUG}/fakturaer?year=2026&q=Kunde&status=open&pageSize=25`;
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/ny?${new URLSearchParams({ year: "2026", returnTo })}`);
  await fillInvoice(page);
  await page.getByRole("button", { name: "Udsted faktura", exact: true }).click();
  await expect(page.getByRole("main")).toContainText(/2026-00003.*udstedt/s);
  await expect(page.getByRole("main")).toContainText("7.500,00");
  await expect(page.getByRole("link", { name: "Hent PDF", exact: true })).toHaveAttribute("href", "/api/companies/acme-aps/invoices/3/pdf");
  await page.getByRole("button", { name: "Luk", exact: true }).click();
  await expect(page).toHaveURL(new URL(returnTo, page.url()).href);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("link", { name: "2026-00003", exact: true })).toBeVisible();
  const writes = fixture.calls.filter((call) => call.method !== "GET");
  expect(writes).toHaveLength(1);
  expect(writes[0].body).toMatchObject({ lines: [{ description: "Syntetisk rådgivning", quantity: 3, unitPriceExVat: 2000 }], buyer: { name: "Kunde A/S", vatOrCvr: "DK87654321" } });
  fixture.assertComplete();
});

test("successful document booking shows a receipt and returns without discarding confirmed work", async ({ page }) => {
  let booked = false;
  const voucher = { ...core.documents.documents[0], hasFile: false, journalEntryId: null, journalEntryNo: null, journalEntryText: null, journalEntryTotal: null };
  const fixture = await mockApi(page, { overrides: {
    ...bookingReadFixtures(voucher),
    "GET /api/companies/acme-aps/documents": async () => ({ body: { ok: true, documents: { ...core.documents, documents: [{ ...voucher, ...(booked ? { journalEntryId: 42, journalEntryNo: "B-2026-0042", journalEntryText: "Syntetisk kontorkøb", journalEntryTotal: 1250 } : {}) }], linkedCount: booked ? 1 : 0, unlinkedCount: booked ? 0 : 1 } } }),
    "POST /api/companies/acme-aps/documents/book-expense": async () => {
      booked = true;
      return { status: 201, body: { ok: true, booking: { entryId: 42, documentId: 1, bankTransactionId: 1, grossAmount: 1250, netAmount: 1000, vatAmount: 250, vatTreatment: "standard", grossAmountForeign: null, grossAmountDkk: 1250, netAmountDkk: 1000, vatAmountDkk: 250, fxRateToDkk: null } } };
    },
  } });
  const returnTo = `/companies/${COMPANY_SLUG}/bilag?year=2026&q=Leverand%C3%B8r&status=booked&pageSize=25`;
  await page.goto(`/companies/${COMPANY_SLUG}/bilag/1/bogfoer?${new URLSearchParams({ year: "2026", returnTo })}`);
  await page.getByRole("combobox", { name: "Udgiftskonto", exact: true }).selectOption("3000");
  await page.getByRole("button", { name: "Bogfør", exact: true }).click();
  await expect(page.getByRole("main")).toContainText("Bilaget blev bogført som journalpost 42");
  await expect(page.getByRole("main")).toContainText("1.250,00");
  await page.getByRole("button", { name: "Luk", exact: true }).click();
  await expect(page).toHaveURL(new URL(returnTo, page.url()).href);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("main")).toContainText("B-2026-0042");
  expect(fixture.calls.filter((call) => call.method !== "GET")).toHaveLength(1);
  fixture.assertComplete();
});

test("an uncertain settlement checks status with a read and never repeats the posting request", async ({ page }) => {
  let uncertain = false;
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/invoices": async () => ({ body: { ok: true, invoices: { ...core.invoices, invoices: core.invoices.invoices.map((invoice) => uncertain && invoice.documentId === 2 ? { ...invoice, status: "paid", openBalance: 0, overdueDays: 0 } : invoice) } } }),
    "POST /api/companies/acme-aps/invoices/settle": async () => {
      uncertain = true;
      return { status: 500, body: { ok: false, code: "internal", errors: ["Syntetisk afbrudt svar efter behandling."] } };
    },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer/2?year=2026`);
  await page.getByRole("button", { name: "Afstem", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Afstem faktura mod bankbetaling" });
  await dialog.getByRole("textbox", { name: "Bankreference", exact: true }).fill("SYNTHETIC-PAY-2");
  await dialog.getByRole("button", { name: "Afstem faktura", exact: true }).click();
  await expect(dialog).toContainText("Serverens resultat kunne ikke bekræftes");
  await expect(dialog.getByRole("button", { name: "Afstem faktura", exact: true })).toBeDisabled();
  const readsBefore = fixture.calls.filter((call) => call.method === "GET" && call.path.endsWith("/invoices")).length;
  const refreshed = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname.endsWith("/invoices"));
  await dialog.getByRole("button", { name: "Kontrollér status", exact: true }).click();
  await refreshed;
  await expect(dialog.getByRole("button", { name: "Afstem faktura", exact: true })).toBeDisabled();
  await dialog.getByRole("button", { name: "Annullér", exact: true }).click();
  await expect(page.getByRole("main")).toContainText("Betalt");
  await expect(page.getByRole("button", { name: "Afstem", exact: true })).toHaveCount(0);
  expect(fixture.calls.filter((call) => call.method === "GET" && call.path.endsWith("/invoices"))).toHaveLength(readsBefore + 1);
  const writes = fixture.calls.filter((call) => call.method !== "GET");
  expect(writes).toHaveLength(1);
  expect(writes[0]).toMatchObject({ path: "/api/companies/acme-aps/invoices/settle", body: { invoiceDocumentId: 2, bankTransactionReference: "SYNTHETIC-PAY-2", confirm: true } });
  fixture.assertComplete();
});

test("compiled StyleX CSS supplies the actual layout, font, control sizes and token colors", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Bilag");
  const layout = await page.getByRole("main").evaluate((main) => {
    const shell = getComputedStyle(main.parentElement!);
    const body = getComputedStyle(document.body);
    return { display: shell.display, columns: shell.gridTemplateColumns.split(/\s+/), background: body.backgroundColor, ink: body.color };
  });
  expect(layout.display).toBe("grid");
  expect(layout.columns[0]).toBe("248px");
  expect(layout.background).toBe("rgb(244, 241, 235)");
  expect(layout.ink).toBe("rgb(27, 26, 23)");
  const headline = await page.getByRole("heading", { level: 1 }).evaluate((element) => { const style = getComputedStyle(element); return { family: style.fontFamily, size: style.fontSize }; });
  expect(headline.family).toContain("Source Serif 4");
  expect(headline.size).toBe("32px");
  const primary = page.getByRole("button", { name: "Indlæs bilag", exact: true });
  const button = await primary.evaluate((element) => { const style = getComputedStyle(element); return { minHeight: style.minHeight, height: element.getBoundingClientRect().height, background: style.backgroundColor, ink: style.color }; });
  expect(button.minHeight).toBe("44px");
  expect(button.height).toBeGreaterThanOrEqual(44);
  expect(button.background).toBe("rgb(27, 26, 23)");
  expect(button.ink).toBe("rgb(244, 241, 235)");
  const input = await page.getByRole("searchbox", { name: "Søg", exact: true }).evaluate((element) => ({ minHeight: getComputedStyle(element).minHeight, height: element.getBoundingClientRect().height, border: getComputedStyle(element).borderTopColor }));
  expect(input.border).toBe("rgb(143, 136, 125)");
  expect(input.minHeight).toBe("44px");
  expect(input.height).toBeGreaterThanOrEqual(44);
  fixture.assertComplete();
});


test("mobile modal navigation includes closed disclosures in the keyboard cycle", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/kontakter?year=2026`);
  const trigger = page.getByRole("button", { name: "Menu", exact: true });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "Virksomhedsnavigation" });
  await expect(trigger).toHaveAttribute("aria-controls", (await dialog.getAttribute("id"))!);
  await expect(trigger).toHaveAttribute("aria-expanded", "true");
  await expect(dialog.getByRole("button", { name: "Luk menu" })).toBeFocused();
  expect(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  await page.keyboard.press("Tab");
  await expect(dialog.locator("summary").filter({ hasText: "Overblik" })).toBeFocused();
  await page.keyboard.press("Tab");
  const bookkeeping = dialog.locator("summary").filter({ hasText: "Bogføring" });
  await expect(bookkeeping).toBeFocused();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  const close = dialog.getByRole("button", { name: "Luk menu" });
  await dialog.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  const closeBox = await close.boundingBox();
  expect(closeBox?.y).toBeGreaterThanOrEqual(0);
  expect((closeBox?.y ?? 900) + (closeBox?.height ?? 900)).toBeLessThan(844);
  const documents = dialog.getByRole("link", { name: "Bilag", exact: true });
  await expect(documents).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(bookkeeping).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/bilag\?year=2026/);
  await expect(dialog).toHaveCount(0);
  fixture.assertComplete();
});

test("account dialogs keep keyboard focus, identify their triggers, and erase dismissed passwords", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  const fixture = await mockApi(page, { profile: "hosted" });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  const sessions = page.getByRole("button", { name: "Sessioner", exact: true });
  await expect(sessions).toHaveAttribute("aria-expanded", "false");
  await sessions.click();
  const sessionsDialog = page.getByRole("dialog", { name: "Aktive sessioner" });
  await expect(sessionsDialog).toBeVisible();
  await expect(sessions).toHaveAttribute("aria-controls", (await sessionsDialog.getAttribute("id"))!);
  await expect(sessions).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("Escape");
  await expect(sessions).toBeFocused();
  const password = page.getByRole("button", { name: "Skift adgangskode", exact: true }).and(page.locator("[aria-haspopup=dialog]"));
  await password.click();
  const passwordDialog = page.getByRole("dialog", { name: "Skift adgangskode" });
  await expect(passwordDialog).toBeVisible();
  await expect(page.getByLabel("Nuværende adgangskode", { exact: true })).toBeFocused();
  await expect(password).toHaveAttribute("aria-controls", (await passwordDialog.getAttribute("id"))!);
  await page.getByLabel("Nuværende adgangskode", { exact: true }).fill("ephemeral-synthetic-secret");
  for (let index = 0; index < 7; index++) {
    await page.keyboard.press("Tab");
    expect(await passwordDialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"]).analyze();
  expect(results.violations.map(({ id, nodes }) => ({ id, targets: nodes.map((node) => node.target) }))).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(password).toBeFocused();
  await expect(password).toHaveAttribute("aria-expanded", "false");
  await password.click();
  await expect(page.getByLabel("Nuværende adgangskode", { exact: true })).toHaveValue("");
  expect(fixture.calls.filter((call) => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
});

test("P&L chart exposes the same zero and negative values in an accessible currency table", async ({ page }) => {
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/overview": { body: { ok: true, overview: { ...core.overview, company: { ...core.overview.company, currency: "EUR" }, profitAndLoss: { ...core.overview.profitAndLoss, months: [{ month: 1, label: "Januar", income: 0, expense: -12.5 }] } } } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}?year=2026`);
  await expect(page.getByRole("img", { name: /Månedlige indtægter og udgifter i EUR/ })).toBeVisible();
  const disclosure = page.locator("summary").filter({ hasText: "Se indtægter og udgifter som tabel" });
  await disclosure.focus();
  await page.keyboard.press("Enter");
  const table = page.getByRole("table", { name: "Månedlige indtægter og udgifter (EUR)" });
  await expect(table).toBeVisible();
  const row = table.getByRole("row").filter({ hasText: "Januar" });
  await expect(row.getByRole("cell")).toHaveText(["Januar", "0,00 EUR", "-12,50 EUR"]);
  fixture.assertComplete();
});

test("cashflow graph's data table distinguishes absent, zero and negative bank balances", async ({ page }) => {
  const fixture = await mockApi(page, { overrides: {
    "GET /api/companies/acme-aps/cashflow": { body: { ok: true, cashflow: { ...core.cashflow, balanceSeries: [{ date: "2026-02-01", balance: 0 }, { date: "2026-03-01", balance: -50 }], closingBalance: -50 } } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/likviditet?year=2026`);
  const graph = page.getByRole("img", { name: /Månedlige indbetalinger, udbetalinger og banksaldo/ });
  await expect(graph).toHaveAttribute("aria-details", "cashflow-months");
  const table = page.getByRole("table", { name: "Pengestrøm og banksaldo pr. måned (DKK)" });
  await expect(table.getByRole("columnheader", { name: "Banksaldo ultimo" })).toBeVisible();
  const rows = table.getByRole("row");
  await expect(rows.nth(1).getByRole("cell").last()).toHaveText("—");
  await expect(rows.nth(2).getByRole("cell").last()).toHaveText(/0,00/);
  await expect(rows.nth(3).getByRole("cell").last()).toHaveText(/-50,00/);
  await expect(rows.last().getByRole("cell").last()).toHaveText(/-50,00/);
  fixture.assertComplete();
});


test("multi-year canvases identify their structured tables and partial-year comparisons", async ({ page }) => {
  const fixture = await mockApi(page);
  await page.goto(`/companies/${COMPANY_SLUG}/fleraar?year=2026`);
  await expect(page.getByRole("img", { name: /Omsætning, udgifter og resultat pr. regnskabsår/ })).toHaveAttribute("aria-details", "multiyear-result");
  await expect(page.getByRole("img", { name: /Balancesum og egenkapital pr. regnskabsår/ })).toHaveAttribute("aria-details", "multiyear-balance");
  for (const name of ["Omsætning, udgifter og resultat pr. regnskabsår (DKK)", "Balancesum og egenkapital pr. regnskabsår (DKK)"]) {
    const table = page.getByRole("table", { name, exact: true });
    await expect(table).toBeVisible();
    const current = table.getByRole("row").filter({ hasText: "2026" });
    await expect(current).toContainText("år til dato");
    await expect(current).toContainText("ej sammenligneligt");
  }
  fixture.assertComplete();
});


test("charts redraw with their local fonts after a delayed first visit", async ({ page }) => {
  await page.addInitScript(() => {
    const probe = window;
    probe.chartFontDraws = [];
    const original: (text: string, x: number, y: number, maxWidth?: number) => void = CanvasRenderingContext2D.prototype.fillText;
    CanvasRenderingContext2D.prototype.fillText = function (text, x, y, maxWidth) {
      if (this.font.includes("IBM Plex Mono")) probe.chartFontDraws?.push(Array.from(document.fonts).some((face) => face.family.includes("IBM Plex Mono") && face.weight === "400" && face.status === "loaded"));
      if (maxWidth === undefined) original.call(this, text, x, y);
      else original.call(this, text, x, y, maxWidth);
    };
  });
  let releaseFonts: () => void = () => {};
  const fonts = new Promise<void>((resolve) => { releaseFonts = resolve; });
  // Bun embeds these fonts in CSS. Give their original bytes local URLs so
  // both engines experience a real delayed font load, including WebKit's
  // otherwise synchronous decoding of embedded fonts.
  const fontFiles = new Map<string, Buffer>();
  await page.route("**/*.css", async (route) => {
    const response = await route.fetch();
    const css = await response.text();
    const body = css.replace(/url\(data:font\/(woff2?);base64,([^)]+)\)/g, (_, format: string, base64: string) => {
      const path = `/__test_fonts/${fontFiles.size}.${format}`;
      fontFiles.set(path, Buffer.from(base64, "base64"));
      return `url("${path}")`;
    });
    await route.fulfill({ response, body });
  });
  await page.route("**/__test_fonts/*", async (route) => {
    const bytes = fontFiles.get(new URL(route.request().url()).pathname);
    if (!bytes) throw new Error("Missing original font bytes");
    await fonts;
    await route.fulfill({ contentType: route.request().url().endsWith(".woff2") ? "font/woff2" : "font/woff", body: bytes });
  });
  const fixture = await mockApi(page);
  try {
    await page.goto(`/companies/${COMPANY_SLUG}?year=2026`, { waitUntil: "domcontentloaded" });
    await expect(page.getByRole("img", { name: /Månedlige indtægter og udgifter/ })).toBeVisible();
    await expect.poll(() => page.evaluate(() => window.chartFontDraws?.includes(false))).toBe(true);
    releaseFonts();
    await expect.poll(() => page.evaluate(() => window.chartFontDraws?.at(-1))).toBe(true);
    fixture.assertComplete();
  } finally { releaseFonts(); }
});
