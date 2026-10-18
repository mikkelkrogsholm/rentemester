import type { Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
const saved = "/tmp/rentemester-rust-reports/native-customer-edit.json";

export async function checkNativeCustomerEdit(page: Page, base: string, reopened = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  let evidence: { id: number; invoiceId: number; pdf: string; originalId: number; original: string };
  if (!reopened) {
    const contacts = await (await page.request.get(`${url}/contacts`)).json();
    const customer = contacts.contacts.customers.find((c: { name: string }) => c.name === "Synthetic Customer");
    const invoices = await (await page.request.get(`${url}/invoices`)).json();
    const invoice = invoices.invoices.invoices.find((i: { invoiceNo: string }) => i.invoiceNo.endsWith("-0001"));
    const documents = await (await page.request.get(`${url}/documents`)).json();
    // The invoice document itself is the immutable JSON original.
    const pdf = await page.request.get(`${url}/invoices/${invoice.documentId}/pdf`);
    const original = await page.request.get(`${url}/documents/${invoice.documentId}/file`);
    if (!original.ok() || !pdf.ok() || !documents.ok) throw new Error("Missing original invoice evidence");
    evidence = { id: customer.id, invoiceId: invoice.documentId, pdf: (await pdf.body()).toString("base64"), originalId: invoice.documentId, original: (await original.body()).toString("base64") };
    await page.goto(`${base}/companies/synthetic/kontakter`);
    await page.getByRole("button", { name: "Redigér Synthetic Customer", exact: true }).click();
    let dialog = page.getByRole("dialog");
    await dialog.getByLabel("Navn", { exact: true }).fill("Synthetic Revised Customer");
    await dialog.getByLabel("Adresse", { exact: true }).fill("Synthetic Updated Street 9");
    await dialog.getByLabel("Telefon", { exact: true }).fill("12345678");
    await dialog.getByLabel("E-mail", { exact: true }).fill("updated@example.invalid");
    await dialog.getByLabel("Betalingsfrist (dage)", { exact: true }).fill("7");
    await dialog.getByLabel("Noter", { exact: true }).fill("Synthetic changed customer details");
    const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith(`/customers/${customer.id}`) && r.request().method() === "PATCH"), dialog.getByRole("button", { name: "Gem ændringer", exact: true }).click()]);
    if (!response.ok()) throw new Error(`Customer edit failed: ${await response.text()}`);
    await dialog.waitFor({ state: "hidden" });
    await page.getByRole("button", { name: "Redigér Synthetic Revised Customer", exact: true }).click();
    dialog = page.getByRole("dialog");
    await dialog.getByLabel("Navn", { exact: true }).fill("Synthetic Customer");
    await dialog.getByLabel("Telefon", { exact: true }).fill("");
    await dialog.getByRole("button", { name: "Gem ændringer", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
    for (const data of [{ name: " " }, { defaultCurrency: null }, { unknownField: true }]) {
      const refused = await page.request.patch(`${url}/customers/${customer.id}`, { headers, data });
      if (refused.status() !== 422) throw new Error("Invalid customer patch accepted");
    }
    // An omitted field survives a partial API patch; null deliberately clears it.
    const patch = await page.request.patch(`${url}/customers/${customer.id}`, { headers, data: { notes: null } });
    if (!patch.ok()) throw new Error("Partial customer patch failed");
    writeFileSync(saved, JSON.stringify(evidence));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const contacts = await (await page.request.get(`${url}/contacts`)).json();
  const customer = contacts.contacts.customers.find((c: { id: number }) => c.id === evidence.id);
  if (!customer || customer.name !== "Synthetic Customer" || customer.address !== "Synthetic Updated Street 9" || customer.email !== "updated@example.invalid" || customer.phone !== null || customer.notes !== null || customer.paymentTermsDays !== 7 || customer.openBalance !== 0) throw new Error(`Customer edit not retained: ${JSON.stringify(customer)}`);
  const pdf = await page.request.get(`${url}/invoices/${evidence.invoiceId}/pdf`);
  const original = await page.request.get(`${url}/documents/${evidence.originalId}/file`);
  if (!(await pdf.body()).equals(Buffer.from(evidence.pdf, "base64")) || !(await original.body()).equals(Buffer.from(evidence.original, "base64"))) throw new Error("Customer edit rewrote original invoice evidence");
  const before = await (await page.request.get(`${url}/invoices`)).json();
  const today = new Date().toISOString().slice(0, 10);
  const due = new Date(`${today}T12:00:00Z`); due.setUTCDate(due.getUTCDate() + 7);
  const preview = await page.request.post(`${url}/invoices/preview`, { headers, data: { issueDate: today, customerId: evidence.id, seller: { vatOrCvr: "DK87654321" }, lines: [{ description: "Synthetic future sale", quantity: 1, unitPriceExVat: 100 }] } });
  if (!preview.ok()) throw new Error(`Updated customer preview failed: ${await preview.text()}`);
  const text = execFileSync("pdftotext", ["-", "-"], { input: await preview.body() }).toString("utf8");
  if (!text.includes("Synthetic Updated Street 9") || !text.includes(due.toISOString().slice(0, 10))) throw new Error(`New invoice ignores updated customer: ${text}`);
  const after = await (await page.request.get(`${url}/invoices`)).json();
  if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error("Customer edit preview changed issued invoices");
  await page.goto(`${base}/companies/synthetic/kontakter`);
  await page.getByRole("button", { name: "Redigér Synthetic Customer", exact: true }).click();
  await page.getByRole("dialog").getByLabel("Adresse", { exact: true }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-customer-edit.png", fullPage: true });
  await page.getByRole("dialog").getByRole("button", { name: "Annullér", exact: true }).click();
  console.log(reopened ? "PASS: updated customer and cleared fields persist after restart; future invoice uses new address/terms, immutable originals unchanged" : "PASS: browser edit/rename customer → clear phone → partial patch → new invoice preview uses changed address/terms; original invoice/PDF unchanged and invalid patches refused");
}
