import type { APIRequestContext, Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { readFileSync, writeFileSync } from "node:fs";
type Client = (kind: "cli" | "mcp", command: string, body?: unknown, success?: boolean, id?: number) => any;
/** Explicit service customer→preview→issue→post→exact original, through real native clients. */
export async function checkNativeServiceSales(owner: Page, http: APIRequestContext, base: string, client: Client) {
  const prefix = "/api/companies/synthetic/";
  const need = (value: unknown, label: string) => { if (!value) throw Error(label); };
  async function human(resource: string, body?: unknown) {
    const r = body === undefined ? await owner.request.get(base + prefix + resource) : await owner.request.post(base + prefix + resource, { headers: { origin: base }, data: body });
    need(r.ok(), `Synthetic human sales setup ${resource} HTTP${r.status()}`); return r.json();
  }
  for (const [accountNo, name, type, normalBalance] of [["1194", "Synthetic service debtors", "asset", "debit"], ["1094", "Synthetic service revenue", "income", "credit"], ["4294", "Synthetic service output VAT", "vat", "credit"]]) await human("accounts", { accountNo, name, type, normalBalance });
  for (const [role, accountNo] of [["debtors", "1194"], ["output_vat", "4294"]]) await human("account-roles", { role, accountNo, confirm: true });
  const customerBody = { name: "Synthetic service customer", address: "Synthetic Buyer Street 94", vatOrCvr: "DK12345678" };
  const beforeContacts = (await human("contacts")).contacts;
  need(isDeepStrictEqual(client("mcp", "contacts").contacts, beforeContacts), "Service contacts canonical parity");
  const customer = client("cli", "create_customer", customerBody).customer;
  need(customer.id > 0, "Service customer created through CLI");
  const today = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  const invoice = { customerId: customer.id, issueDate: today, seller: { name: "Synthetic service seller", address: "Synthetic Seller Street 94", vatOrCvr: "DK87654321" }, lines: [{ description: "Synthetic service invoicing", quantity: 2, unitPriceExVat: 75 }] };
  const beforeInvoices = (await human("invoices")).invoices;
  const beforeJournal = (await human("journal")).journal;
  const preview = client("cli", "preview_invoice", invoice).preview;
  need(Buffer.from(preview.bytes).subarray(0, 5).toString() === "%PDF-", "Real service PDF preview");
  need(isDeepStrictEqual((await human("invoices")).invoices, beforeInvoices) && isDeepStrictEqual((await human("journal")).journal, beforeJournal), "Preview allocates neither invoice nor journal");
  const issued = client("mcp", "issue_invoice", invoice).invoice;
  need(issued.documentId > 0 && issued.netAmount === 150 && issued.vatAmount === 37.5 && issued.grossAmount === 187.5 && issued.invoiceNumber.endsWith("-0001"), "First service invoice correct and preview reserved no number");
  need(isDeepStrictEqual((await human("journal")).journal, beforeJournal), "Issuance does not post accounting");
  const body = { invoiceDocumentId: issued.documentId, revenueAccountNo: "1094", confirm: true };
  const posting = client("cli", "post_invoice", body).posting;
  need(posting.entryId > 0, "Service invoice posted once via CLI");
  client("mcp", "post_invoice", body, false);
  const duplicate = await http.post(prefix + "invoices/post", { data: body });
  need(duplicate.status() === 422, "Service business refusal retains native HTTP422 contract");
  const after = (await human("journal")).journal;
  need(after.entries.length === beforeJournal.entries.length + 1, "Duplicate post refuses without second journal");
  const listed = (await human("invoices")).invoices;
  const row = listed.invoices.find((v: { documentId: number }) => v.documentId === issued.documentId);
  need(row?.posted && row.openBalance === 187.5, "Canonical service invoice remains an open receivable");
  for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "invoices").invoices, listed), `${kind} invoice list canonical parity`);
  const stored = await owner.request.get(base + prefix + `invoices/${issued.documentId}/pdf`);
  need(stored.ok(), "Owner immutable issued original readable"); const bytes = await stored.body();
  const service = await http.get(prefix + `invoices/${issued.documentId}/pdf`);
  need(service.ok() && Buffer.from(await service.body()).equals(bytes), "Service HTTP retrieves exact stored original");
  for (const kind of ["cli", "mcp"] as const) need(Buffer.from(client(kind, "invoice_pdf", undefined, true, issued.documentId).pdf.bytes).equals(bytes), `${kind} PDF exact canonical bytes`);
  for (const [resource, envelope, command] of [["journal", "journal", "journal"], ["trial-balance", "trialBalance", "trial_balance"], ["income-statement", "incomeStatement", "income_statement"]]) need(isDeepStrictEqual(client("mcp", command)[envelope], (await human(resource))[envelope]), `Service sales ${resource} canonical parity`);
  const contacts = (await human("contacts")).contacts;
  const current = contacts.customers.find((v: { id: number }) => v.id === customer.id);
  need(current?.openBalance === 187.5 && current.openInvoiceCount === 1, "Customer service receivable reconciles");
  return { customerViaCli: true, previewNonmutating: true, issueViaMcp: true, postedOnceViaCli: true, duplicateMcpPostRefused: true, exactOriginalHttpCliMcp: true, canonicalAccountingParity: true, documentId: issued.documentId, invoiceNumber: issued.invoiceNumber, pdfSha256: createHash("sha256").update(bytes).digest("hex"), journalSha256: createHash("sha256").update(JSON.stringify(after.entries)).digest("hex"), finalJournalEntries: after.entries.length };
}

/** Read-back only after an actual second server restart and independent normal MFA. */
export async function verifyNativeServiceSalesRestart(owner: Page, base: string) {
  const path = "/tmp/rentemester-rust-reports/native-multi-company.json";
  const report = JSON.parse(readFileSync(path, "utf8")); const expected = report.serviceCompany.sales;
  const prefix = `${base}/api/companies/synthetic/`;
  const invoices = await owner.request.get(prefix + "invoices");
  const journal = await owner.request.get(prefix + "journal");
  const pdf = await owner.request.get(prefix + `invoices/${expected.documentId}/pdf`);
  if (!invoices.ok() || !journal.ok() || !pdf.ok()) throw Error("Service invoice restart read refused");
  const row = (await invoices.json()).invoices.invoices.find((v: { documentId: number }) => v.documentId === expected.documentId);
  const journalSha256 = createHash("sha256").update(JSON.stringify((await journal.json()).journal.entries)).digest("hex");
  if (!row?.posted || row.openBalance !== 187.5 || row.invoiceNo !== expected.invoiceNumber || journalSha256 !== expected.journalSha256 || createHash("sha256").update(await pdf.body()).digest("hex") !== expected.pdfSha256) throw Error("Service customer/invoice/accounting/original changed after restart");
  expected.restartReadback = true; writeFileSync(path, JSON.stringify(report, null, 2), { mode: 0o600 });
}
