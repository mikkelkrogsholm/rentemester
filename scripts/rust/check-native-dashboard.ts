import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { CompanyOverview } from "../../app/src/lib/types/dashboard";
type Overview = CompanyOverview & { asOfDate: string; periodStart: string; periodEnd: string };
const saved = "/tmp/rentemester-rust-reports/native-dashboard.json";
const cents = (n: number) => Math.round(n * 100);
export async function checkNativeDashboard(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  async function get(path: string) {
    const result = await page.request.get(`${url}/${path}`);
    if (!result.ok()) throw new Error(`Dashboard dependency ${path}: ${result.status()} ${await result.text()}`);
    return result.json();
  }
  const overview: Overview = (await get("overview")).overview;
  const suffix = `?year=${encodeURIComponent(overview.selectedYear)}`;
  // Sequential reads also prove ordinary navigation uses the same live reports.
  const income = (await get(`income-statement${suffix}`)).incomeStatement;
  const balance = (await get(`balance${suffix}`)).balance;
  const bank = (await get(`bank${suffix}`)).bank;
  const vat = (await get(`vat${suffix}`)).vat;
  const documents = (await get("documents")).documents;
  const payables = (await get(`payables?status=open&asOf=${overview.periodEnd}`)).payables;
  const journal = (await get(`journal${suffix}`)).journal;
  if (!journal.entries.length || income.totalExpense === 0) throw new Error("Run dashboard after the synthetic bookkeeping journeys so parity has meaningful live amounts");
  if (overview.profitAndLoss.omsaetning !== income.totalIncome || overview.profitAndLoss.udgifter !== income.totalExpense || overview.profitAndLoss.resultat !== income.result) throw new Error("Dashboard P&L disagrees with financial report");
  if (overview.profitAndLoss.months.reduce((n, m) => n + cents(m.income), 0) !== cents(income.totalIncome) || overview.profitAndLoss.months.reduce((n, m) => n + cents(m.expense), 0) !== cents(income.totalExpense)) throw new Error("Dashboard monthly chart disagrees with yearly P&L");
  if (overview.bank.balance !== bank.bookedBalance || overview.bank.actualBalance !== bank.actualBalance || overview.bank.difference !== bank.difference) throw new Error("Dashboard bank status disagrees with bank view");
  if (vat.vatRegistered) for (const field of ["outputVat", "inputVat", "payable", "periodStart", "periodEnd", "deadline", "periodStatus"] as const) {
    if (overview.vat?.[field] !== vat[field]) throw new Error(`Dashboard VAT ${field} disagrees with VAT report`);
  }
  if (overview.documents?.linkedCount !== documents.linkedCount || overview.documents.unlinkedCount !== documents.unlinkedCount || overview.payables?.openTotal !== payables.totalOpenBalance || overview.payables.openCount !== payables.count) throw new Error("Dashboard document/payable status disagrees with shared readers");
  if (overview.keyFigures.egenkapitalandel !== (balance.totalAssets === 0 ? null : balance.equity.total / balance.totalAssets)) throw new Error("Dashboard equity ratio double-counted result or disagrees with balance");
  if (overview.keyFigures.bruttomargin !== (income.totalIncome === 0 ? null : income.result / income.totalIncome)) throw new Error("Dashboard profit margin disagrees with P&L");
  const expectedRecent = journal.entries.slice(0, 8).map((e: { id: number; entryNo: string; date: string; text: string; total: number }) => ({ id: e.id, entryNo: e.entryNo, date: e.date, text: e.text, amount: e.total }));
  if (JSON.stringify(overview.recentEntries) !== JSON.stringify(expectedRecent)) {
    // API objects have stable fields but serializer key ordering need not match hand-written objects.
    for (let i = 0; i < expectedRecent.length; i++) for (const key of ["id", "entryNo", "date", "text", "amount"] as const) {
      if (overview.recentEntries[i]?.[key] !== expectedRecent[i][key]) throw new Error("Dashboard recent entries disagree with journal");
    }
  }
  let open = 0, count = 0;
  for (const year of overview.fiscalYears.filter(y => y.source === "live")) {
    const invoices = (await get(`invoices?year=${encodeURIComponent(year.label)}`)).invoices.invoices as Array<{ openBalance: number }>;
    for (const invoice of invoices) if (invoice.openBalance > 0) { open += cents(invoice.openBalance); count++; }
  }
  if (overview.receivables?.basis !== "current" || cents(overview.receivables.openTotal) !== open || overview.receivables.openCount !== count) throw new Error("Current all-year receivables disagree with shared invoice views");
  for (const path of ["overview?year=missing", "overview?year=2026&year=2025", "overview?asOf=2026-02-30"]) {
    const result = await page.request.get(`${url}/${path}`);
    if (![400, 422].includes(result.status())) throw new Error(`Invalid overview selection accepted: ${path}`);
  }
  if ((await page.request.get(`${base}/api/companies/unselected/overview`)).status() !== 404) throw new Error("Dashboard crossed selected company boundary");
  const evidence = { year: overview.selectedYear, pnl: overview.profitAndLoss, bank: overview.bank, receivables: overview.receivables, payables: overview.payables, documents: overview.documents, recentEntries: overview.recentEntries, keyFigures: overview.keyFigures };
  if (!restarted) { mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(saved, JSON.stringify(evidence)); }
  else if (JSON.stringify(evidence) !== readFileSync(saved, "utf8")) throw new Error("Dashboard live evidence changed after restart");
  await page.goto(`${base}/companies/synthetic`);
  await page.getByRole("heading", { name: "Overblik", exact: true, level: 1 }).waitFor();
  if (!new URL(page.url()).pathname.endsWith("/companies/synthetic")) throw new Error("Rust company overview still redirects to journal");
  await page.getByRole("img", { name: /Månedlige indtægter og udgifter/ }).waitFor();
  await page.getByText("Se indtægter og udgifter som tabel", { exact: true }).click();
  await page.getByRole("table", { name: /Månedlige indtægter og udgifter/ }).waitFor();
  for (const [title, segment] of [["Bank", "bank"], ["Moms", "moms"], ["Tilgodehavender", "fakturaer"], ["Leverandørgæld", "leverandoerfaktura"], ["Bilag", "bilag"]]) {
    if (title === "Moms" && !overview.vat) continue;
    const card = page.getByRole("heading", { name: title, exact: true }).locator("..");
    if (!(await card.getAttribute("href"))?.includes(`/companies/synthetic/${segment}?year=`)) throw new Error(`Dashboard ${title} drill-down lost company/year`);
  }
  const resultLink = page.getByRole("link").filter({ has: page.getByText("Resultat", { exact: true }) });
  await resultLink.click();
  await page.getByRole("heading", { name: "Resultatopgørelse", exact: true, level: 1 }).waitFor();
  await page.goto(`${base}/companies/synthetic`);
  await page.getByRole("heading", { name: "Overblik", exact: true, level: 1 }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-dashboard.png", fullPage: true });
  console.log(restarted ? "PASS: native company overview and live financial/status evidence persist after restart" : "PASS: native company opens Overblik; P&L/months, ratios, bank/VAT, all-year receivables, payables, documents and recent entries agree with shared live readers; chart and year-aware links work");
}
