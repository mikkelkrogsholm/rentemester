import type { Page } from "@playwright/test";

/** Real authenticated Rust-server evidence; no mocked hash or backup success. */
export async function checkNativeIntegrity(page: Page, base: string) {
  const endpoint = `${base}/api/companies/synthetic/integrity`;
  const response = await page.request.get(endpoint);
  if (!response.ok()) throw new Error(`Native integrity refused: HTTP ${response.status()}`);
  const { integrity } = await response.json();
  if (integrity.auditChain.available !== true || integrity.auditChain.ok !== true || integrity.auditChain.scope !== "journal_hash_chain" || !Number.isInteger(integrity.auditChain.entries)) throw new Error("Actual native journal hash-chain evidence missing");
  if (integrity.backup.available !== false || integrity.destinationsAvailable !== false) throw new Error("Unqualified native backup evidence claimed available");
  for (const key of ["ok", "backupDue", "hasActivitySinceBackup", "backupsFound", "checkedAt"]) {
    if (integrity.backup[key] !== null) throw new Error(`Unknown backup fact ${key} was fabricated`);
  }
  for (const path of [`${endpoint}?year=2026`, `${base}/api/companies/missing/integrity`]) {
    if ((await page.request.get(path)).ok()) throw new Error("Unsupported integrity selector/company accepted");
  }
  if ((await page.request.head(endpoint)).status() !== 405) throw new Error("Integrity HEAD dispatched instead of refusing");
  await page.goto(`${base}/companies/synthetic/integritet`);
  await page.getByRole("heading", { name: "Integritet", exact: true }).waitFor();
  await page.getByText("OK — den kontrollerede kæde er hel", { exact: true }).waitFor();
  await page.getByText("Backupstatus kan ikke fastslås.", { exact: true }).waitFor();
  await page.getByText("Ikke verificeret", { exact: true }).waitFor();
  if (await page.getByText("Backup forfalden.", { exact: true }).count()) throw new Error("Unknown backup was labelled overdue");
  if (await page.getByText(/Ingen destinationer konfigureret/).count()) throw new Error("Unread destination configuration was labelled empty");
  const [repeated] = await Promise.all([
    page.waitForResponse(r => r.url() === endpoint && r.request().method() === "GET"),
    page.getByRole("button", { name: "Verificér igen", exact: true }).click(),
  ]);
  if (!repeated.ok()) throw new Error("Native integrity refresh failed");
  console.log("PASS: real native journal hash-chain read/refresh; explicit unavailable backup/compliance/destinations; query/company/HEAD refusal");
}
