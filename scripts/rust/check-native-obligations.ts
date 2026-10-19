import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-obligations.json";
/** Isolated synthetic liability; no role remapping, tax posting, or payment mutation. */
export async function checkNativeObligations(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const account = "7291",
    label = "Synthetic obligations liability";
  const read = async (path: string) => {
    const r = await page.request.get(`${url}/${path}`);
    if (!r.ok()) throw new Error(`Obligations ${path}: ${await r.text()}`);
    return r.json();
  };
  let evidence: { year: string; row: unknown; total: number; asOf: string };
  if (!restarted) {
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const [no, name, type] of [
      [account, label, "liability"],
      ["8791", "Synthetic obligations equity", "equity"],
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(name);
      await form.getByLabel(/^Kontotype/).selectOption(type);
      await form.getByLabel(/^Normal saldo/).selectOption("credit");
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("row").filter({ hasText: name }).waitFor();
    }
    const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
    const posting = JSON.parse(state.postedBody);
    const date = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
    posting.input.request = {
      transactionDate: date,
      text: "Synthetic obligations exact debt",
      currency: "DKK",
      lines: [
        { accountNo: "8791", debitAmount: "6.37", creditAmount: "0.00" },
        { accountNo: account, debitAmount: "0.00", creditAmount: "6.37" },
      ],
    };
    posting.input.idempotencyKey = "synthetic-obligations-exact-debt";
    const r = await page.request.post(`${url}/manual-journal`, { headers, data: posting });
    if (!r.ok()) throw new Error(`Synthetic obligation post: ${await r.text()}`);
    const value = (await read("obligations")).obligations;
    const row = value.obligations.find((r: { accountNo: string }) => r.accountNo === account);
    if (!row || row.amount !== 6.37 || row.dueDate !== null)
      throw new Error("Exact unguessed liability missing");
    evidence = { year: value.selectedYear, row, total: value.totalOwed, asOf: value.asOfDate };
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence, null, 2));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const query = `?year=${encodeURIComponent(evidence.year)}`;
  const before = JSON.stringify((await read(`journal${query}`)).journal.entries);
  const value = (await read(`obligations${query}`)).obligations;
  const row = value.obligations.find((r: { accountNo: string }) => r.accountNo === account);
  if (JSON.stringify(row) !== JSON.stringify(evidence.row))
    throw new Error("Liability projection changed after restart");
  // Other journeys can append debt; only own account, not a global total, is a restart invariant.
  if (value.coverage.complete !== false || !value.coverage.notes.length)
    throw new Error("Missing honest coverage");
  const annual = value.obligations.find((r: { kind: string }) => r.kind === "annual-report");
  if (!annual || annual.amount !== 0 || annual.dueDate !== null)
    throw new Error("Unknown reporting class invented a debt/deadline");
  const invalid = await page.request.get(`${url}/obligations?account=7291`);
  if (invalid.status() !== 400) throw new Error("Unsupported obligations selector accepted");
  const other = await page.request.get(`${base}/api/companies/not-selected/obligations`);
  if (other.status() !== 404) throw new Error("Other company obligation leaked");
  await page.goto(`${base}/companies/synthetic/forpligtelser${query}`);
  await page.getByRole("heading", { name: "Forpligtelser", exact: true }).waitFor();
  await page.getByRole("heading", { name: "Grundlag og begrænsninger", exact: true }).waitFor();
  const uiRow = page.getByRole("row").filter({ hasText: label });
  await uiRow.getByText("Ingen frist", { exact: true }).waitFor();
  if (!(await uiRow.innerText()).includes("6,37")) throw new Error("UI lost exact cents");
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: base });
  await uiRow.getByRole("button", { name: /Kopiér beløb/ }).click();
  await uiRow.getByRole("status").getByText("Beløb kopieret", { exact: true }).waitFor();
  if ((await page.evaluate(() => navigator.clipboard.readText())) !== "6,37")
    throw new Error("Wrong copied amount");
  const annualRow = page.getByRole("row").filter({ hasText: annual.label });
  await annualRow.getByRole("link", { name: "Indberet på virk.dk", exact: true }).waitFor();
  if (await annualRow.getByRole("button").count())
    throw new Error("Annual filing offered money action");
  const vat = value.obligations.find((r: { kind: string }) => r.kind === "vat");
  if (vat) {
    const vatRow = page.getByRole("row").filter({ hasText: vat.label });
    await vatRow.getByText(/^Momsgrundlag /).click();
    await vatRow.getByText("Momstilsvar før betalinger", { exact: true }).waitFor();
    await vatRow.getByRole("link", { name: "SKAT-rubrikker", exact: true }).waitFor();
  }
  if (JSON.stringify((await read(`journal${query}`)).journal.entries) !== before)
    throw new Error("Obligations read/actions mutated ledger");
  console.log(`Browser: native obligations ${restarted ? "restart" : "initial"} verified`);
}
