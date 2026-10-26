import type { Page } from "@playwright/test";

/** Exercise the visible correction flow, then verify the immutable original and native guards. */
export async function checkNativeReversal(page: Page, base: string) {
  await page.goto(`${base}/companies/synthetic/posteringer`);
  await page.getByText("Synthetic opening transfer", { exact: true }).waitFor();
  const before = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
  const original = before.journal.entries[0];
  if (before.journal.entries.length !== 1) throw new Error("Expected one original posting");
  let reversalBody: string | null = null;
  page.on("request", (request) => { if (request.url().endsWith("/manual-journal-reversal") && request.method() === "POST") reversalBody = request.postData(); });
  await page.getByRole("button", { name: /Synthetic opening transfer/ }).click();
  await page.getByRole("button", { name: "Tilbagefør postering", exact: true }).click();
  await page.getByLabel("Begrundelse", { exact: true }).fill("Synthetic correction");
  await page.getByRole("button", { name: "Gennemse tilbageførsel" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Bogfør tilbageførsel", exact: true }).click();
  await page.getByText(/^Tilbageført ved /).waitFor();
  if (!reversalBody) throw new Error("Browser did not submit the reversal");
  const after = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
  if (after.journal.entries.length !== 2) throw new Error("Expected original and reversal");
  const preserved = after.journal.entries.find((e: { id: number }) => e.id === original.id);
  const reversal = after.journal.entries.find((e: { reversalOfEntryId: number }) => e.reversalOfEntryId === original.id);
  if (!preserved || !reversal || preserved.reversedByEntryId !== reversal.id || reversal.reversalOfEntryNo !== original.entryNo) throw new Error("Missing reversal provenance");
  for (const field of ["entryNo", "date", "text", "total", "documentId", "lines"]) {
    if (JSON.stringify(original[field]) !== JSON.stringify(preserved[field])) throw new Error(`Original changed: ${field}`);
  }
  for (const line of original.lines) {
    const opposite = reversal.lines.find((l: { accountNo: string }) => l.accountNo === line.accountNo);
    if (!opposite || opposite.debit !== line.credit || opposite.credit !== line.debit) throw new Error("Reversal did not swap debit and credit");
  }
  const envelope = JSON.parse(reversalBody);
  const headers = { origin: base, "content-type": "application/json" };
  const replay = await page.request.post(`${base}/api/companies/synthetic/manual-journal-reversal`, { headers, data: envelope });
  if (!replay.ok()) throw new Error("Reversal idempotency replay failed");
  for (const entryId of [original.id, reversal.id]) {
    const refused = await page.request.post(`${base}/api/companies/synthetic/manual-journal-reversal`, { headers, data: {
      ...envelope, input: { ...envelope.input, idempotencyKey: crypto.randomUUID(), request: { ...envelope.input.request, entryId: String(entryId) } },
    } });
    if (refused.status() !== 422) throw new Error("Duplicate reversal or reversal of a reversal was accepted");
  }
  await page.getByRole("main").getByRole("link", { name: "Saldobalance", exact: true }).click();
  await page.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
  const balances = await page.getByRole("main").locator("tbody tr td:last-child").allTextContents();
  if (balances.length !== 2 || !balances.every((value) => value.trim().startsWith("0,00"))) throw new Error("Balances did not return to zero");
  const totals = await page.getByRole("main").locator("tfoot").textContent();
  if (!totals || (totals.match(/251,00/g) ?? []).length !== 2) throw new Error("Original and reversal totals were lost");
  await page.reload();
  await page.getByText("Saldobalancen stemmer — debet = kredit.", { exact: true }).waitFor();
  const journal = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
  if (journal.journal.entries.length !== 2) throw new Error("Replay or rejected correction created extra entries");
  await page.screenshot({ path: "/tmp/rentemester-rust-reversal.png", fullPage: true });
  console.log("PASS: browser reversal preserves original, swaps lines, links both entries and returns balances to zero; duplicate and inverse reversal refused");
}
