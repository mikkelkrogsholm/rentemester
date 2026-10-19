import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-commitments.json";
export async function checkNativeCommitments(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`,
    headers = { origin: base, "content-type": "application/json" };
  async function read(path: string) {
    const r = await page.request.get(`${url}/${path}`);
    if (!r.ok()) throw new Error(`Commitment read ${path}: ${await r.text()}`);
    return r.json();
  }
  let evidence: {
    id: string;
    doc: number;
    date: string;
    endDate: string;
    vendor: number;
    hash: string;
    original: string;
  };
  const list = async () =>
    (await read(`supplier-commitments?startDate=${evidence.date}`)).supplierCommitments;
  if (!restarted) {
    const year = new Date()
      .toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" })
      .slice(0, 4);
    evidence = {
      id: "synthetic-native-commitment",
      doc: 0,
      date: `${year}-01-31`,
      endDate: new Date(Date.UTC(Number(year),2,0)).toISOString().slice(0,10),
      vendor: 0,
      hash: "",
      original:
        "Synthetic native commitment source:10.01DKK monthly service, reviewed planning only.\n",
    };
    const vendor = await page.request.post(`${url}/vendors`, {
      headers,
      data: { name: "Synthetic Commitment Supplier" },
    });
    if (!vendor.ok()) throw new Error(await vendor.text());
    evidence.vendor = (await vendor.json()).vendor.id;
    const upload = await page.request.post(`${url}/documents/ingest`, {
      headers,
      data: {
        confirm: true,
        fileName: "synthetic-native-commitment.txt",
        fileBase64: Buffer.from(evidence.original).toString("base64"),
        metadata: {
          source: "Synthetic commitment source",
          documentType: "cash_register_receipt",
          issueDate: evidence.date,
          currency: "DKK",
          amountIncVat: 10.01,
        },
      },
    });
    if (!upload.ok()) throw new Error(await upload.text());
    evidence.doc = (await upload.json()).document.id;
    const beforeJournal = JSON.stringify((await read("journal")).journal.entries),
      beforePayables = JSON.stringify((await read("payables?status=all")).payables.rows),
      beforeBank = JSON.stringify((await read("bank")).bank.transactions);
    await page.goto(`${base}/companies/synthetic/likviditet`);
    await page.getByRole("heading", { name: "Likviditet", exact: true }).waitFor();
    await page
      .locator("header.company-context")
      .getByText("Hele virksomheden", { exact: true })
      .waitFor();
    if (await page.getByRole("combobox", { name: "Vælg regnskabsår" }).count())
      throw new Error("Native forecast displayed irrelevant annual filter");
    const [dated] = await Promise.all([
      page.waitForResponse((r) =>
        r.url().includes("/supplier-commitments?startDate=" + evidence.date),
      ),
      page.getByLabel("Prognose fra", { exact: true }).fill(evidence.date),
    ]);
    if (!dated.ok()) throw new Error(await dated.text());
    await page.getByRole("button", { name: "Ny forpligtelse", exact: true }).click();
    const form = page.getByRole("form", { name: "Planlæg leverandørforpligtelse", exact: true });
    for (const [label, value] of Object.entries({
      Reference: evidence.id,
      Type: "subscription",
      Beskrivelse: "Synthetic subscription source",
      Forretningsformål: "Synthetic service planning",
      "Beløb pr. betaling": "10.01",
      Valuta: "DKK",
      "Næste betaling": evidence.date,
    }))
      await form.getByLabel(label, { exact: true }).fill(value);
    await form.getByLabel("Leverandør", { exact: true }).selectOption(String(evidence.vendor));
    await form
      .getByLabel("Originalt kildebilag", { exact: true })
      .selectOption(String(evidence.doc));
    const [planned] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().endsWith("/supplier-commitments/plan") && r.request().method() === "POST",
      ),
      form.getByRole("button", { name: "Vis plan til review", exact: true }).click(),
    ]);
    if (!planned.ok()) throw new Error(await planned.text());
    const plan = (await planned.json()).plan;
    evidence.hash = plan.payloadHash;
    if (
      plan.schedule[1].date !== evidence.endDate ||
      plan.schedule[2].date !== `${year}-03-31` ||
      plan.amount !== 10.01
    )
      throw new Error("Month-end/exact plan differs");
    if (
      (await list()).commitments.some(
        (c: { commitmentId: string }) => c.commitmentId === evidence.id,
      )
    )
      throw new Error("Planning created approval before confirmation");
    const review = page.getByRole("dialog", { name: "Godkend forpligtelse", exact: true });
    await review.getByText(evidence.endDate, { exact: true }).waitFor();
    const [approved] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().endsWith("/supplier-commitments") && r.request().method() === "POST",
      ),
      review.getByRole("button", { name: "Godkend planen", exact: true }).click(),
    ]);
    if (!approved.ok()) throw new Error(await approved.text());
    await review.waitFor({ state: "hidden" });
    const amount = async () => {
      const d = await list();
      return d.forecast.periods
        .flatMap((p: { sources: Array<{ reference: string; amount: number }> }) => p.sources)
        .filter((s: { reference: string }) => s.reference === evidence.id)
        .reduce((n: number, s: { amount: number }) => n + Math.round(s.amount * 100), 0);
    };
    if ((await amount()) !== -4004)
      throw new Error("Four expected occurrences did not enter exact13-week forecast");
    await page.getByText("Match faktisk bilag, kreditor eller bankpost", { exact: true }).click();
    const match = page.getByRole("form", { name: "Match forventet betaling", exact: true });
    await match
      .getByLabel("Forventet betaling", { exact: true })
      .selectOption(`${evidence.id}:${evidence.date}`);
    await match
      .getByLabel("Faktisk kilde", { exact: true })
      .selectOption(`canonical_document:${evidence.doc}`);
    await match.getByRole("button", { name: "Gennemgå match", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Bekræft evidensmatch", exact: true });
    const [matched] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().endsWith("/supplier-commitments/match") && r.request().method() === "POST",
      ),
      dialog.getByRole("button", { name: "Bekræft match", exact: true }).click(),
    ]);
    if (!matched.ok()) throw new Error(await matched.text());
    await dialog.waitFor({ state: "hidden" });
    if ((await amount()) !== -3003)
      throw new Error("Canonical match did not remove duplicate expectation");
    const duplicate = await page.request.post(`${url}/supplier-commitments/match`, {
      headers,
      data: {
        commitmentId: evidence.id,
        occurrenceDate: evidence.endDate,
        evidence: { kind: "canonical_document", id: String(evidence.doc) },
        confirm: true,
      },
    });
    if (duplicate.status() !== 422) throw new Error("Canonical source was consumed twice");
    const spoof = await page.request.post(`${url}/supplier-commitments/match`, {
      headers,
      data: {
        commitmentId: evidence.id,
        occurrenceDate: evidence.endDate,
        evidence: { kind: "canonical_document", id: String(evidence.doc), amount: 0 },
        confirm: true,
      },
    });
    if (spoof.status() !== 400) throw new Error("Caller supplied fake canonical evidence amount");
    const row = page
      .getByRole("table", { name: "Leverandørforpligtelser", exact: true })
      .getByRole("row")
      .filter({ hasText: evidence.id });
    for (const action of ["Pause", "Afslut"]) {
      await row.getByRole("button", { name: action, exact: true }).click();
      const change = page.getByRole("dialog", {
        name: action === "Pause" ? "Pause forpligtelse" : "Afslut forpligtelse",
        exact: true,
      });
      await change.getByLabel("Begrundelse", { exact: true }).fill(`Synthetic reviewed ${action}`);
      const [r] = await Promise.all([
        page.waitForResponse(
          (r) =>
            r.url().endsWith("/supplier-commitments/change") && r.request().method() === "POST",
        ),
        change.getByRole("button", { name: action, exact: true }).click(),
      ]);
      if (!r.ok()) throw new Error(await r.text());
      await change.waitFor({ state: "hidden" });
      await row.getByText(action === "Pause" ? /Pauseret/ : /Afsluttet/).waitFor();
      const d = await list();
      if (d.occurrences.some((o: { commitmentId: string }) => o.commitmentId === evidence.id))
        throw new Error("Stopped commitment still forecast");
    }
    if (
      JSON.stringify((await read("journal")).journal.entries) !== beforeJournal ||
      JSON.stringify((await read("payables?status=all")).payables.rows) !== beforePayables ||
      JSON.stringify((await read("bank")).bank.transactions) !== beforeBank
    )
      throw new Error("Planning mutated journal/payable/payment/bank facts");
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const d = await list(),
    row = d.commitments.find((c: { commitmentId: string }) => c.commitmentId === evidence.id);
  if (
    row?.status !== "ended" ||
    d.history.filter((h: { commitmentId: string }) => h.commitmentId === evidence.id).length !==
      3 ||
    d.matches.filter((m: { commitmentId: string }) => m.commitmentId === evidence.id).length !==
      1 ||
    d.occurrences.some((o: { commitmentId: string }) => o.commitmentId === evidence.id) ||
    d.forecast.completeness.status !== "incomplete"
  )
    throw new Error("Commitment history/match/forecast state not persisted");
  const original = await page.request.get(`${url}/documents/${evidence.doc}/file`);
  if (!original.ok() || (await original.body()).toString() !== evidence.original)
    throw new Error("Commitment original changed");
  if (
    (await read("payables?status=all")).payables.rows.some(
      (p: { documentId: number }) => p.documentId === evidence.doc,
    )
  )
    throw new Error("Planning source became a payable");
  await page.goto(`${base}/companies/synthetic/likviditet`);
  await page.getByRole("row").filter({ hasText: evidence.id }).getByText(/Afsluttet/).waitFor();
  await page.getByText("Uforanderlig historik", { exact: true }).click();
  await page.getByText(/Synthetic reviewed Afslut/).waitFor();
  console.log(
    restarted
      ? "PASS: canonical commitment approval/match, reasoned lifecycle and original evidence survive restart"
      : "PASS: real source/vendor → explicit plan → hash-reviewed approval → exact13-week occurrences → canonical match/deduplication → reasoned pause/end/history; no accounting effects, incomplete coverage explicit",
  );
}
