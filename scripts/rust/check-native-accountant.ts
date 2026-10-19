import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { readTar } from "../../src/core/tar";
const saved = "/tmp/rentemester-rust-reports/native-accountant.json";
type Evidence = { journal: unknown[]; documents: Array<{ id: number; sha256: string }>; audit: Array<{ id: number; [key: string]: unknown }>; periodStart: string; periodEnd: string };
export async function checkNativeAccountant(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const overview = (await (await page.request.get(`${url}/overview`)).json()).overview;
  if (overview.availability?.accountantExport !== true) throw new Error("Native accountant export still unavailable on company overview");
  const periodStart: string = overview.periodStart, periodEnd: string = overview.periodEnd;
  const journalBefore = (await (await page.request.get(`${url}/journal?year=${encodeURIComponent(overview.selectedYear)}`)).json()).journal.entries;
  for (const data of [{ periodStart: "2026-02-30", periodEnd, confirm: true }, { periodStart: periodEnd, periodEnd: periodStart, confirm: true }, { periodStart, periodEnd, confirm: false }]) {
    const denied = await page.request.post(`${url}/accountant-export`, { headers, data });
    if (denied.status() !== 422) throw new Error(`Invalid accountant export accepted: ${denied.status()}`);
  }
  const wrongCompany = await page.request.post(`${base}/api/companies/unselected/accountant-export`, { headers, data: { periodStart, periodEnd, confirm: true } });
  if (wrongCompany.status() !== 404) throw new Error("Accountant export crossed selected company boundary");
  await page.goto(`${base}/companies/synthetic`);
  await page.getByRole("heading", { name: "Revisor-eksport", exact: true }).waitFor();
  await page.getByLabel("Fra", { exact: true }).fill(periodStart);
  await page.getByLabel("Til", { exact: true }).fill(periodEnd);
  const [download, response] = await Promise.all([
    page.waitForEvent("download"),
    page.waitForResponse(r => r.url().endsWith("/accountant-export") && r.request().method() === "POST"),
    page.getByRole("button", { name: "Generér og download", exact: true }).click(),
  ]);
  if (!response.ok()) throw new Error(`Accountant download failed: ${await response.text()}`);
  const filename = `revisor-eksport-synthetic-${periodStart}-${periodEnd}.tar`;
  if (download.suggestedFilename() !== filename || response.headers()["content-type"] !== "application/x-tar" || response.headers()["cache-control"] !== "private, no-store") throw new Error("Accountant archive download contract is wrong");
  mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
  const path = `/tmp/rentemester-rust-reports/${restarted ? "reopened-" : ""}${filename}`;
  await download.saveAs(path);
  const entries = readTar(readFileSync(path));
  const roots = new Set(entries.map(e => e.path.split("/")[0]));
  if (roots.size !== 1 || ![...roots][0].startsWith("revisor-eksport-")) throw new Error("Accountant archive is not one portable handoff package");
  const root = [...roots][0];
  const files = new Map(entries.map(e => [e.path.slice(root.length + 1), Buffer.from(e.content)]));
  const manifest = JSON.parse(files.get("manifest.json")!.toString("utf8"));
  if (manifest.manifestVersion !== 2 || manifest.packageType !== "accountant_handoff_export" || manifest.periodStart !== periodStart || manifest.periodEnd !== periodEnd || manifest.handoffModel !== "local_export_package" || manifest.accessModel !== "no_runtime_access") throw new Error("Accountant manifest contract is wrong");
  if (manifest.provenance !== null || !manifest.limitations.includes("native_release_provenance_unavailable")) throw new Error("Accountant export invented native release provenance");
  for (const output of manifest.outputs as Array<{ path: string; sha256: string; sizeBytes: number }>) {
    const bytes = files.get(output.path);
    if (!bytes || bytes.length !== output.sizeBytes || createHash("sha256").update(bytes).digest("hex") !== output.sha256) throw new Error(`Accountant output hash/size mismatch: ${output.path}`);
  }
  if (manifest.outputs.length !== files.size - 1) throw new Error("Accountant manifest omitted an archive file");
  const journal = JSON.parse(files.get("machine-readable/journal-entries.json")!.toString("utf8"));
  const documents = JSON.parse(files.get("machine-readable/documents.json")!.toString("utf8"));
  const audit = JSON.parse(files.get("machine-readable/audit-log.json")!.toString("utf8"));
  const banks = JSON.parse(files.get("machine-readable/bank-transactions.json")!.toString("utf8"));
  if (journal.length !== journalBefore.length || !journal.length || !documents.length || !banks.length || !audit.length) throw new Error("Accountant archive has missing synthetic bookkeeping evidence");
  for (const key of ["journal-entries.csv", "trial-balance.csv", "accounts.json", "companies.json", "schema-migrations.json", "vat-report.json", "exceptions.json", "bank-journal-reconciliation-links.json"]) if (!files.get(`machine-readable/${key}`)?.length) throw new Error(`Accountant archive omitted ${key}`);
  for (const copy of manifest.copiedDocuments as Array<{ exportedPath: string; sha256: string; sizeBytes: number; documentId: number }>) {
    const bytes = files.get(copy.exportedPath);
    if (!bytes || createHash("sha256").update(bytes).digest("hex") !== copy.sha256) throw new Error("Accountant original does not match registered evidence");
    const original = await page.request.get(`${url}/documents/${copy.documentId}/file`);
    if (!original.ok() || !(await original.body()).equals(bytes)) throw new Error("Accountant original differs from actual document download");
  }
  if (!manifest.copiedDocuments.length || manifest.counts.copiedReadableDocuments !== manifest.copiedDocuments.length) throw new Error("Accountant archive contains no verified originals");
  for (const [header, count] of [["x-rentemester-journal-entries", journal.length], ["x-rentemester-documents", documents.length], ["x-rentemester-bank-transactions", banks.length]] as const) if (Number(response.headers()[header]) !== count) throw new Error("Accountant UI receipt counters disagree with archive");
  await page.getByText(`Hentede ${filename}`, { exact: false }).waitFor();
  const journalAfter = (await (await page.request.get(`${url}/journal?year=${encodeURIComponent(overview.selectedYear)}`)).json()).journal.entries;
  if (JSON.stringify(journalAfter) !== JSON.stringify(journalBefore)) throw new Error("Accountant export wrote bookkeeping effects");
  const evidence: Evidence = { journal, documents: documents.map((d: { id: number; sha256: string }) => ({ id: d.id, sha256: d.sha256 })), audit, periodStart, periodEnd };
  if (!restarted) writeFileSync(saved, JSON.stringify(evidence));
  else {
    const original: Evidence = JSON.parse(readFileSync(saved, "utf8"));
    if (JSON.stringify(original.journal) !== JSON.stringify(journal) || JSON.stringify(original.documents) !== JSON.stringify(evidence.documents)) throw new Error("Accountant bookkeeping/original evidence changed after restart");
    for (const row of original.audit) if (JSON.stringify(audit.find((a: { id: number }) => a.id === row.id)) !== JSON.stringify(row)) throw new Error("Accountant export lost original audit history after restart");
    if (!audit.some((a: { eventType: string }) => a.eventType === "accountant_handoff_export")) throw new Error("Accountant export audit event did not survive restart");
  }
  await page.screenshot({ path: "/tmp/rentemester-rust-accountant.png", fullPage: true });
  console.log(restarted ? "PASS: accountant browser download reopens after restart with original journal/documents/audit preserved" : "PASS: Overblik → Revisor-eksport browser download → portable manifest/JSON/CSV/VAT/audit and hash-verified original documents; period, confirmation and company guards hold; journal unchanged");
}
