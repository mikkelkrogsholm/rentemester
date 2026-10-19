import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import type { CompanyMultiYear, MultiYearRow } from "../../app/src/lib/types/statements";
const saved = "/tmp/rentemester-rust-reports/native-multi-year.json";
type Evidence = { rows: MultiYearRow[]; selected: string; entryIds: number[] };
// Run before dashboard/accountant baselines: two original documents and old-year postings are
// added through the public APIs. Their effect nets to zero before prior/current fiscal years.
export async function checkNativeMultiYear(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`, headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) {
    const response = await page.request.get(`${url}/${path}`);
    if (!response.ok()) throw new Error(`Comparison dependency ${path}: ${await response.text()}`);
    return response.json();
  }
  async function read(): Promise<CompanyMultiYear> { return (await get("multi-year")).multiYear; }
  let evidence: Evidence;
  if (!restarted) {
    const journal = (await get("journal")).journal;
    const selected = encodeURIComponent(journal.selectedYear);
    const beforeIncome = (await get(`income-statement?year=${selected}`)).incomeStatement;
    const beforeBalance = (await get(`balance?year=${selected}`)).balance;
    const options = journal.nativePosting as { ruleVersion: string; accounts: Array<{ accountNo: string; type: string; active: boolean; allowDirectPosting: boolean }> };
    const asset = options.accounts.find(a => a.type === "asset" && a.active && a.allowDirectPosting);
    const income = options.accounts.find(a => a.type === "income" && a.active && a.allowDirectPosting);
    if (!asset || !income) throw new Error("Synthetic comparison needs existing postable asset/income accounts");
    const currentStart = journal.periodStart as string;
    if (!/^\d{4}-\d{2}-01$/.test(currentStart)) throw new Error("Missing native fiscal-year start");
    const dates = [4, 3].map(back => `${Number(currentStart.slice(0, 4)) - back}${currentStart.slice(4)}`);
    const entryIds: number[] = [];
    for (let i = 0; i < dates.length; i++) {
      const text = `Synthetic multi-year ${i === 0 ? "income" : "income correction"}`;
      const file = Buffer.from(`${text}\nDate: ${dates[i]}\nDKK 100.01\nControlled comparison; corrected next fiscal year.\n`);
      const uploaded = await page.request.post(`${url}/documents/ingest`, { headers, data: { confirm: true, fileName: `synthetic-multi-year-${i}.txt`, fileBase64: file.toString("base64"), metadata: { source: text, documentType: "cash_register_receipt", issueDate: dates[i] } } });
      if (!uploaded.ok()) throw new Error(`Historical original: ${await uploaded.text()}`);
      const documentId = String((await uploaded.json()).document.id);
      const posted = await page.request.post(`${url}/manual-journal`, { headers, data: { confirm: true, input: {
        format: "rentemester-manual-journal-post/1", expectedRuleVersion: options.ruleVersion, idempotencyKey: randomUUID(),
        request: { transactionDate: dates[i], text, currency: journal.company.currency, documentId, lines: [
          { accountNo: i === 0 ? asset.accountNo : income.accountNo, debitAmount: "100.01" },
          { accountNo: i === 0 ? income.accountNo : asset.accountNo, creditAmount: "100.01" },
        ] },
      } } });
      if (!posted.ok()) throw new Error(`Historical posting: ${await posted.text()}`);
      const row = (await read()).years.find(y => y.periodStart === dates[i]);
      if (!row) throw new Error("Legitimately posted historical fiscal year missing");
      const past = (await get(`journal?year=${encodeURIComponent(row.year)}`)).journal;
      const entry = past.entries.find((e: { text: string }) => e.text === text);
      if (!entry) throw new Error("Historical committed posting missing");
      entryIds.push(entry.id);
    }
    for (const [before, after] of [
      [journal.entries, (await get(`journal?year=${selected}`)).journal.entries],
      [[beforeIncome.totalIncome, beforeIncome.totalExpense, beforeIncome.result], await get(`income-statement?year=${selected}`).then(r => [r.incomeStatement.totalIncome, r.incomeStatement.totalExpense, r.incomeStatement.result])],
      [[beforeBalance.totalAssets, beforeBalance.equity.total, beforeBalance.balanced], await get(`balance?year=${selected}`).then(r => [r.balance.totalAssets, r.balance.equity.total, r.balance.balanced])],
    ]) if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error("Neutral historical fixture changed current journal/reports");
    const comparison = await read();
    const history = dates.map(date => comparison.years.find(y => y.periodStart === date)!);
    if (history.some(y => !y) || history[0].omsaetning < 100.01 || history[1].omsaetning > -100.01) throw new Error("Historical comparison sign lost");
    evidence = { rows: comparison.years, selected: history[0].year, entryIds };
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence));
    if ((await page.request.get(`${url}/multi-year?year=2026`)).status() !== 400) throw new Error("Unsupported comparison query accepted");
    if ((await page.request.get(`${base}/api/companies/unselected/multi-year`)).status() !== 404) throw new Error("Comparison crossed company boundary");
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const comparison = await read();
  if (JSON.stringify(comparison.years) !== JSON.stringify(evidence.rows)) throw new Error("Comparison changed after restart");
  for (const row of comparison.years.filter(y => y.source === "live")) {
    const query = `?year=${encodeURIComponent(row.year)}`;
    const income = (await get(`income-statement${query}`)).incomeStatement;
    const balance = (await get(`balance${query}`)).balance;
    for (const [actual, expected] of [[row.omsaetning, income.totalIncome], [row.udgifter, income.totalExpense], [row.resultat, income.result], [row.balancesum, balance.totalAssets], [row.egenkapital, balance.equity.total]]) {
      if (actual !== expected) throw new Error(`Comparison differs from financial reports for ${row.year}`);
    }
  }
  await page.goto(`${base}/companies/synthetic/fleraar?year=${encodeURIComponent(evidence.selected)}`);
  await page.getByRole("heading", { name: "Flerårsoverblik", exact: true }).waitFor();
  if (await page.getByRole("combobox", { name: "Vælg regnskabsår" }).inputValue() !== evidence.selected) throw new Error("Comparison discarded selected year");
  const historicalRow = page.locator("#multiyear-result").getByRole("row").filter({ has: page.getByRole("cell", { name: evidence.selected, exact: true }) });
  await historicalRow.waitFor();
  if ((await historicalRow.innerText()).includes("år til dato")) throw new Error("Historical year marked partial");
  await page.locator("#multiyear-balance").waitFor();
  if (await page.locator("canvas").count() < 2) throw new Error("Comparison charts missing");
  await page.screenshot({ path: "/tmp/rentemester-rust-multi-year.png", fullPage: true });
  const link = page.locator("section.statement").getByRole("link", { name: "Resultatopgørelse", exact: true });
  const href = await link.getAttribute("href");
  if (!href || !href.includes(`year=${encodeURIComponent(evidence.selected)}`)) throw new Error("Report navigation lost fiscal year");
  await link.click();
  await page.getByRole("heading", { name: "Resultatopgørelse", exact: true }).waitFor();
  console.log(restarted ? "PASS: multi-year figures persist and match native income/balance reports after restart" : "PASS: comparison tables/charts, signed historical income, balance/report parity and fiscal navigation; current ledger preserved");
}
