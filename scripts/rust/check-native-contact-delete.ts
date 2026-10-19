import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
const saved = "/tmp/rentemester-rust-reports/native-contact-delete.json";
/** Real contact confirmation, refusal and persisted deletion through the Rust server. */
export async function checkNativeContactDelete(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const get = async (path: string) => {
    const response = await page.request.get(`${url}/${path}`);
    if (!response.ok()) throw new Error(`Contact dependency ${path}: ${await response.text()}`);
    return response.json();
  };
  let removed: Array<{kind: string; name: string; id: number}>;
  if (!restarted) {
    const before = (await get("journal")).journal;
    removed = [];
    for (const kind of ["customer", "vendor"]) {
      const name = `Synthetic unused ${kind} deletion`;
      const created = await page.request.post(`${url}/${kind}s`, { headers, data: { name } });
      if (!created.ok()) throw new Error(`Create removable contact: ${await created.text()}`);
      const id = (await created.json())[kind].id;
      for (const body of [{ confirm: false }, {}]) {
        const denied = await page.request.delete(`${url}/${kind}s/${id}`, { headers, data: body });
        if (denied.ok()) throw new Error("Contact deleted without explicit confirmation");
      }
      await page.goto(`${base}/companies/synthetic/kontakter`);
      await page.getByLabel("Søg kontakter", { exact: true }).fill(name);
      await page.getByRole("button", { name: `Slet ${name}`, exact: true }).click();
      const dialog = page.getByRole("dialog", { name: `Slet ${kind === "customer" ? "kunde" : "leverandør"} ${name}?`, exact: true });
      await dialog.waitFor();
      const [response] = await Promise.all([
        page.waitForResponse(r => r.url() === `${url}/${kind}s/${id}` && r.request().method() === "DELETE"),
        dialog.getByRole("button", { name: "Slet", exact: true }).click(),
      ]);
      if (!response.ok() || (await response.json())[kind].deleted !== true) throw new Error("Confirmed contact deletion failed");
      await dialog.waitFor({ state: "hidden" });
      await page.getByRole("button", { name: `Slet ${name}`, exact: true }).waitFor({ state: "hidden" });
      removed.push({ kind, name, id });
    }
    // Existing domain-client fixture has a real open invoice; deletion must preserve it.
    const contacts = (await get("contacts")).contacts;
    const invoices = (await get("invoices")).invoices.invoices;
    const invoice = invoices.find((i: {customerId?: number; openBalance: number}) => i.customerId && i.openBalance !== 0);
    if (!invoice) throw new Error("Contact refusal requires the preceding real issued-invoice journey");
    const customer = contacts.customers.find((c: {id: number}) => c.id === invoice.customerId);
    if (!customer) throw new Error("Open invoice customer missing before deletion refusal");
    const refused = await page.request.delete(`${url}/customers/${customer.id}`, { headers, data: { confirm: true } });
    if (refused.status() !== 422) throw new Error("Open invoice allowed contact deletion");
    if (JSON.stringify((await get("contacts")).contacts) !== JSON.stringify(contacts) || JSON.stringify((await get("invoices")).invoices.invoices) !== JSON.stringify(invoices)) throw new Error("Rejected contact deletion changed contacts/invoice evidence");
    if (JSON.stringify((await get("journal")).journal) !== JSON.stringify(before)) throw new Error("Contact deletion changed journal");
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(removed));
  } else removed = JSON.parse(readFileSync(saved, "utf8"));
  const contacts = (await get("contacts")).contacts;
  for (const row of removed) if (contacts[`${row.kind}s`].some((c: {name: string}) => c.name === row.name)) throw new Error("Deleted contact returned after reload/restart");
  console.log(restarted ? "PASS: contact deletions persist after native restart" : "PASS: actual contact UI confirmation/deletion, missing confirmation and open-invoice refusals, immutable journal/evidence");
}
