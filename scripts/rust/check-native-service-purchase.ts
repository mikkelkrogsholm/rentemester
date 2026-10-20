import { request, type APIRequestContext, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

type Reply = { status: number; body: any };
/** Owned synthetic required-scanner purchase proof. Root controls provider hold/restart/login. */
export async function createServicePurchaseProof(ownerPage: Page, base: string, work: string) {
  const prefix = "/api/companies/synthetic/";
  const today = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  const keyFile = join(work, "service-purchase.key");
  const account = { expense: "6101", vat: "4101", creditor: "3101", replacement: "3102", bank: "1701" };
  let key = "", serviceId = "", credentialId = "", companyName = "";
  let http: APIRequestContext | undefined, pending: Promise<Reply> | undefined;
  let baseline: any, stable: any, documentId = 0, payableId = 0, started = false, revoked = false, finished = false;
  const need = (value: unknown, label: string) => { if (!value) throw Error(label); };
  const hash = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
  const original = (kind: "cli" | "held") => Buffer.from(`Synthetic required scanner ${kind}\nOwned harmless original, no accounting instruction.\n`, "utf8");
  const service = () => { if (!http) throw Error("Service purchase proof not initialized"); return http; };
  async function human(resource: string, body?: unknown, method = "POST") {
    const url = base + prefix + resource;
    const response = body === undefined ? await ownerPage.request.get(url)
      : await ownerPage.request.fetch(url, { method, headers: { origin: base }, data: body });
    need(response.ok(), `Owner purchase ${resource} HTTP${response.status()}`);
    return response.json();
  }
  async function admin(resource: string, body: unknown) {
    const response = await ownerPage.request.post(base + "/api/workspace/" + resource, { headers: { origin: base }, data: body });
    need(response.ok(), `Owner service ${resource} HTTP${response.status()}`);
    return response.json();
  }
  function upload(kind: "cli" | "held") {
    return { confirm: true, fileName: `synthetic-scanner-${kind}.txt`, fileBase64: original(kind).toString("base64"), metadata: {
      source: "Synthetic service purchase " + kind, documentType: "purchase_sale", issueDate: today,
      invoiceNo: "SYNTHETIC-SERVICE-PURCHASE-" + kind.toUpperCase(), deliveryDescription: "Synthetic supplied materials",
      amountIncVat: 125, vatAmount: 25, currency: "DKK",
      sender: { name: "Synthetic Supplier", address: "Synthetic Supplier Street 101", vatOrCvr: "DK12345678" },
      recipient: { name: companyName, address: "Synthetic Company Street 101", vatOrCvr: "DK87654321" },
    } };
  }
  function client(kind: "cli" | "mcp", command: string, body?: unknown, id?: number, success = true): any {
    const common = ["--server", base, "--allow-loopback-http", "--service-key-file", keyFile];
    const selectors = command === "payables" ? { status: "all", asOf: today } : {};
    let argv: string[], input: string;
    if (kind === "cli") {
      argv = ["remote", command.replaceAll("_", "-"), ...common, "--company", "synthetic",
        ...(id === undefined ? [] : ["--id", String(id)]), ...(body === undefined ? [] : ["--confirm"]),
        ...(command === "payables" ? ["--status", "all", "--as-of", today] : [])];
      input = body === undefined ? "" : JSON.stringify(body);
    } else {
      argv = common;
      input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-service-purchase", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: command, arguments: { company: "synthetic", ...selectors,
          ...(id === undefined ? {} : { id }), ...(body === undefined ? {} : { body: JSON.stringify(body), confirm: true }) } } },
      ].map((value) => JSON.stringify(value)).join("\n") + "\n";
    }
    const output = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), argv,
      { input, encoding: "utf8", timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!output.error && !output.stdout.includes(key) && !output.stderr.includes(key), `${kind} purchase transport or secret disclosure failure`);
    if (kind === "cli") {
      need(output.status === (success ? 0 : 1), `CLI purchase ${command} unexpected exit${output.status}`);
      return success ? JSON.parse(output.stdout).data : { refused: true };
    }
    need(output.status === 0, `MCP purchase ${command} process failure`);
    const reply = output.stdout.trim().split("\n").map((line) => JSON.parse(line)).find((value) => value.id === 2);
    need(reply?.result?.isError === !success && !reply.error, `MCP purchase ${command} unexpected outcome`);
    return success ? reply.result.structuredContent.data : { refused: true };
  }
  async function snapshot() {
    return { documents: (await human("documents")).documents, payables: (await human(`payables?status=all&asOf=${today}`)).payables,
      bank: (await human("bank")).bank, journal: (await human("journal")).journal,
      trialBalance: (await human("trial-balance")).trialBalance, incomeStatement: (await human("income-statement")).incomeStatement,
      balance: (await human("balance")).balance, vat: (await human("vat")).vat };
  }
  async function unchanged(expected: any) { need(isDeepStrictEqual(await snapshot(), expected), "Purchase refusal changed canonical documents/books/payables/bank"); }
  async function sourceBytes() {
    const response = await ownerPage.request.get(base + prefix + `documents/${documentId}/file`);
    need(response.ok() && (await response.body()).equals(original("cli")), "Owner exact scanned purchase original bytes");
    return { sha256: hash(original("cli")), bytes: original("cli").length, contentType: response.headers()["content-type"] };
  }
  async function parity(canonical: any) {
    for (const kind of ["cli", "mcp"] as const) {
      for (const [command, field] of [["documents", "documents"], ["payables", "payables"], ["bank", "bank"], ["journal", "journal"],
        ["trial_balance", "trialBalance"], ["income_statement", "incomeStatement"], ["balance", "balance"]]) {
        need(isDeepStrictEqual(client(kind, command)[field], canonical[field]), `${kind} purchase canonical ${field} parity`);
      }
    }
  }
  async function run() {
    need(!started, "Purchase proof must run once"); started = true;
    const fresh = { documents: (await human("documents")).documents, journal: (await human("journal")).journal,
      bank: (await human("bank")).bank, payables: (await human(`payables?status=all&asOf=${today}`)).payables };
    need(fresh.documents.documents.length === 0 && fresh.journal.entries.length === 0 && fresh.bank.transactions.length === 0 && fresh.payables.rows.length === 0, "Fresh synthetic purchase workspace required");
    await human("company", { cvr: "87654321", address: "Synthetic Company Street 101", postalCode: "1000", city: "Synthetic City", vatPeriodType: "quarter" }, "PATCH");
    companyName = (await human("company")).company.name;
    for (const row of [
      { accountNo: account.expense, name: "Synthetic purchase expense", type: "expense", normalBalance: "debit", defaultVatCode: "DK_PURCHASE_25" },
      { accountNo: account.vat, name: "Synthetic purchase VAT", type: "vat", normalBalance: "debit" },
      { accountNo: account.creditor, name: "Synthetic original creditor", type: "liability", normalBalance: "credit" },
      { accountNo: account.replacement, name: "Synthetic replacement creditor", type: "liability", normalBalance: "credit" },
      { accountNo: account.bank, name: "Synthetic purchase bank", type: "asset", normalBalance: "debit" },
    ]) await human("accounts", row);
    await human("account-roles", { role: "creditors", accountNo: account.creditor, confirm: true });
    await human("account-roles", { role: "input_vat", accountNo: account.vat, confirm: true });
    const bankAccount = (await human("bank-accounts", { name: "Synthetic purchases", slug: "service-purchases", currency: "DKK", ledgerAccountNo: account.bank })).bankAccount;
    const issued = await admin("service-principals", { displayName: "Synthetic purchase executor", operationId: randomUUID(), confirm: true });
    need(typeof issued.secret === "string" && issued.secret.length > 0, "Normal owner-issued purchase service credential");
    key = issued.secret; serviceId = issued.serviceAccountId; credentialId = issued.credentialId;
    writeFileSync(keyFile, key + "\n", { mode: 0o600, flag: "wx" });
    await admin("members/access", { userId: serviceId, action: "set-role", workspaceRole: "member" });
    await admin("members/company", { userId: serviceId, action: "grant", companySlug: "synthetic", role: "bookkeeper" });
    http = await request.newContext({ baseURL: base, extraHTTPHeaders: { origin: base, "x-rentemester-service-key": key } });
    const beforeUpload = await snapshot();
    const uploadReceipt = client("cli", "ingest_document", upload("cli")); documentId = uploadReceipt.document.id;
    need(Number.isSafeInteger(documentId) && documentId > 0, "CLI service required scan registered one document");
    const listed = client("mcp", "documents").documents;
    const document = listed.documents.find((value: any) => value.id === documentId);
    need(listed.documents.length === 1 && document?.sha256Hash === hash(original("cli")) && document.invoiceNo === upload("cli").metadata.invoiceNo, "MCP exact factual scanned document");
    const evidence = client("mcp", "document_original", undefined, documentId).original;
    need(evidence.contentType === "text/plain" && Buffer.from(evidence.bytes).equals(original("cli")), "MCP exact required-scanner original bytes");
    need(isDeepStrictEqual((await human("journal")).journal, beforeUpload.journal), "Scanned ingress creates no financial posting");
    const registration = { documentId, billDate: today, dueDate: today, expenseAccountNo: account.expense, confirm: true };
    const recognized = client("mcp", "register_payable", registration).payable; payableId = recognized.payableId;
    need(payableId > 0 && recognized.grossAmount === 125 && recognized.netAmount === 100 && recognized.vatAmount === 25, "MCP factual supplier bill recognition");
    const afterRecognition = await snapshot();
    const duplicateBill = await service().post(prefix + "payables", { data: registration });
    need(duplicateBill.status() === 422 && (await duplicateBill.json()).business === "not_committed", "Duplicate payable is a definite business refusal");
    await unchanged(afterRecognition);
    await human("account-roles", { role: "creditors", accountNo: account.replacement, confirm: true });
    const bytes = Buffer.from(`\uFEFFdate,text,amount,currency,reference,balance_after\r\n${today},Synthetic supplier payment,-125.00,DKK,synthetic-service-purchase-payment-101,-125.00\r\n`, "utf8");
    const imported = client("mcp", "import_bank", { csvBase64: bytes.toString("base64"), account: "service-purchases", confirm: true }).import;
    need(imported.imported === 1 && imported.sourceFileHash === hash(bytes) && imported.bankAccountId === bankAccount.id, "Exact BOM/CRLF mapped outgoing source import");
    const bank = (await human("bank")).bank;
    const row = bank.transactions.find((value: any) => value.reference === "synthetic-service-purchase-payment-101");
    need(bank.transactions.length === 1 && row?.amount === -125 && row.sourceFileHash === hash(bytes), "One exact outgoing bank transaction");
    const paymentBody = { bankTransactionId: row.id, confirm: true };
    const payment = client("cli", "pay_payable", paymentBody, payableId).payment;
    need(payment.paymentId > 0 && payment.openBalance === 0, "CLI service payment clears payable");
    stable = await snapshot();
    need(stable.journal.entries.length === 2 && stable.trialBalance.balanced && stable.incomeStatement.totalExpense === 100 && stable.vat.inputVat === 25, "Recognition/payment has exact expense/VAT and balanced two-entry journal");
    const entry = stable.journal.entries.find((value: any) => value.id === payment.journalEntryId);
    need(entry?.lines.length === 2 && entry.lines.some((line: any) => line.accountNo === account.creditor && line.debit === 125 && line.credit === 0)
      && entry.lines.some((line: any) => line.accountNo === account.bank && line.debit === 0 && line.credit === 125), "Payment relieves original creditor, not replacement mapping, without repeated expense/VAT");
    need(stable.trialBalance.rows.find((value: any) => value.accountNo === account.creditor)?.balance === 0
      && !stable.journal.entries.some((value: any) => value.lines.some((line: any) => line.accountNo === account.replacement)), "Original creditor reconciles to zero and replacement receives no posting");
    const paid = stable.payables.rows.find((value: any) => value.payableId === payableId);
    need(paid?.status === "paid" && paid.openBalance === 0 && stable.payables.totalOpenBalance === 0, "Canonical paid payable has zero open balance");
    need(stable.bank.transactions[0].reconciliationStatus === "matched" && stable.bank.bookedBalance === -125 && stable.bank.actualBalance === -125 && stable.bank.difference === 0, "Source statement and bank journal reconcile exactly");
    const duplicatePayment = await service().post(prefix + `payables/${payableId}/pay`, { data: paymentBody });
    need(duplicatePayment.status() === 422 && (await duplicatePayment.json()).business === "not_committed", "Duplicate payment is a definite business refusal");
    await unchanged(stable); await parity(stable);
    return { serviceAccountId: serviceId, credentialId, documentId, payableId, requiredScanCliIngest: true, mcpOriginalAndList: true,
      mcpBillRecognition: true, cliBankPayment: true, originalCreditorPreserved: true, duplicateBillAndPaymentRefused: true,
      canonicalClientParity: true, documentSource: await sourceBytes(), bankSourceBase64: bytes.toString("base64"), bankSourceSha256: hash(bytes), canonical: stable };
  }
  async function holdBegin() {
    need(stable && !pending && !revoked, "Paid purchase baseline required before held scan");
    baseline = await snapshot();
    pending = service().post(prefix + "documents/ingest", { data: upload("held"), timeout: 45000 })
      .then(async (response) => ({ status: response.status(), body: await response.json() }));
    // Attach a rejection handler immediately while root waits for provider arrival/revokes.
    void pending.catch(() => {});
    return { requestStarted: true, heldSourceSha256: hash(original("held")), heldSourceBase64: original("held").toString("base64") };
  }
  async function revoke() {
    need(pending && !revoked, "Held service upload required before revocation");
    const result = await admin("service-principals/revoke", { serviceAccountId: serviceId, credentialId, operationId: randomUUID(), confirm: true });
    revoked = true;
    return { revocationCompletedBeforeProviderRelease: true, operationId: result.operationId, serviceAccountId: serviceId, credentialId };
  }
  async function holdFinish() {
    need(pending && revoked && !finished, "Revoked held upload required");
    let result: Reply;
    try { result = await pending!; } catch { throw Error("Held service upload outcome unavailable; no retry"); }
    need(result.status === 401 && result.body?.ok === false && result.body.code === "invalid_credentials", "Successful held scan must refuse revoked credential before registration");
    await unchanged(baseline); await sourceBytes(); finished = true;
    return { refusal: result, noDocumentPublication: true, unchangedBooksAndPayables: true, noAutomaticRetry: true };
  }
  async function reopen() {
    need(finished && revoked, "Held revocation must finish before normal reopened verification");
    await unchanged(stable); const source = await sourceBytes();
    need((await service().get(prefix + "documents")).status() === 401, "Revoked service still refused after restart");
    client("cli", "payables", undefined, undefined, false);
    return { exactPaidStateAfterRestart: true, exactOriginalAfterRestart: source, revokedService401: true, noRescanRequested: true, canonical: stable };
  }
  return { run, holdBegin, revoke, holdFinish, reopen };
}
