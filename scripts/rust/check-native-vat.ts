import type { Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";

export async function checkNativeVat(page: Page, base: string, reopened = false) {
  await page.goto(`${base}/companies/synthetic/moms`);
  await page.getByRole("heading", { name: "Moms", exact: true }).waitFor().catch(async error => { console.error(await page.locator("body").innerText()); throw error; });
  const response = await page.request.get(`${base}/api/companies/synthetic/vat`);
  const result = await response.json();
  const v = result.vat;
  if (!response.ok() || v.inputVat !== 25 || v.outputVat !== 0 || v.outputVatAdjustment !== 0 || v.payable !== -25 || v.rubrikker.kobsmoms !== 25 || v.rubrikker.momstilsvar !== -25 || v.periodStatus !== "open" || v.momsangivelseReady || v.vatReportErrors.length) throw new Error(`Wrong VAT return: ${JSON.stringify(result)}`);
  await page.getByRole("row").filter({ hasText: "Købsmoms (indgående moms)" }).getByText("25,00 kr.", { exact: true }).waitFor();
  await page.getByText("SKAT-rubrikker (foreløbige — åben periode)", { exact: true }).waitFor();
  if (await page.getByRole("button", { name: "Kopier Købsmoms", exact: true }).isEnabled()) throw new Error("Open VAT period advertised ready to file");
  if (await page.getByRole("button", { name: "Luk momsperiode", exact: true }).count()) throw new Error("Unported period closure offered as working");
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("link", { name: "Hent PDF", exact: true }).click()]);
  mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
  const path = "/tmp/rentemester-rust-reports/native-vat.pdf";
  await download.saveAs(path);
  const text = execFileSync("pdftotext", ["-layout", path, "-"], { encoding: "utf8" });
  if (!text.includes("Købsmoms") || !text.includes("25,00") || !text.includes("Foreløbig") || !text.includes(v.periodStart) || !text.includes(v.deadline)) throw new Error(`Missing VAT PDF content: ${text}`);
  const repeated = await page.request.get(`${base}/api/companies/synthetic/vat/export?format=pdf`);
  if (!repeated.ok() || !readFileSync(path).equals(await repeated.body())) throw new Error("VAT PDF differs from the visible report snapshot");
  const anonymous = await page.context().browser()!.newContext();
  try { if ((await anonymous.request.get(`${base}/api/companies/synthetic/vat`)).status() !== 401) throw new Error("VAT report accessible without login"); }
  finally { await anonymous.close(); }
  await page.screenshot({ path: "/tmp/rentemester-rust-vat.png", fullPage: true });
  console.log(reopened ? "PASS: VAT report, provisional rubrics and exact PDF remain correct after Rust restart" : "PASS: booked VAT purchase → native VAT overview → 25 DKK input VAT / -25 payable → provisional rubrics → real PDF; no premature filing or anonymous read");
}
