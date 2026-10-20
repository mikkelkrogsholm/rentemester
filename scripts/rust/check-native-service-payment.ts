import type { APIRequestContext, Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
type Client = (kind: "cli" | "mcp", command: string, body?: unknown, success?: boolean, id?: number) => any;
/** Continue the same issued invoice to a canonical, bank-linked zero receivable. */
export async function checkNativeServicePayment(owner: Page, http: APIRequestContext, base: string, client: Client, documentId: number, customerId: number, grant: (role: string) => Promise<unknown>) {
  const prefix = "/api/companies/synthetic/";
  const need = (value: unknown, label: string) => { if (!value) throw Error(label); };
  async function human(resource: string, body?: unknown) {
    const r = body === undefined ? await owner.request.get(base + prefix + resource) : await owner.request.post(base + prefix + resource, { headers: { origin: base }, data: body });
    need(r.ok(), `Synthetic owner bank setup ${resource} HTTP${r.status()}`); return r.json();
  }
  await human("accounts", { accountNo: "1797", name: "Synthetic service receipt bank", type: "asset", normalBalance: "debit" });
  const account = (await human("bank-accounts", { name: "Synthetic service receipts", slug: "service-receipts", currency: "DKK", ledgerAccountNo: "1797" })).bankAccount;
  need(account.id > 0, "Owner created explicitly mapped bank account");
  await human("accounts", {accountNo:"1198",name:"Synthetic replacement debtor mapping",type:"asset",normalBalance:"debit"});
  await human("account-roles", {role:"debtors",accountNo:"1198",confirm:true});
  const beforeAccounts = (await human("bank-accounts")).bankAccounts;
  for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "bank_accounts").bankAccounts, beforeAccounts), `${kind} bank accounts canonical parity`);
  const forbiddenAdmin = await http.post(prefix + "bank-accounts", { data: {name:"Forbidden service account",currency:"DKK",ledgerAccountNo:"1797"} });
  need(forbiddenAdmin.status() === 403 && (await forbiddenAdmin.json()).usage === "not_committed", "Service bank administration refused before quota");
  const today = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  const bytes = Buffer.from(`\uFEFFdate,text,amount,currency,reference,balance_after\r\n${today},Synthetic Group97 invoice receipt,187.50,DKK,synthetic-service-payment-97,187.50\r\n`, "utf8");
  const sourceSha256 = createHash("sha256").update(bytes).digest("hex");
  const body = { confirm: true, csvBase64: bytes.toString("base64"), account: "service-receipts" };
  const journalBefore = (await human("journal")).journal;
  const bankBefore = (await human("bank")).bank;
  await grant("reader");
  const denied = await http.post(prefix + "bank/import", { data: body });
  need(denied.status() === 403 && (await denied.json()).usage === "not_committed", "Reader import refused before quota");
  client("cli", "import_bank", body, false);
  need(isDeepStrictEqual((await human("bank")).bank, bankBefore), "Reader import leaves bank unchanged");
  await grant("bookkeeper");
  const imported = client("cli", "import_bank", body).import;
  need(imported.imported === 1 && imported.sourceFileHash === sourceSha256 && imported.bankAccountId === account.id, "CLI imported exactly one mapped original row");
  need(isDeepStrictEqual((await human("journal")).journal, journalBefore), "Bank import creates no journal");
  const importedBank = (await human("bank")).bank;
  const matches = importedBank.transactions.filter((row: {reference?: string}) => row.reference === "synthetic-service-payment-97");
  need(matches.length === 1 && importedBank.transactions.length === bankBefore.transactions.length + 1, "Exactly one unique canonical incoming bank row");
  const row = matches[0];
  need(row.amount === 187.5 && row.sourceFileHash === sourceSha256 && row.reconciliationStatus === "unmatched", "Exact source hash and unmatched receipt retained");
  const duplicateImport = client("mcp", "import_bank", body).import;
  need(duplicateImport.imported === 0 && duplicateImport.skippedDuplicates === 1, "Exact original reimport deduplicates across clients");
  need(isDeepStrictEqual((await human("bank")).bank, importedBank) && isDeepStrictEqual((await human("journal")).journal, journalBefore), "Deduplication preserves canonical row identity and journal");
  const settle = { invoiceDocumentId: documentId, bankTransactionId: row.id, confirm: true };
  await grant("reader");
  const deniedSettle = await http.post(prefix + "invoices/settle", { data: settle });
  need(deniedSettle.status() === 403 && (await deniedSettle.json()).usage === "not_committed", "Reader settlement refused before quota");
  await grant("bookkeeper");
  const payment = client("mcp", "settle_invoice", settle).settlement;
  need(payment.paymentId > 0 && payment.entryId > 0 && payment.openBalance === 0, "MCP settles the exact invoice once to zero debtor");
  const after = (await human("journal")).journal;
  need(after.entries.length === journalBefore.entries.length + 1, "Exactly one payment journal added");
  const entry = after.entries.find((v: {id:number})=>v.id===payment.entryId);
  need(entry?.lines.length === 2 && entry.lines.some((line:{accountNo:string;debit:number;credit:number})=>line.accountNo==="1797" && line.debit===187.5 && line.credit===0) && entry.lines.some((line:{accountNo:string;debit:number;credit:number})=>line.accountNo==="1194" && line.debit===0 && line.credit===187.5), "Payment debits mapped bank and credits invoice's original debtor without new VAT/revenue");
  const bankAfter = (await human("bank")).bank;
  const matched = bankAfter.transactions.find((v:{id:number})=>v.id===row.id);
  need(matched?.reconciliationStatus === "matched" && matched.journalEntryNo === entry.entryNo, "Imported receipt links exact payment journal");
  need(bankAfter.actualBalance===187.5 && bankAfter.bookedBalance===187.5 && bankAfter.difference===0,"Statement balance reconciles with booked receipt");
  client("cli", "settle_invoice", settle, false);
  const duplicateSettle = await http.post(prefix + "invoices/settle", {data:settle});
  need(duplicateSettle.status() === 422 && (await duplicateSettle.json()).business === "not_committed", "Duplicate settlement is explicit business refusal");
  need(isDeepStrictEqual((await human("journal")).journal, after) && isDeepStrictEqual((await human("bank")).bank, bankAfter), "Duplicate settlement adds neither journal nor bank link");
  const invoices = (await human("invoices")).invoices;
  const invoice = invoices.invoices.find((v:{documentId:number})=>v.documentId===documentId);
  const contacts = (await human("contacts")).contacts;
  const customer = contacts.customers.find((v:{id:number})=>v.id===customerId);
  need(invoice?.posted && invoice.openBalance === 0 && customer?.openBalance === 0 && customer.openInvoiceCount === 0, "Invoice and customer reconcile to zero receivable");
  for (const kind of ["cli", "mcp"] as const) {
    need(isDeepStrictEqual(client(kind, "bank").bank, bankAfter), `${kind} final bank canonical parity`);
    need(isDeepStrictEqual(client(kind, "invoices").invoices, invoices), `${kind} paid invoice canonical parity`);
    for (const [resource,envelope,command] of [["journal","journal","journal"],["trial-balance","trialBalance","trial_balance"],["income-statement","incomeStatement","income_statement"],["balance","balance","balance"]]) need(isDeepStrictEqual(client(kind,command)[envelope],(await human(resource))[envelope]),`${kind} settled ${resource} parity`);
  }
  return {ownerMappedBank:true,serviceAdminRefusedBeforeQuota:true,readerImportAndSettleRefusedBeforeQuota:true,exactOriginalCliImport:true,mcpOriginalDeduplication:true,mcpExplicitSettlement:true,duplicateSettlementRefused:true,canonicalZeroReceivable:true,historicalDebtorPreserved:true,statementReconcilesToZero:true,sourceBase64:bytes.toString("base64"),canonicalBankAndReportsCliMcp:true,sourceSha256,bankAccountId:account.id,bankTransactionId:row.id,paymentId:payment.paymentId,entryId:payment.entryId,entryNo:entry.entryNo,customerId};
}
