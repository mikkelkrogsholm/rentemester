import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-budget-dimensions.json";
const dimension = "synthetic-budget-project",
  account = "3729";
type Row = {
  accountNo: string;
  period: string;
  actual: number;
  otherMembersActual: number;
  unallocatedActual: number;
  accountActual: number;
  accountBudget: number;
  budget: number | null;
  variance: number | null;
  journalLineIds: number[];
};
type Evidence = { year: string; period: string; line: number; assignment: number; row: Row };
/** Own accounts, budget and dimension; does not depend on existing dimension classifications. */
export async function checkNativeBudgetDimensions(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`,
    headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) {
    const r = await page.request.get(`${url}/${path}`);
    if (!r.ok()) throw new Error(`Budget dimensions ${path}: ${await r.text()}`);
    return r.json();
  }
  async function post(path: string, data: unknown) {
    const r = await page.request.post(`${url}/${path}`, { headers, data });
    if (!r.ok()) throw new Error(`Budget dimensions ${path}: ${await r.text()}`);
    return r.json();
  }
  let evidence: Evidence;
  const read = async (member = "beta") =>
    (
      await get(
        `budget-dimension-actuals?year=${encodeURIComponent(evidence.year)}&dimension=${dimension}&member=${member}`,
      )
    ).budgetDimensionActuals;
  const ownRow = (v: { comparison: Row[] }) =>
    v.comparison.find((r) => r.accountNo === account && r.period === evidence.period)!;
  if (!restarted) {
    const date = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
    const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
    const posting = JSON.parse(state.postedBody);
    await page.goto(`${base}/companies/synthetic/kontoplan`);
    for (const [no, name, kind, normal] of [
      [account, "Synthetic dimension budget expense", "expense", "debit"],
      ["8729", "Synthetic dimension budget equity", "equity", "credit"],
    ]) {
      const form = page.getByRole("form", { name: "Opret konto", exact: true });
      await form.getByLabel("Kontonummer", { exact: true }).fill(no);
      await form.getByLabel("Kontonavn", { exact: true }).fill(name);
      await form.getByLabel(/^Kontotype/).selectOption(kind);
      await form.getByLabel(/^Normal saldo/).selectOption(normal);
      await form.getByRole("button", { name: "Opret konto", exact: true }).click();
      await page.getByRole("row").filter({ hasText: name }).waitFor();
    }
    for (const [index, amount] of ["12.34", "3.21"].entries()) {
      const original = await post("documents/ingest", {
        confirm: true,
        fileName: `synthetic-dimension-budget-${index}.txt`,
        fileBase64: Buffer.from(`Synthetic dimension budget expense ${index}: DKK${amount}, no VAT, paid by owner.\n`).toString("base64"),
        metadata: {
          source: "Synthetic dimension budget fixture",
          documentType: "cash_register_receipt",
          issueDate: date,
          amountIncVat: Number(amount),
          vatAmount: 0,
          currency: "DKK",
        },
      });
      posting.input.request = {
        transactionDate: date,
        text: `Synthetic dimension budget ${index}`,
        currency: "DKK",
        documentId: String(original.document.id),
        lines: [
          { accountNo: account, debitAmount: amount, creditAmount: "0.00" },
          { accountNo: "8729", debitAmount: "0.00", creditAmount: amount },
        ],
      };
      posting.input.idempotencyKey = `synthetic-dimension-budget-source-${index}`;
      await post("manual-journal", posting);
    }
    const year = (await get("budget")).budget.selectedYear;
    const journal = (await get(`journal?year=${encodeURIComponent(year)}`)).journal;
    const entry = journal.entries.find(
      (e: { text: string }) => e.text === "Synthetic dimension budget 0",
    );
    const line = entry?.lines.find(
      (l: { accountNo: string }) => l.accountNo === account,
    )?.journalLineId;
    if (!line) throw new Error("Own expense journal line missing");
    evidence = {
      year: journal.selectedYear,
      period: date.slice(0, 7),
      line,
      assignment: 0,
      row: null as unknown as Row,
    };
    await post("budget", { accountNo: account, period: evidence.period, amount: 20 });
    await post("dimensions/define", {
      dimensionId: dimension,
      kind: "project",
      name: "Synthetic budget projects",
      confirm: true,
    });
    for (const member of ["alpha", "beta"])
      await post("dimensions/member", {
        dimensionId: dimension,
        memberId: member,
        name: `Synthetic budget ${member}`,
        confirm: true,
      });
    const initial = {
      journalLineId: line,
      source: "reviewed",
      allocations: [
        { dimensionId: dimension, memberId: "alpha", amountMinor: 1001, currency: "DKK" },
        { dimensionId: dimension, memberId: "beta", amountMinor: 233, currency: "DKK" },
      ],
    };
    const first = (await post("dimensions/plan", initial)).plan;
    const assigned = await post("dimensions/apply", {
      ...initial,
      planHash: first.planHash,
      confirm: true,
    });
    const before = ownRow(await read("alpha"));
    if (
      !before ||
      before.actual !== 10.01 ||
      before.otherMembersActual !== 2.33 ||
      before.unallocatedActual !== 3.21 ||
      before.accountActual !== 15.55 ||
      before.accountBudget !== 20 ||
      before.budget !== null ||
      before.variance !== null
    )
      throw new Error(`Wrong initial member actual/context: ${JSON.stringify(before)}`);
    // The new Budget UI reads a server-authoritative scoped result, including unallocated amounts.
    await page.goto(`${base}/companies/synthetic/budget?year=${encodeURIComponent(evidence.year)}`);
    await page.getByRole("button", { name: "Sammenlign med faktisk", exact: true }).click();
    await page.getByLabel("Filter dimension", { exact: true }).selectOption(`${dimension}:alpha`);
    const section = page.getByRole("region", { name: "Dimensionssammenligning", exact: true });
    const uiRow = section
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: account, exact: true }) });
    await uiRow.getByText("10,01 kr.", { exact: true }).waitFor();
    await uiRow.getByText("3,21 kr.", { exact: true }).waitFor();
    if ((await uiRow.getByText("Ukendt", { exact: true }).count()) !== 2)
      throw new Error("Account budget fabricated dimension budget/variance");
    const replacement = {
      journalLineId: line,
      source: "reviewed",
      allocations: [
        { dimensionId: dimension, memberId: "beta", amountMinor: 1234, currency: "DKK" },
      ],
    };
    const planned = (await post("dimensions/plan", replacement)).plan;
    const result = await post("dimensions/replace", {
      ...replacement,
      expectedAssignmentId: assigned.id,
      planHash: planned.planHash,
      reason: "Synthetic budget review corrected member with immutable source unchanged.",
      confirm: true,
    });
    evidence.assignment = result.id;
    const stale = await page.request.post(`${url}/dimensions/replace`, {
      headers,
      data: {
        ...initial,
        expectedAssignmentId: assigned.id,
        planHash: first.planHash,
        reason: "Synthetic stale review refusal",
        confirm: true,
      },
    });
    if (stale.status() !== 422) throw new Error("Stale reviewed replacement accepted");
    const alpha = ownRow(await read("alpha"));
    if (
      alpha.actual !== 0 ||
      alpha.otherMembersActual !== 12.34 ||
      alpha.unallocatedActual !== 3.21
    )
      throw new Error("Superseded assignment still counted");
    evidence.row = ownRow(await read());
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence, null, 2));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const beforeJournal = JSON.stringify(
    (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries,
  );
  const current = ownRow(await read());
  if (
    JSON.stringify(current) !== JSON.stringify(evidence.row) ||
    current.actual !== 12.34 ||
    current.unallocatedActual !== 3.21
  )
    throw new Error("Final dimension actual/provenance did not persist");
  const history = (await get(`dimensions/${evidence.line}`)).assignments;
  if (history.length !== 3 || history[2].id !== evidence.assignment)
    throw new Error("Reasoned replacement history changed");
  const invalid = await page.request.get(`${url}/budget-dimension-actuals?member=beta`);
  if (invalid.status() !== 422) throw new Error("Unscoped member selector accepted");
  await page.goto(`${base}/companies/synthetic/budget?year=${encodeURIComponent(evidence.year)}`);
  await page.getByRole("button", { name: "Sammenlign med faktisk", exact: true }).click();
  await page.getByLabel("Filter dimension", { exact: true }).selectOption(`${dimension}:beta`);
  const section = page.getByRole("region", { name: "Dimensionssammenligning", exact: true });
  const row = section
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name: account, exact: true }) });
  await row.getByText("12,34 kr.", { exact: true }).waitFor();
  await row.getByText("3,21 kr.", { exact: true }).waitFor();
  await row.getByRole("link", { name: `Journal-linje ${evidence.line}`, exact: true }).waitFor();
  if (
    JSON.stringify(
      (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries,
    ) !== beforeJournal
  )
    throw new Error("Dimension comparison changed accounting");
  console.log(
    `Browser: native budget dimension actuals ${restarted ? "restart" : "initial"} verified`,
  );
}
