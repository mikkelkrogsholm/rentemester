import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-bank-correction.json";
const text = "Synthetic native bank link correction";
const reason =
  "Synthetic reviewer checked actual reversal and independent replacement against original statement.";
type Body = {
  bankTransactionId: number;
  replacementJournalEntryId: number;
  expectedReconciliationId: string;
  planHash: string;
  reason: string;
  idempotencyKey: string;
  confirm: true;
};
type Evidence = {
  year: string;
  bank: number;
  original: number;
  reversal: number;
  replacement: number;
  replacementNo: string;
  body: Body;
  correction: string;
  history: unknown[];
  journals: unknown[];
};
/** Own fixtures; correction itself must never create reversal, replacement, payment or journal. */
export async function checkNativeBankCorrection(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`,
    headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) {
    const r = await page.request.get(`${url}/${path}`);
    if (!r.ok()) throw new Error(`Bank correction ${path}: ${await r.text()}`);
    return r.json();
  }
  async function post(path: string, data: unknown) {
    const r = await page.request.post(`${url}/${path}`, { headers, data });
    if (!r.ok()) throw new Error(`Bank correction ${path}: ${await r.text()}`);
    return r.json();
  }
  let evidence: Evidence;
  const journal = async () =>
    (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
  const bank = async () => (await get(`bank?year=${encodeURIComponent(evidence.year)}`)).bank;
  if (!restarted) {
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const [no, name, type, normal] of [
      ["1748", "Synthetic correction bank asset", "asset", "debit"],
      ["8748", "Synthetic correction original equity", "equity", "credit"],
      ["8749", "Synthetic correction replacement equity", "equity", "credit"],
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(name);
      await form.getByLabel(/^Kontotype/).selectOption(type);
      await form.getByLabel(/^Normal saldo/).selectOption(normal);
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("row").filter({ hasText: name }).waitFor();
    }
    await post("bank-accounts", {
      name: "Synthetic native correction bank",
      slug: "synthetic-native-correction",
      currency: "DKK",
      ledgerAccountNo: "1748",
    });
    const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
    const posting = JSON.parse(state.postedBody);
    const date = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
    const request = (equity: string, label: string) => ({
      transactionDate: date,
      text: label,
      currency: "DKK",
      lines: [
        { accountNo: "1748", debitAmount: "27.18", creditAmount: "0.00" },
        { accountNo: equity, debitAmount: "0.00", creditAmount: "27.18" },
      ],
    });
    posting.input.request = request("8748", text);
    posting.input.idempotencyKey = "synthetic-native-correction-original";
    await post("manual-journal", posting);
    await post("bank/import", {
      csvContent: `transaction_date,text,amount,currency,balance_after\n${date},${text},27.18,DKK,27.18\n`,
      account: "synthetic-native-correction",
      confirm: true,
    });
    const initialBank = (await get("bank")).bank;
    evidence = {
      year: initialBank.selectedYear,
      bank: initialBank.transactions.find((t: { text: string }) => t.text === text).id,
      original: 0,
      reversal: 0,
      replacement: 0,
      replacementNo: "",
      body: null as unknown as Body,
      correction: "",
      history: [],
      journals: [],
    };
    const original = (await journal()).find((e: { text: string }) => e.text === text);
    evidence.original = original.id;
    const initial = (
      await get(`bank/reconciliation-candidates?bankTransactionId=${evidence.bank}`)
    ).reconciliationCandidates.plans.find(
      (p: { journalEntryId: number }) => p.journalEntryId === evidence.original,
    );
    await post("bank/reconciliation-link", {
      bankTransactionId: evidence.bank,
      journalEntryId: evidence.original,
      planHash: initial.planHash,
      note: "Synthetic initial source confirmation before correction",
      confirm: true,
    });
    // Independent preparation uses the existing accounting engines, not the correction endpoint.
    posting.input.request = request("8749", `${text} replacement`);
    posting.input.idempotencyKey = "synthetic-native-correction-replacement";
    await post("manual-journal", posting);
    const replacement = (await journal()).find(
      (e: { text: string }) => e.text === `${text} replacement`,
    );
    evidence.replacement = replacement.id;
    evidence.replacementNo = replacement.entryNo;
    const planPath = `bank/reconciliation-correction-plan?bankTransactionId=${evidence.bank}&replacementJournalEntryId=${evidence.replacement}`;
    const active = await page.request.get(`${url}/${planPath}`);
    if (active.status() !== 422)
      throw new Error("Active unreversed original accepted for correction");
    const reversal = JSON.parse(state.postedBody);
    reversal.input.format = "rentemester-manual-journal-reversal/1";
    reversal.input.idempotencyKey = "synthetic-native-correction-explicit-reversal";
    reversal.input.request = {
      entryId: String(evidence.original),
      transactionDate: date,
      reason: "Synthetic explicit reversal before reviewed bank link correction",
    };
    await post("manual-journal-reversal", reversal);
    const entries = await journal();
    evidence.reversal = entries.find(
      (e: { reversalOfEntryId: number }) => e.reversalOfEntryId === evidence.original,
    )?.id;
    if (!evidence.reversal) throw new Error("Authoritative reversal child not created");
    const planned = (await get(planPath)).plan.plan;
    if (
      planned.reversalJournalEntryId !== evidence.reversal ||
      planned.currentJournalEntryId !== evidence.original ||
      planned.bankAmountDkk !== 27.18 ||
      planned.replacementBankMovement !== 27.18
    )
      throw new Error("Correction plan omitted actual accounting/source facts");
    evidence.body = {
      bankTransactionId: evidence.bank,
      replacementJournalEntryId: evidence.replacement,
      expectedReconciliationId: planned.reconciliationId,
      planHash: planned.planHash,
      reason,
      idempotencyKey: "synthetic-native-correction-confirmed",
      confirm: true,
    };
    const before = JSON.stringify(entries),
      beforePayables = JSON.stringify((await get("payables?status=all")).payables.rows),
      beforeBank = await bank();
    const bad = await page.request.post(`${url}/bank/reconciliation-correction`, {
      headers,
      data: {
        ...evidence.body,
        planHash: "0".repeat(64),
        idempotencyKey: "synthetic-native-correction-bad-proof",
      },
    });
    if (bad.status() !== 422) throw new Error("Wrong correction proof accepted");
    await page.goto(
      `${base}/companies/synthetic/bank?year=${encodeURIComponent(evidence.year)}&q=${encodeURIComponent(text)}`,
    );
    const row = page
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: text, exact: true }) });
    await row.getByRole("button", { name: "Ret", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Ret afstemt bankpost", exact: true });
    await dialog
      .getByLabel("Erstatningsjournal-id", { exact: true })
      .fill(String(evidence.replacement));
    await dialog.getByRole("button", { name: "Kontrollér plan", exact: true }).click();
    await dialog.getByText("Kontrolleret tilbageførsel:", { exact: true }).waitFor();
    if (await dialog.getByRole("button", { name: "Anvend korrektion", exact: true }).isEnabled())
      throw new Error("Correction skipped confirmation/reason/key review");
    await dialog.getByLabel("Begrundelse", { exact: true }).fill(reason);
    await dialog.getByLabel("Idempotensnøgle", { exact: true }).fill(evidence.body.idempotencyKey);
    await dialog.getByRole("checkbox").check();
    const [applied] = await Promise.all([
      page.waitForResponse(
        (r) =>
          r.url().endsWith("/bank/reconciliation-correction") && r.request().method() === "POST",
      ),
      dialog.getByRole("button", { name: "Anvend korrektion", exact: true }).click(),
    ]);
    if (!applied.ok()) throw new Error(await applied.text());
    const result = (await applied.json()).correction;
    evidence.correction = result.reconciliationId;
    if (result.receipt.replayed !== false)
      throw new Error("First correction returned a replay receipt");
    await dialog.waitFor({ state: "hidden" });
    const afterBank = await bank();
    if (
      JSON.stringify(await journal()) !== before ||
      JSON.stringify((await get("payables?status=all")).payables.rows) !== beforePayables ||
      afterBank.bookedBalance !== beforeBank.bookedBalance ||
      afterBank.actualBalance !== beforeBank.actualBalance
    )
      throw new Error("Correction mutated accounting/payment/statement totals");
    const stale = await page.request.post(`${url}/bank/reconciliation-correction`, {
      headers,
      data: { ...evidence.body, idempotencyKey: "synthetic-native-correction-stale" },
    });
    if (stale.status() !== 422) throw new Error("Concurrent stale supersession accepted");
    const current = afterBank.transactions.find((t: { id: number }) => t.id === evidence.bank);
    evidence.history = current.reconciliationHistory;
    evidence.journals = (await journal()).filter((e: { id: number }) =>
      [evidence.original, evidence.reversal, evidence.replacement].includes(e.id),
    );
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence, null, 2));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const replay = (await post("bank/reconciliation-correction", evidence.body)).correction;
  if (
    replay.reconciliationId !== evidence.correction ||
    replay.planHash !== evidence.body.planHash ||
    replay.receipt.replayed !== true ||
    replay.idempotent !== true
  )
    throw new Error("Durable exact receipt replay failed");
  const current = (await bank()).transactions.find((t: { id: number }) => t.id === evidence.bank);
  if (
    current.journalEntryNo !== evidence.replacementNo ||
    current.reconciliationStatus !== "matched" ||
    current.reconciliationId !== evidence.correction ||
    current.reconciliationNote !== reason ||
    JSON.stringify(current.reconciliationHistory) !== JSON.stringify(evidence.history)
  )
    throw new Error("Correction current target/immutable history did not persist");
  if (
    JSON.stringify(
      (await journal()).filter((e: { id: number }) =>
        [evidence.original, evidence.reversal, evidence.replacement].includes(e.id),
      ),
    ) !== JSON.stringify(evidence.journals)
  )
    throw new Error("Prepared accounting evidence changed after receipt replay/restart");
  await page.goto(
    `${base}/companies/synthetic/bank?year=${encodeURIComponent(evidence.year)}&q=${encodeURIComponent(text)}`,
  );
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: text, exact: true }) });
  await row.getByText(`Afstemt · ${evidence.replacementNo}`, { exact: true }).waitFor();
  await row.getByText("Afstemningshistorik", { exact: true }).click();
  await row.getByText("Linkkorrektion", { exact: true }).waitFor();
  await row.getByText(reason, { exact: true }).first().waitFor();
  console.log(`Browser: native bank link correction ${restarted ? "restart" : "initial"} verified`);
}
