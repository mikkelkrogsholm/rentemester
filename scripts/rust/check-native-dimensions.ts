import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import type { DimensionAssignmentEvent, DimensionPlan } from "../../app/src/lib/api/dimensions";
const saved = "/tmp/rentemester-rust-reports/native-dimensions.json";
const dimension = "synthetic-project", reason = "Synthetic reviewer corrected the project after checking original evidence.";
type Entry = { id: number; entryNo: string; text: string; lines: Array<{ journalLineId: number; accountNo: string; debit: number; credit: number }> };
type Evidence = { year: string; entry: Entry; lineId: number; history: DimensionAssignmentEvent[]; journal: Entry[]; definitions?: unknown[]; members?: unknown[] };
function canonical(v: unknown): string {
  if (Array.isArray(v)) return `[${v.map(canonical).join(",")}]`;
  if (v !== null && typeof v === "object") return `{${Object.keys(v).sort().map(k => `${JSON.stringify(k)}:${canonical((v as Record<string, unknown>)[k])}`).join(",")}}`;
  return JSON.stringify(v);
}
export async function checkNativeDimensions(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`, headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) { const r = await page.request.get(`${url}/${path}`); if (!r.ok()) throw new Error(`Dimensions ${path}: ${await r.text()}`); return r.json(); }
  async function history(line: number): Promise<DimensionAssignmentEvent[]> { return (await get(`dimensions/${line}`)).assignments; }
  async function openLine(evidence: Evidence) {
    await page.goto(`${base}/companies/synthetic/posteringer?year=${encodeURIComponent(evidence.year)}`);
    const entry = page.locator(".entry-item").filter({ has: page.locator(".entry-no", { hasText: evidence.entry.entryNo }) });
    await entry.getByRole("button", { name: new RegExp(evidence.entry.entryNo) }).click();
    const line = entry.getByRole("row").filter({ has: page.getByRole("cell", { name: evidence.entry.lines.find(l => l.journalLineId === evidence.lineId)!.accountNo, exact: true }) });
    const details = line.locator(".dimension-assignment");
    await details.locator(":scope > summary").click();
    return details;
  }
  let evidence: Evidence;
  if (!restarted) {
    const journal = (await get("journal")).journal;
    let entry: Entry | undefined, lineId = 0;
    for (const candidate of journal.entries as Entry[]) {
      const line = candidate.lines.find(l => l.journalLineId && Math.abs(l.debit - l.credit) > 0);
      if (line && (await history(line.journalLineId)).length === 0) { entry = candidate; lineId = line.journalLineId; break; }
    }
    if (!entry) throw new Error("Synthetic fixture needs one existing, unclassified immutable journal line");
    evidence = { year: journal.selectedYear, entry, lineId, history: [], journal: journal.entries };
    await page.goto(`${base}/companies/synthetic/dimensioner`);
    await page.getByRole("heading", { name: "Dimensioner", exact: true }).waitFor();
    await page.getByLabel("Dimensions-id", { exact: true }).fill(dimension);
    await page.getByLabel("Dimensionstype", { exact: true }).fill("project");
    await page.getByLabel("Dimensionsnavn", { exact: true }).fill("Synthetic projects");
    await page.getByRole("button", { name: "Opret dimension", exact: true }).click();
    await page.getByRole("dialog", { name: "Opret dimension", exact: true }).getByRole("button", { name: "Bekræft ændring", exact: true }).click();
    await page.getByRole("dialog").waitFor({ state: "hidden" });
    for (const member of ["alpha", "beta"]) {
      await page.getByLabel("Medlemmets dimension", { exact: true }).fill(dimension);
      await page.getByLabel("Medlems-id", { exact: true }).fill(member);
      await page.getByLabel("Medlemsnavn", { exact: true }).fill(`Synthetic ${member}`);
      await page.getByRole("button", { name: "Opret medlem", exact: true }).click();
      await page.getByRole("dialog", { name: "Opret medlem", exact: true }).getByRole("button", { name: "Bekræft ændring", exact: true }).click();
      await page.getByRole("dialog").waitFor({ state: "hidden" });
    }
    let details = await openLine(evidence);
    await details.getByRole("button", { name: "Fordel på dimensioner", exact: true }).click();
    await details.getByLabel("Dimension 1", { exact: true }).fill(dimension);
    await details.getByLabel("Medlem 1", { exact: true }).fill("alpha");
    async function preview() {
      const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/dimensions/plan") && r.request().method() === "POST"), details.getByRole("button", { name: "Validér revideret plan", exact: true }).click()]);
      if (!response.ok()) throw new Error(`Allocation preview: ${await response.text()}`);
      const plan = (await response.json()).plan as DimensionPlan;
      const { planHash, ...facts } = plan;
      if (createHash("sha256").update(canonical(facts)).digest("hex") !== planHash) throw new Error("Allocation hash does not bind exact reviewed facts");
      await details.getByText(planHash, { exact: true }).waitFor();
      return plan;
    }
    const first = await preview();
    if (first.journalLineId !== lineId || first.lineAmountMinor !== Math.round(Math.abs(entry.lines.find(l => l.journalLineId === lineId)!.debit - entry.lines.find(l => l.journalLineId === lineId)!.credit) * 100)) throw new Error("Preview changed immutable line identity or amount");
    if (await details.getByRole("button", { name: "Anvend godkendt fordeling", exact: true }).isEnabled()) throw new Error("Allocation skipped review");
    await details.getByRole("checkbox", { name: "Jeg har gennemgået den præcise plan.", exact: true }).check();
    await details.getByRole("button", { name: "Anvend godkendt fordeling", exact: true }).click();
    await page.getByRole("dialog", { name: "Anvend dimensionsklassifikation", exact: true }).getByRole("button", { name: "Anvend tildeling", exact: true }).click();
    await page.getByRole("dialog").waitFor({ state: "hidden" });
    const original = (await history(lineId))[0];
    if (!original || original.plan_hash !== first.planHash) throw new Error("Reviewed allocation not committed");
    details = await openLine(evidence);
    await details.getByRole("button", { name: "Gennemgå og ret", exact: true }).click();
    await details.getByLabel("Medlem 1", { exact: true }).fill("beta");
    const replacement = await preview();
    const bad = await page.request.post(`${url}/dimensions/replace`, { headers, data: { journalLineId: lineId, allocations: replacement.allocations, source: "reviewed", planHash: replacement.planHash, expectedAssignmentId: original.id, reason: " ", confirm: true } });
    if (bad.status() !== 422) throw new Error("Unreasoned replacement accepted");
    await details.getByRole("checkbox", { name: "Jeg har gennemgået den præcise plan.", exact: true }).check();
    await details.getByRole("button", { name: "Erstat nuværende tildeling atomisk", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Erstat dimensionsklassifikation", exact: true });
    await dialog.getByLabel("Begrundelse", { exact: true }).fill(reason);
    await dialog.getByRole("button", { name: "Erstat tildeling", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
    evidence.history = await history(lineId);
    if (evidence.history.length !== 3 || evidence.history[0].id !== original.id || evidence.history[1].supersedes_assignment_id !== original.id || evidence.history[1].reason !== reason || evidence.history[2].plan_hash !== replacement.planHash) throw new Error("Atomic replacement history incomplete");
    const stale = await page.request.post(`${url}/dimensions/replace`, { headers, data: { confirm: true, journalLineId: lineId, expectedAssignmentId: original.id, allocations: first.allocations, source: "reviewed", planHash: first.planHash, reason } });
    if (stale.status() !== 422 || !(await stale.text()).includes("CURRENT_ASSIGNMENT_CONFLICT")) throw new Error("Stale replacement accepted");
    if ((await page.request.get(`${base}/api/companies/unselected/dimensions`)).status() !== 404) throw new Error("Dimension read crossed selected company");
    if (JSON.stringify((await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries) !== JSON.stringify(evidence.journal)) throw new Error("Classification changed journal entries, amounts or numbering");
    evidence.definitions = (await get("dimensions")).definitions.filter((d: { dimension_id: string }) => d.dimension_id === dimension);
    evidence.members = (await get(`dimensions/members?dimensionId=${dimension}`)).members;
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(saved, JSON.stringify(evidence));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  if (JSON.stringify(await history(evidence.lineId)) !== JSON.stringify(evidence.history)) throw new Error("Immutable allocation history changed after restart");
  const masterDefinitions = (await get("dimensions")).definitions.filter((d: { dimension_id: string }) => d.dimension_id === dimension);
  if (JSON.stringify(masterDefinitions) !== JSON.stringify(evidence.definitions) || JSON.stringify((await get(`dimensions/members?dimensionId=${dimension}`)).members) !== JSON.stringify(evidence.members)) throw new Error("Dimension/member creation audit changed after restart");
  const actual = (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries as Entry[];
  for (const original of evidence.journal) if (JSON.stringify(actual.find(e => e.id === original.id)) !== JSON.stringify(original)) throw new Error("Original journal entry changed");
  const details = await openLine(evidence);
  await details.getByText("Tildelingshistorik (3)", { exact: true }).click();
  await details.getByText(new RegExp(reason.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-dimensions.png", fullPage: true });
  console.log(restarted ? "PASS: dimensions allocation and reasoned immutable replacement history survive restart; original journal unchanged" : "PASS: real definition/member UI → exact hash-bound allocation → reviewed reasoned replacement/history; stale replacement refused and journal untouched");
}
