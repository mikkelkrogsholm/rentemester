import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-bank-reconciliation.json";
const text = "Synthetic already posted journal bank receipt";
const note =
  "Synthetic reviewer checked the original statement and existing accounting as the same receipt.";
type Evidence = {
  year: string;
  bank: number;
  journal: number;
  entryNo: string;
  link: string;
  proof: string;
};
export async function checkNativeBankReconciliation(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`,
    headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) {
    const r = await page.request.get(`${url}/${path}`);
    if (!r.ok()) throw new Error(`Bank reconciliation ${path}: ${await r.text()}`);
    return r.json();
  }
  async function post(path: string, data: unknown) {
    const r = await page.request.post(`${url}/${path}`, { headers, data });
    if (!r.ok()) throw new Error(`Bank reconciliation ${path}: ${await r.text()}`);
    return r.json();
  }
  let evidence: Evidence;
  if (!restarted) {
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const [no, name, kind, normal] of [
      ["1728", "Synthetic existing-journal bank asset", "asset", "debit"],
      ["8728", "Synthetic existing-journal equity", "equity", "credit"],
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(name);
      await form.getByLabel(/^Kontotype/).selectOption(kind);
      await form.getByLabel(/^Normal saldo/).selectOption(normal);
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("row").filter({ hasText: name }).waitFor();
    }
    await post("bank-accounts", {
      name: "Synthetic existing-journal bank",
      slug: "synthetic-existing-journal",
      currency: "DKK",
      ledgerAccountNo: "1728",
    });
    const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
    const posting = JSON.parse(state.postedBody);
    const date = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
    posting.input.request = {
      transactionDate: date,
      text,
      currency: "DKK",
      lines: [
        { accountNo: "1728", debitAmount: "15.27", creditAmount: "0.00" },
        { accountNo: "8728", debitAmount: "0.00", creditAmount: "15.27" },
      ],
    };
    posting.input.idempotencyKey = "synthetic-existing-journal-bank-source";
    // Accounting deliberately exists before importing its statement row.
    await post("manual-journal", posting);
    await post("bank/import", {
      csvContent: `transaction_date,text,amount,currency,balance_after\n${date},${text},15.27,DKK,15.27\n`,
      account: "synthetic-existing-journal",
      confirm: true,
    });
    const bank = (await get("bank")).bank;
    const transaction = bank.transactions.find((t: { text: string }) => t.text === text);
    const journal = (await get(`journal?year=${encodeURIComponent(bank.selectedYear)}`)).journal;
    const entry = journal.entries.find((e: { text: string }) => e.text === text);
    if (!transaction || !entry || transaction.reconciliationStatus !== "unmatched")
      throw new Error("Dedicated unmatched bank/existing journal fixture missing");
    evidence = {
      year: bank.selectedYear,
      bank: transaction.id,
      journal: entry.id,
      entryNo: entry.entryNo,
      link: "",
      proof: "",
    };
    const before = JSON.stringify(journal.entries);
    const beforePayables = JSON.stringify((await get("payables?status=all")).payables.rows);
    const plans = (await get(`bank/reconciliation-candidates?bankTransactionId=${evidence.bank}`))
      .reconciliationCandidates.plans;
    const plan = plans.find(
      (p: { journalEntryId: number }) => p.journalEntryId === evidence.journal,
    );
    if (
      !plan ||
      plan.journalBankMovement !== 15.27 ||
      plan.bankAmountDkk !== 15.27 ||
      !/^[a-f0-9]{64}$/.test(plan.sourceHash)
    )
      throw new Error("Exact source-bound manual plan missing");
    const bad = await page.request.post(`${url}/bank/reconciliation-link`, {
      headers,
      data: {
        bankTransactionId: evidence.bank,
        journalEntryId: evidence.journal,
        planHash: "0".repeat(64),
        note,
        confirm: true,
      },
    });
    if (bad.status() !== 422) throw new Error("Wrong reviewed proof accepted");
    await page.goto(`${base}/companies/synthetic/bank?year=${encodeURIComponent(evidence.year)}&q=${encodeURIComponent(text)}`);
    const row = page
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: text, exact: true }) });
    await row.getByRole("button", { name: "Afstem eksisterende postering", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Afstem eksisterende postering", exact: true });
    await dialog
      .getByLabel("Eksisterende postering", { exact: true })
      .selectOption(String(evidence.journal));
    if (
      await dialog.getByRole("button", { name: "Opret afstemningslink", exact: true }).isEnabled()
    )
      throw new Error("Manual link skipped explicit review");
    await dialog.getByLabel("Begrundelse for afstemning", { exact: true }).fill(note);
    await dialog.getByRole("checkbox").check();
    const [applied] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().endsWith("/bank/reconciliation-link") && r.request().method() === "POST",
      ),
      dialog.getByRole("button", { name: "Opret afstemningslink", exact: true }).click(),
    ]);
    if (!applied.ok()) throw new Error(await applied.text());
    await dialog.waitFor({ state: "hidden" });
    if (
      JSON.stringify(
        (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries,
      ) !== before ||
      JSON.stringify((await get("payables?status=all")).payables.rows) !== beforePayables
    )
      throw new Error("Manual reconciliation created accounting/payment effects");
    const duplicate = await page.request.post(`${url}/bank/reconciliation-link`, {
      headers,
      data: {
        bankTransactionId: evidence.bank,
        journalEntryId: evidence.journal,
        planHash: plan.planHash,
        note,
        confirm: true,
      },
    });
    if (duplicate.status() !== 422)
      throw new Error("Already reconciled bank/journal was linked again");
    const current = (
      await get(`bank?year=${encodeURIComponent(evidence.year)}`)
    ).bank.transactions.find((t: { id: number }) => t.id === evidence.bank);
    evidence.link = current.reconciliationId;
    evidence.proof = current.reconciliationProof;
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence, null, 2));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const current = (
    await get(`bank?year=${encodeURIComponent(evidence.year)}`)
  ).bank.transactions.find((t: { id: number }) => t.id === evidence.bank);
  if (
    !current ||
    current.reconciliationStatus !== "matched" ||
    current.journalEntryNo !== evidence.entryNo ||
    current.reconciliationId !== evidence.link ||
    current.reconciliationNote !== note ||
    current.reconciliationProof !== evidence.proof
  )
    throw new Error("Immutable link/reason/proof did not survive reload/restart");
  await page.goto(`${base}/companies/synthetic/bank?year=${encodeURIComponent(evidence.year)}&q=${encodeURIComponent(text)}`);
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: text, exact: true }) });
  await row.getByText(`Afstemt · ${evidence.entryNo}`, { exact: true }).waitFor();
  await row.getByText("Afstemningsbevis", { exact: true }).click();
  await row.getByText(note, { exact: true }).waitFor();
  if (await row.getByRole("button", { name: "Ret", exact: true }).count())
    throw new Error("Native view offered unsupported correction route");
  console.log(
    `Browser: native existing-journal reconciliation ${restarted ? "restart" : "initial"} verified`,
  );
}
