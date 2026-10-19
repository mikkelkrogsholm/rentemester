import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { ExceptionRow } from "../../app/src/lib/types/exceptions";
// Controlled launcher-only fixture, inserted before the isolated native service starts:
// INSERT INTO exceptions(type,severity,status,message,required_action,source_evidence,posting_preview,resolution_key)
// VALUES('SYNTHETIC_REVIEW_REQUIRED','low','open','Synthetic exception review fixture',
// 'Record the synthetic review reason; this does not post a journal.',
// '{"source":"rust-browser-fixture","underlyingInvariantSatisfied":false}',
// '{"mustPostSeparately":true}','rust-browser-exception-review-v1');
const saved = "/tmp/rentemester-rust-reports/native-exception-review.json";
const message = "Synthetic exception review fixture";
const note = "Synthetic reviewer checked the evidence; any posting remains a separate action.";
export async function checkNativeExceptionReview(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`, headers = { origin: base, "content-type": "application/json" };
  async function list(status: string): Promise<ExceptionRow[]> {
    const response = await page.request.get(`${url}/exceptions?status=${status}`);
    if (!response.ok()) throw new Error(`Exception queue failed: ${await response.text()}`);
    return (await response.json()).exceptions.rows;
  }
  const rows = await list("all");
  const fixture = rows.find(r => r.type === "SYNTHETIC_REVIEW_REQUIRED" && r.message === message);
  if (!fixture) throw new Error("Seed the controlled synthetic review fixture in the isolated launcher before starting the service");
  if (!restarted) {
    if (fixture.status !== "open") throw new Error("Synthetic exception review fixture was already reviewed");
    const journal = (await (await page.request.get(`${url}/journal`)).json()).journal;
    const invalid = await page.request.post(`${url}/exceptions/${fixture.id}/resolve`, { headers, data: { note: 123 } });
    if (invalid.status() !== 400) throw new Error("Invalid exception review note accepted");
    const missing = await page.request.post(`${url}/exceptions/9007199254740991/resolve`, { headers, data: {} });
    if (missing.status() !== 422) throw new Error("Missing exception resolved");
    const denied = await page.request.post(`${base}/api/companies/unselected/exceptions/${fixture.id}/resolve`, { headers, data: {} });
    if (denied.status() !== 404) throw new Error("Exception review crossed selected company boundary");
    await page.goto(`${base}/companies/synthetic`);
    const task = page.locator(".task-row").filter({ hasText: message });
    await task.getByText(fixture.requiredAction!, { exact: true }).waitFor();
    await task.getByRole("button", { name: "Markér som gennemgået", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Markér opgave som gennemgået", exact: true });
    await dialog.getByText(/dette bogfører ikke noget/).waitFor();
    await dialog.getByLabel("Note — hvad blev bogført? (valgfri)", { exact: true }).fill(note);
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url().endsWith(`/exceptions/${fixture.id}/resolve`) && r.request().method() === "POST"),
      dialog.getByRole("button", { name: "Markér som gennemgået", exact: true }).click(),
    ]);
    if (!response.ok() || (await response.json()).exception.resolved !== true) throw new Error("Real exception review did not resolve the queue item");
    await dialog.waitFor({ state: "hidden" });
    await page.getByText(message, { exact: true }).waitFor({ state: "hidden" });
    const after = (await list("resolved")).find(r => r.id === fixture.id)!;
    if (!after || after.resolutionNote !== note || !after.resolvedAt || !after.resolvedBy?.startsWith("user:")) throw new Error("Exception resolution lost reason, actor or timestamp");
    for (const field of ["type", "message", "requiredAction", "sourceEvidence", "postingPreview", "createdAt"] as const) if (JSON.stringify(after[field]) !== JSON.stringify(fixture[field])) throw new Error(`Review changed original exception fact ${field}`);
    if (JSON.stringify((await (await page.request.get(`${url}/journal`)).json()).journal) !== JSON.stringify(journal)) throw new Error("Review unexpectedly changed journal/underlying bookkeeping");
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(after));
  }
  const expected: ExceptionRow = JSON.parse(readFileSync(saved, "utf8"));
  const replay = await page.request.post(`${url}/exceptions/${expected.id}/resolve`, { headers, data: { note: "Must not replace first review" } });
  if (!replay.ok() || (await replay.json()).exception.resolved !== false) throw new Error("Already resolved exception is not a source-compatible no-op");
  const actual = (await list("resolved")).find(r => r.id === expected.id);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error("No-op/restart changed original review decision or source evidence");
  if ((actual?.sourceEvidence as { underlyingInvariantSatisfied?: boolean }).underlyingInvariantSatisfied !== false || (actual?.postingPreview as { mustPostSeparately?: boolean }).mustPostSeparately !== true) throw new Error("Review falsely repaired the fixture's underlying invariant");
  if ((await list("open")).some(r => r.id === expected.id)) throw new Error("Reviewed exception remains on open queue");
  await page.goto(`${base}/companies/synthetic/undtagelser?status=resolved`);
  await page.getByRole("row").filter({ hasText: message }).getByText("Løst", { exact: true }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-exception-review.png", fullPage: true });
  console.log(restarted ? "PASS: review reason, actor, timestamp, original facts and review-only invariant persist after restart; replay remains no-op" : "PASS: dashboard task → existing confirmation and optional note → reviewed queue/history with actor; invalid input/company refused, replay no-op and journal/invariant unchanged");
}
