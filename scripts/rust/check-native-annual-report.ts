import type { Page } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
const saved = "/tmp/rentemester-rust-reports/native-annual-report.json";

/** Owned synthetic workspace only. Uses normal MFA browser authority and existing reviewed close. */
export async function checkNativeAnnualReport(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const start = "2020-01-01", end = "2020-12-31";
  const endpoint = `${url}/annual-report?fiscalYearStart=${start}&fiscalYearEnd=${end}`;
  async function get(path: string) {
    const response = await page.request.get(path);
    if (!response.ok()) throw new Error(`Annual read failed: ${response.status()} ${await response.text()}`);
    return response.json();
  }
  async function ledger() {
    const years = (await get(`${url}/fiscal-years`)).fiscalYears.years;
    const entries = [];
    for (const year of years) {
      const journal = (await get(`${url}/journal?year=${encodeURIComponent(year.label)}`)).journal;
      entries.push({year: year.label, ledgerUuid: journal.ledgerUuid, entries: journal.entries});
    }
    return JSON.stringify(entries);
  }
  async function buildUi(ok: boolean) {
    await page.goto(`${base}/companies/synthetic/aarsrapport`);
    await page.getByLabel("Start (YYYY-MM-DD)", { exact: true }).fill(start);
    await page.getByLabel("Slut (YYYY-MM-DD)", { exact: true }).fill(end);
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url() === endpoint && r.request().method() === "GET"),
      page.getByRole("button", { name: "Byg årsrapport", exact: true }).click(),
    ]);
    const data = (await response.json()).annualReport;
    if (!response.ok() || data.report.ok !== ok) throw new Error(`Annual UI outcome wrong: ${JSON.stringify(data)}`);
    await page.getByRole("heading", { name: ok ? "Ledelsespåtegning" : "Forudsætninger mangler", exact: true }).waitFor();
    return data;
  }
  async function cvrUi(cvr: string) {
    await page.goto(`${base}/companies/synthetic/manage`);
    const form = page.getByRole("form", { name: "Rediger stamdata", exact: true });
    await form.getByLabel("CVR-nummer", { exact: true }).fill(cvr);
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url() === `${url}/company` && r.request().method() === "PATCH"),
      form.getByRole("button", { name: "Gem stamdata", exact: true }).click(),
    ]);
    if (!response.ok()) throw new Error(`CVR update refused: ${await response.text()}`);
    await page.getByText("Stamdata opdateret.", { exact: true }).waitFor();
  }
  async function closeUi() {
    await page.goto(`${base}/companies/synthetic/periodelas`);
    await page.getByRole("button", { name: "Luk periode …", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Start (YYYY-MM-DD)", { exact: true }).fill(start);
    await dialog.getByLabel("Slut (YYYY-MM-DD)", { exact: true }).fill(end);
    await dialog.getByLabel("Type", { exact: true }).selectOption("fiscal_year");
    await dialog.getByRole("button", { name: "Kontrollér", exact: true }).click();
    await dialog.getByText(/Kontrolleret: 0 blokeringer/).waitFor();
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url() === `${url}/periods/close` && r.request().method() === "POST"),
      dialog.getByRole("button", { name: "Gem review og luk", exact: true }).click(),
    ]);
    if (!response.ok()) throw new Error(`Annual close refused: ${await response.text()}`);
    await dialog.waitFor({ state: "hidden" });
  }
  const before = await ledger();
  if (!restarted) {
    // A separate historical empty year leaves every existing harness posting period untouched.
    const initial = await buildUi(false);
    if (!initial.report.errors.some((e: string) => e.includes("ikke låst"))) throw new Error("Open year did not block annual preparation");
    const profile = (await get(`${url}/company`)).company;
    if (!profile.cvr) throw new Error("Annual helper requires the synthetic company's configured CVR");
    await cvrUi("");
    try {
      const missing = await buildUi(false);
      if (!missing.report.errors.some((e: string) => e.includes("CVR"))) throw new Error("Missing CVR did not block annual preparation");
    } finally { await cvrUi(profile.cvr); }
    await closeUi();
    await buildUi(true);
    await page.getByText(/Skabelon — udfyld dato/).waitFor();
    await page.getByText(/ansvarlig for indberetning til Erhvervsstyrelsen/).waitFor();
    await page.goto(`${base}/companies/synthetic/periodelas`);
    await page.getByRole("row").filter({ hasText: start }).getByRole("button", { name: "Genåbn …", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Begrundelse (påkrævet)", { exact: true }).fill("Synthetic annual preparation correction");
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url() === `${url}/periods/reopen` && r.request().method() === "POST"),
      dialog.getByRole("button", { name: "Genåbn periode", exact: true }).click(),
    ]);
    if (!response.ok()) throw new Error(`Annual reopen refused: ${await response.text()}`);
    await dialog.waitFor({ state: "hidden" });
    await buildUi(false);
    await closeUi();
    writeFileSync(saved, JSON.stringify(await buildUi(true)));
  } else if (JSON.stringify(await buildUi(true)) !== readFileSync(saved, "utf8")) {
    throw new Error("Annual preparation changed after restart");
  }
  // Compare a real observed live year's figures even when its still-open year blocks preparation.
  const years = (await get(`${url}/fiscal-years`)).fiscalYears.years;
  const year = years[years.length - 1];
  const annual = (await get(`${url}/annual-report?fiscalYearStart=${year.start}&fiscalYearEnd=${year.end}`)).annualReport.report;
  const income = (await get(`${url}/income-statement?year=${encodeURIComponent(year.label)}`)).incomeStatement;
  const balance = (await get(`${url}/balance?year=${encodeURIComponent(year.label)}`)).balance;
  if (annual.profitAndLoss.result !== income.result || annual.balanceSheet.totalAssets !== balance.totalAssets || annual.balanceSheet.equity.total !== balance.equity.total || annual.comparison.aretsResultat !== income.priorResult) throw new Error("Annual preparation differs from shared live statements");
  for (const query of ["", "?fiscalYearStart=2025-02-29&fiscalYearEnd=2025-12-31", "?fiscalYearStart=2025-01-01&fiscalYearStart=2025-01-01&fiscalYearEnd=2025-12-31", "?fiscalYearStart=2025-01-01&fiscalYearEnd=2025-12-31&year=2025"]) {
    if ((await page.request.get(`${url}/annual-report${query}`)).status() !== 400) throw new Error("Invalid annual selector accepted");
  }
  if ((await page.request.get(endpoint.replace("/synthetic/", "/missing/"))).ok()) throw new Error("Annual preparation leaked another company");
  const anonymous = await page.context().browser()!.newContext();
  try { if ((await anonymous.request.get(endpoint)).ok()) throw new Error("Anonymous annual report accepted"); }
  finally { await anonymous.close(); }
  if (await ledger() !== before) throw new Error("Annual preparation/period lifecycle changed journal entries");
  console.log(restarted ? "PASS: annual preparation and shared figures survive restart; journal unchanged" : "PASS: open year and missing CVR block → UI fixes CVR → reviewed fiscal close → annual placeholders → reopen blocks → reclose rebuild; shared figures and journal unchanged");
}
