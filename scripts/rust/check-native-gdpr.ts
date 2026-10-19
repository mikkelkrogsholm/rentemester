import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { GdprSubjectExport } from "../../app/src/lib/types/gdpr";
const saved = "/tmp/rentemester-rust-reports/native-gdpr.json";
const free = { name: "Synthetic GDPR Free Subject", cvr: "SYN-GDPR-FREE" };
const retained = { name: "Synthetic GDPR Retained Subject", cvr: "SYN-GDPR-RETAINED" };
type Evidence = { freeRecords: GdprSubjectExport["records"]; originalPath: string; originalBytes: string; journal: unknown[]; year: string; retainedDeadline: string; customerId: number; maintenanceId?: number };
// Run after supplier/customer maintenance and before dashboard/accountant baseline snapshots.
// Scoped erased-identity fences remain active; unrelated maintenance is supported.
export async function checkNativeGdpr(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`, headers = { origin: base, "content-type": "application/json" };
  async function get(path: string) { const r = await page.request.get(`${url}/${path}`); if (!r.ok()) throw new Error(`GDPR dependency ${path} unavailable`); return r.json(); }
  async function report(subject: typeof free): Promise<GdprSubjectExport> {
    const response = await page.request.post(`${url}/gdpr/export`, { headers, data: { ...subject, confirm: true } });
    if (!response.ok()) throw new Error(`GDPR export refused (${response.status()})`);
    return (await response.json()).gdpr.export;
  }
  async function lookupUi(subject: typeof free) {
    await page.goto(`${base}/companies/synthetic/gdpr`);
    await page.getByLabel("CVR", { exact: true }).fill(subject.cvr);
    await page.getByLabel("Navn", { exact: true }).fill(subject.name);
    const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/gdpr/export") && r.request().method() === "POST"), page.getByRole("button", { name: "Hent indsigtsrapport", exact: true }).click()]);
    if (!response.ok()) throw new Error("GDPR UI lookup failed");
    await page.getByRole("heading", { name: /^Indsigtsrapport/ }).waitFor();
  }
  let evidence: Evidence;
  if (!restarted) {
    const journal = (await get("journal")).journal;
    const retention = (await get("retention")).retention;
    for (const domain of ["documents", "journal_entries", "bank_transactions"]) if (!retention.report.rows.some((r: { table: string }) => r.table === domain)) throw new Error("Retention omitted a required domain");
    let customerId = 0;
    for (const subject of [free, retained]) {
      const created = await page.request.post(`${url}/customers`, { headers, data: { name: subject.name, vatOrCvr: subject.cvr, address: "Synthetic GDPR Street", email: "synthetic-gdpr@example.invalid" } });
      if (!created.ok()) throw new Error("Synthetic GDPR customer setup failed");
      if (subject === free) customerId = (await created.json()).customer.id;
    }
    const bytes = Buffer.from("Synthetic GDPR retained original; immutable evidence.\n");
    const uploaded = await page.request.post(`${url}/documents/ingest`, { headers, data: { confirm: true, fileName: "synthetic-gdpr-retained.txt", fileBase64: bytes.toString("base64"), metadata: { source: "Synthetic GDPR retained fixture", documentType: "cash_register_receipt", issueDate: retention.report.asOf, sender: { name: retained.name, vatOrCvr: retained.cvr, address: "Synthetic retained address" } } } });
    if (!uploaded.ok()) throw new Error("Synthetic retained original setup failed");
    const documentId = (await uploaded.json()).document.id;
    const documents = (await get("documents")).documents.documents;
    const document = documents.find((d: { id: number }) => d.id === documentId);
    if (!document) throw new Error("Synthetic retained original missing");
    // Obtain the actual original route from the existing documents UI.
    await page.goto(`${base}/companies/synthetic/bilag`);
    const docRow = page.getByRole("row").filter({ hasText: document.documentNo });
    const originalPath = await docRow.getByRole("link", { name: "Åbn bilag", exact: true }).getAttribute("href");
    if (!originalPath) throw new Error("Retained original has no readable route");
    const before = await report(retained);
    const record = before.records.find(r => r.source === "documents" && r.sourceRowId === documentId);
    if (!record?.underRetention || record.erasable || !record.retainUntil) throw new Error("Current original lost mandatory retention");
    for (const data of [{ ...free, confirm: false }, { cvr: free.cvr, name: retained.name, confirm: true }]) {
      const denied = await page.request.post(`${url}/gdpr/export`, { headers, data }); if (denied.status() !== 422) throw new Error("Unconfirmed or mismatched GDPR scope accepted");
    }
    const future = await page.request.post(`${url}/gdpr/erase`, { headers, data: { ...retained, confirm: true, asOf: "9999-12-31" } });
    if (future.status() !== 400) throw new Error("Erasure accepted a caller-controlled date");
    const denied = await page.request.post(`${url}/gdpr/erase`, { headers, data: { ...retained, confirm: true } });
    if (!denied.ok()) throw new Error("Retention review did not return a structured result");
    const retainedResult = (await denied.json()).gdprErasure;
    if (retainedResult.erasedCount !== 0 || retainedResult.refusedCount < 2) throw new Error("Retained master/original records were not refused");
    await lookupUi(retained);
    await page.getByText("Under bogføringspligt", { exact: true }).first().waitFor();
    if (await page.getByRole("button", { name: /^Anonymisér de 0/ }).isEnabled()) throw new Error("Retained subject advertised erasable rows");
    await lookupUi(free);
    const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/gdpr/erase") && r.request().method() === "POST"), (async () => {
      await page.getByRole("button", { name: /^Anonymisér de [1-9]/ }).click();
      await page.getByRole("dialog", { name: "Anonymisér personoplysninger", exact: true }).getByRole("button", { name: "Anonymisér nu", exact: true }).click();
    })()]);
    if (!response.ok() || (await response.json()).gdprErasure.erasedCount < 1) throw new Error("Confirmed GDPR overlay was not committed");
    await page.getByRole("dialog").waitFor({ state: "hidden" });
    await page.getByText("[redigeret — GDPR]", { exact: true }).first().waitFor();
    const after = await report(free);
    if (!after.records.some(r => r.source === "customers" && r.erased && r.personalData.email === null)) throw new Error("Customer overlay missing");
    if (after.coverage?.complete !== false) throw new Error("Partial GDPR coverage was not disclosed");
    evidence = { freeRecords: after.records.filter(r => r.source !== "audit_log"), originalPath, originalBytes: bytes.toString("base64"), journal: journal.entries, year: journal.selectedYear, retainedDeadline: record.retainUntil, customerId };
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(saved, JSON.stringify(evidence));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const after = await report(free);
  if (JSON.stringify(after.records.filter(r => r.source !== "audit_log")) !== JSON.stringify(evidence.freeRecords)) throw new Error("Subject overlay changed after restart");
  const current = await report(retained);
  if (!current.records.some(r => r.source === "documents" && r.underRetention && !r.erased && r.retainUntil === evidence.retainedDeadline)) throw new Error("Retained document changed after restart");
  const original = await page.request.get(`${base}${evidence.originalPath}`);
  if (!original.ok() || !(await original.body()).equals(Buffer.from(evidence.originalBytes, "base64"))) throw new Error("Protected original bytes changed");
  const actual = (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
  if (JSON.stringify(actual) !== JSON.stringify(evidence.journal)) throw new Error("GDPR changed journal entries or amounts");
  const guarded = await page.request.patch(`${url}/customers/${evidence.customerId}`, { headers, data: { name: "Synthetic identity bypass" } });
  if (guarded.status() !== 422) throw new Error("Prior erasure identity guard was bypassed");
  if (!restarted) {
    const contacts = (await get("contacts")).contacts;
    const erased = contacts.customers.find((c: { id: number }) => c.id === evidence.customerId);
    if (!erased || erased.name !== "[redigeret — GDPR]" || erased.email !== null || erased.vatOrCvr !== null) throw new Error("Erased master fields resurfaced in native contacts");
    const retired = await page.request.delete(`${url}/customers/${evidence.customerId}`, { headers, data: { confirm: true } });
    if (!retired.ok()) throw new Error("Eligible erased contact could not be retired safely");
    const created = await page.request.post(`${url}/customers`, { headers, data: { name: "Synthetic post-erasure unrelated", vatOrCvr: "SYN-GDPR-UNRELATED", email: "unrelated@example.invalid" } });
    if (!created.ok()) throw new Error("Unrelated post-erasure contact remained globally blocked");
    evidence.maintenanceId = (await created.json()).customer.id;
    if (evidence.maintenanceId! <= evidence.customerId) throw new Error("Erased contact row identity was recycled");
    const updated = await page.request.patch(`${url}/customers/${evidence.maintenanceId}`, { headers, data: { address: "Synthetic post-erasure updated address" } });
    if (!updated.ok()) throw new Error("Unrelated post-erasure update failed");
    const reintroduced = await page.request.post(`${url}/customers`, { headers, data: { name: "Synthetic attempted identity reintroduction", vatOrCvr: free.cvr } });
    if (reintroduced.status() !== 422) throw new Error("Erased stable identity was reintroduced");
    writeFileSync(saved, JSON.stringify(evidence));
  }
  const stillFenced = await page.request.post(`${url}/customers`, { headers, data: { name: "Synthetic alternate erased label", vatOrCvr: free.cvr } });
  if (stillFenced.status() !== 422) throw new Error("Persistent stable erasure alias stopped fencing identity recreation");
  const maintained = (await get("contacts")).contacts.customers;
  if (maintained.some((c: { id: number }) => c.id === evidence.customerId) || !maintained.some((c: { id: number; address: string }) => c.id === evidence.maintenanceId && c.address === "Synthetic post-erasure updated address")) throw new Error("Post-erasure maintenance or retired identity changed after restart");
  await lookupUi(free);
  await page.getByText("[redigeret — GDPR]", { exact: true }).first().waitFor();
  await page.getByText(/Originalfiler, øvrige kontaktfelter/).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-gdpr.png", fullPage: true });
  const retentionPageErrors: string[] = [];
  const authResponses: Array<{ path: string; status: number }> = [];
  const recordAuthResponse = (response: import("@playwright/test").Response) => {
    const path = new URL(response.url()).pathname;
    if (path === "/api/auth/get-session" || path === "/api/me") authResponses.push({ path, status: response.status() });
  };
  const recordRetentionError = (error: Error) => retentionPageErrors.push(error.message);
  page.on("pageerror", recordRetentionError);
  page.on("response", recordAuthResponse);
  try {
    const [response] = await Promise.all([
      page.waitForResponse(r => r.url() === `${url}/retention` && r.request().method() === "GET"),
      page.goto(`${base}/companies/synthetic/retention`),
    ]);
    if (!response.ok()) throw new Error(`Retention UI HTTP ${response.status()}: ${await response.text()}`);
    await page.getByRole("heading", { name: "Opbevaring", exact: true }).waitFor();
  } catch (error) {
    await page.screenshot({ path: "/tmp/rentemester-rust-retention-failure.png", fullPage: true }).catch(() => {});
    throw new Error(`Retention navigation failed at ${page.url()}; auth responses: ${JSON.stringify(authResponses)}; page errors: ${JSON.stringify(retentionPageErrors)}; ${error}`);
  } finally {
    page.off("pageerror", recordRetentionError);
    page.off("response", recordAuthResponse);
  }
  console.log(restarted ? "PASS: GDPR overlays persist after restart, retained originals and journal unchanged; scoped identity fences and unrelated maintenance persist" : "PASS: real retention/GDPR UI → subject export/review → confirmed overlay; retained subject refused, scope/date/confirmation fenced and partial coverage disclosed");
}
