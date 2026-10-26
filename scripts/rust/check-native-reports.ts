import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

/** Navigate existing report views and exercise their real download links. */
export async function checkNativeReports(page: Page, base: string) {
  const directory = process.env.RM_NATIVE_REPORT_DIR ?? "/tmp/rentemester-rust-reports";
  mkdirSync(directory, { recursive: true });
  const cases = [
    { route: "resultatopgorelse", api: "income-statement", title: "Resultatopgørelse" },
    { route: "balance", api: "balance", title: "Balance" },
    { route: "saldobalance", api: "trial-balance", title: "Saldobalance" },
  ];
  for (const report of cases) {
    await page.getByRole("link", { name: report.title, exact: true }).click();
    await page.getByRole("heading", { name: report.title, exact: true }).waitFor();
    const payload = await (await page.request.get(`${base}/api/companies/synthetic/${report.api}`)).json();
    const value = payload[report.api === "income-statement" ? "incomeStatement" : report.api === "balance" ? "balance" : "trialBalance"];
    if (report.api === "income-statement" && (value.result !== 0 || value.priorResult !== null || value.comparison?.reason !== "prior_native_year_not_present")) throw new Error("Opening equity affected profit or absent prior-year evidence was fabricated");
    if (report.api === "balance" && (value.totalAssets !== 125.5 || value.equity.total !== 125.5 || value.balanced !== true || value.assets.priorTotal !== null)) throw new Error("Balance or missing prior comparison is incorrect");
    const section = page.getByRole("main");
    for (const format of ["csv", "pdf"]) {
      const [download] = await Promise.all([page.waitForEvent("download"), section.getByRole("link", { name: format === "csv" ? "Hent CSV" : "Hent PDF", exact: true }).click()]);
      if (await download.failure()) throw new Error("Native report download failed");
      const path = `${directory}/${download.suggestedFilename()}`;
      await download.saveAs(path);
      const bytes = readFileSync(path);
      if (format === "csv") {
        const csv = bytes.toString("utf8");
        if (!csv.startsWith("\uFEFFsep=;\r\n") || !csv.includes(report.title) || !csv.includes("Synthetic Rust Web")) throw new Error("CSV metadata or encoding changed");
        if (report.api !== "income-statement" && !csv.includes("125,50")) throw new Error("CSV lost the posted amount");
      } else {
        if (!bytes.subarray(0, 8).toString().startsWith("%PDF-1.4")) throw new Error("Download was not a PDF");
        const text = execFileSync("pdftotext", ["-layout", path, "-"], { encoding: "utf8" });
        if (!text.includes(report.title) || !text.includes("Synthetic Rust Web")) throw new Error("PDF text did not decode correctly");
        if (report.api !== "income-statement" && !text.includes("125,50")) throw new Error("PDF lost the posted amount");
        const repeated = await page.request.get(`${base}/api/companies/synthetic/${report.api}/export?format=pdf&year=${encodeURIComponent(value.selectedYear)}`);
        if (!bytes.equals(await repeated.body())) throw new Error("PDF output was not deterministic");
      }
    }
  }
  const journal = await page.request.get(`${base}/api/companies/synthetic/journal/export?format=csv&account=1800`);
  const csv = await journal.text();
  if (!journal.ok() || !csv.includes("Kontofilter;1800") || !csv.includes("Synthetic opening transfer")) throw new Error("Filtered journal CSV failed");
  await page.goto(`${base}/companies/synthetic/saldobalance`);
  await page.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
  console.log("PASS: native income statement, balance, trial balance and real CSV/PDF downloads; parsed PDF text and deterministic bytes; filtered journal CSV");
}
