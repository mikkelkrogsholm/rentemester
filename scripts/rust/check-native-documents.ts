import type { Page } from "@playwright/test";

const filename = "synthetic-native-receipt.txt";
const bytes = Buffer.from("Synthetic receipt\nOffice supplies: DKK 250.00 incl. VAT 50.00\n");
export async function checkNativeDocuments(page: Page, base: string, reopened = false) {
  await page.goto(`${base}/companies/synthetic/bilag`);
  const listed = await page.request.get(`${base}/api/companies/synthetic/documents`);
  if (!listed.ok()) throw new Error(`Native document list: ${listed.status()} ${await listed.text()}`);
  await page.getByRole("heading", { name: "Bilag", exact: true }).waitFor().catch(async (error) => { console.error(await page.locator("body").innerText()); throw error; });
  console.log("Browser: document list loaded");
  if (!reopened) {
    await page.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel(/^Bilagstype/).selectOption("cash_register_receipt");
    await dialog.getByLabel("Bilagsfil", { exact: true }).setInputFiles({ name: filename, mimeType: "text/plain", buffer: bytes });
    await dialog.getByLabel("Kilde", { exact: true }).fill("Synthetic browser receipt");
    await dialog.getByLabel("Bilagsdato (valgfri)", { exact: true }).fill(new Date().toISOString().slice(0, 10));
    await dialog.getByLabel("Beløb inkl. moms (valgfri)", { exact: true }).fill("250,00");
    await dialog.getByLabel("Momsbeløb (valgfri)", { exact: true }).fill("50,00");
    await dialog.getByRole("button", { name: "Indlæs bilag", exact: true }).click();
    await dialog.getByText(/Bilaget er indlæst som DOC-/).waitFor().catch(async (error) => { await page.screenshot({ path: "/tmp/rm-native-document-error.png", fullPage: true }); console.error(await dialog.innerText()); throw error; });
    await dialog.getByRole("button", { name: "Luk", exact: true }).click();
  }
  const persisted = await (await page.request.get(`${base}/api/companies/synthetic/documents`)).json();
  const document = persisted.documents.documents.find((doc: { filename: string }) => doc.filename === filename);
  if (!document || document.amountIncVat !== 250) throw new Error("Receipt metadata not retained");
  const row = page.getByRole("row").filter({ hasText: document.documentNo });
  await row.waitFor();
  const href = await row.getByRole("link", { name: "Åbn bilag", exact: true }).getAttribute("href");
  if (!href) throw new Error("Receipt original has no URL");
  const original = await page.request.get(`${base}${href}`);
  if (!original.ok() || !(await original.body()).equals(bytes) || !original.headers()["content-type"].startsWith("text/plain")) throw new Error("Receipt original changed or cannot be read");
  if (!reopened) {
    const input = { confirm: true, fileName: filename, fileBase64: bytes.toString("base64"), metadata: { source: "Synthetic browser receipt", documentType: "cash_register_receipt" } };
    const headers = { origin: base, "content-type": "application/json" };
    const duplicate = await page.request.post(`${base}/api/companies/synthetic/documents/ingest`, { headers, data: input });
    if (duplicate.status() !== 422 || !(await duplicate.text()).includes("duplicate document content")) throw new Error("Duplicate document was not refused");
    const mismatch = await page.request.post(`${base}/api/companies/synthetic/documents/ingest`, { headers, data: { ...input, fileName: "invalid.pdf" } });
    if (mismatch.status() !== 422 || !(await mismatch.text()).includes("does not match extension")) throw new Error("Mismatched file type was not refused");
    const anonymous = await page.context().browser()!.newContext();
    try {
      const denied = await anonymous.request.get(`${base}${href}`);
      if (denied.status() !== 401) throw new Error("Original accessible without authentication");
    } finally { await anonymous.close(); }
  }
  await page.screenshot({ path: process.env.RM_NATIVE_DOCUMENT_SCREENSHOT ?? "/tmp/rentemester-rust-documents.png", fullPage: true });
  console.log(reopened ? "PASS: receipt metadata and exact original persist after Rust server restart" : "PASS: real browser → Rust receipt upload, list, exact original; duplicate, MIME mismatch and anonymous read refused");
}
