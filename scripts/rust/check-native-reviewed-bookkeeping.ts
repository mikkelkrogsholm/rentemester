import type { Page } from "@playwright/test";
import { isDeepStrictEqual } from "node:util";

type Fixture = { bankTransactionId: number; counterAccount: string; documentId?: number };
/** Two independently MFA-authenticated contexts, with current rights in a synthetic company.
 * The author needs draft.write + review (rule authoring); the other user needs review.
 * Seed an unposted DKK bank row with a mapped bank ledger account before invoking.
 * No user/grant/session bootstrap is performed here. No mail or real workspace data.
 */
export async function checkNativeReviewedBookkeeping(author: Page, reviewer: Page, base: string, fixture: Fixture) {
  const company = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const read = async (page: Page, suffix: string) => {
    const response = await page.request.get(`${company}/${suffix}`);
    if (!response.ok()) throw new Error(`Reviewed bookkeeping ${suffix} unavailable (${response.status()})`);
    return response.json();
  };
  // Distinct browser contexts are an actual prerequisite, not a changed attribution field.
  const me = await author.request.get(`${base}/api/me`);
  const other = await reviewer.request.get(`${base}/api/me`);
  if (!me.ok() || !other.ok()) throw new Error("Two current authenticated users are required");
  const a = await me.json(), b = await other.json();
  if (!a.user?.id || !b.user?.id || a.user.id === b.user.id) throw new Error("Independent reviewer fixture must use another user");
  const queue = (await read(author, "reviewed-bank")).queue;
  const row = queue.rows.find((r: { bankTransactionId: number }) => r.bankTransactionId === fixture.bankTransactionId);
  if (!row || !row.bankAccountNo || row.currency !== "DKK") throw new Error("Unposted mapped synthetic DKK bank row required");
  const ruleId = `synthetic-review-${fixture.bankTransactionId}`;
  await author.goto(`${base}/companies/synthetic/posteringsregler`);
  await author.getByLabel("Regel-id", { exact: true }).fill(ruleId);
  await author.getByLabel("Præcis banktekst", { exact: true }).fill(row.text);
  await author.getByLabel("Foreslået modkonto", { exact: true }).fill(fixture.counterAccount);
  await author.getByLabel("Gælder fra", { exact: true }).fill(row.date);
  await author.getByLabel("Begrundelse", { exact: true }).fill("Synthetic source reviewed by two independent users");
  await author.getByRole("button", { name: "Gennemgå forslag", exact: true }).click();
  await author.getByRole("dialog").getByRole("button", { name: "Bekræft", exact: true }).click();
  await author.getByRole("dialog").waitFor({ state: "hidden" });
  const rule = (await read(author, "posting-rules")).postingRules.find((r: { ruleId: string }) => r.ruleId === ruleId);
  if (!rule) throw new Error("Rule proposal was not persisted");
  const selfRule = await author.request.post(`${company}/posting-rules/approve`, { headers, data: { confirm: true, ruleId, version: rule.version, expectedPayloadHash: rule.payloadHash, rationale: "Synthetic self review must refuse", provenance: "synthetic" } });
  if (selfRule.status() !== 422) throw new Error("Rule author self-approval was not refused");
  await reviewer.goto(`${base}/companies/synthetic/posteringsregler`);
  const ruleRow = reviewer.getByRole("row").filter({ hasText: ruleId });
  await ruleRow.getByRole("button", { name: "Godkend regel", exact: true }).click();
  await reviewer.getByRole("dialog").getByLabel("Begrundelse", { exact: true }).fill("Independent synthetic review");
  await reviewer.getByRole("dialog").getByRole("button", { name: "Bekræft", exact: true }).click();
  await reviewer.getByRole("dialog").waitFor({ state: "hidden" });
  await author.goto(`${base}/companies/synthetic/batchbogfoering?mode=single`);
  const bankRow = author.getByRole("row").filter({ hasText: `#${fixture.bankTransactionId} ${row.text}` });
  await bankRow.getByRole("button", { name: "Opret bankkladde", exact: true }).click();
  if (await author.getByLabel("Modkonto", { exact: true }).inputValue() !== fixture.counterAccount) throw new Error("Approved exact rule did not provide its advisory hint");
  if (fixture.documentId) await author.getByLabel("Bilags-id (kræves ved udgift eller indtægt)", { exact: true }).fill(String(fixture.documentId));
  const draftId = `bank-${fixture.bankTransactionId}`;
  await author.getByRole("button", { name: "Gennemgå oprettelse", exact: true }).click();
  await author.getByRole("dialog").getByRole("button", { name: "Opret kladde", exact: true }).click();
  await author.getByRole("status").waitFor();
  await author.getByRole("link", { name: "Åbn kladder og indsend til uafhængig godkendelse", exact: true }).click();
  const draftRow = author.getByRole("row").filter({ has: author.getByText(draftId, { exact: true }) });
  await draftRow.getByRole("button", { name: "Indsend", exact: true }).click();
  await author.getByRole("dialog").getByRole("button", { name: "Indsend", exact: true }).click();
  await draftRow.getByText("Afventer godkendelse", { exact: true }).waitFor();
  const submitted = (await read(author, "accounting-drafts")).accountingDrafts.find((d: { id: string }) => d.id === draftId);
  const data = { expectedEventHash: submitted.eventHash, confirm: true };
  const endpoint = `${company}/accounting-drafts/${draftId}/approve-and-post`;
  const selfDraft = await author.request.post(endpoint, { headers, data });
  if (selfDraft.status() !== 422) throw new Error("Draft self-review was not refused");
  await reviewer.goto(`${base}/companies/synthetic/kladder`);
  const reviewRow = reviewer.getByRole("row").filter({ has: reviewer.getByText(draftId, { exact: true }) });
  await reviewRow.getByText("Gennemgå præcise linjer og kilder", { exact: true }).click();
  await reviewRow.getByRole("button", { name: "Godkend og bogfør", exact: true }).click();
  await reviewer.getByRole("dialog").getByRole("button", { name: "Godkend og bogfør", exact: true }).click();
  await reviewRow.getByText("Godkendt og bogført", { exact: true }).waitFor();
  const posted = (await read(reviewer, "accounting-drafts")).accountingDrafts.find((d: { id: string }) => d.id === draftId);
  const replay = await reviewer.request.post(endpoint, { headers, data });
  if (!replay.ok() || !isDeepStrictEqual((await replay.json()).accountingDraft, posted)) throw new Error("Exact approved request did not read its durable effect");
  const remaining = (await read(author, "reviewed-bank")).queue.rows;
  if (remaining.some((r: { bankTransactionId: number }) => r.bankTransactionId === fixture.bankTransactionId)) throw new Error("Posted bank row remained in unposted queue");
  console.log("PASS: exact rule proposal → independent approval → bank-backed draft → submission → distinct reviewer → one durable posting; self-review refused and exact receipt replay stable");
  return { draftId, submittedHash: submitted.eventHash, journalEntryId: posted.journalEntryId };
}
