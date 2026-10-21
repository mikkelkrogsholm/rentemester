import { backupDestinationProof } from "./check-native-backup-destinations";
import { request, type APIRequestContext, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

type Reply = { status: number; body: any };
/** Owned synthetic required-scanner purchase proof. Root controls provider hold/restart/login. */
export async function createServicePurchaseProof(ownerPage: Page, base: string, work: string) {
  const prefix = "/api/companies/synthetic/";
  const backupProof = process.env.RM_NATIVE_BACKUP_DESTINATION_PROOF === "1" ? backupDestinationProof(ownerPage,base,work) : null;
  let backupResult: unknown;
  let today = new Date().toISOString().slice(0, 10);
  const keyFile = join(work, "service-purchase.key");
  const account = { expense: "6101", vat: "4101", creditor: "3101", replacement: "3102", bank: "1701" };
  let key = "", serviceId = "", credentialId = "", companyName = "";
  let originalRegistration: any, originalPartial: any, registerReceipt: any, partialReceipt: any, originalLists: any, receiptRestartProof: any, originalSourceRefused = false;
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
  function originalClient(kind: "cli" | "mcp", name: "register" | "pay" | "list", args: Record<string, any>, denied = false) {
    const common = ["--server", base, "--allow-loopback-http", "--service-key-file", keyFile];
    const flags: Record<string,string> = { documentId:"document-id", billDate:"bill-date", dueDate:"due-date", expenseAccount:"expense-account", vatTreatment:"vat-treatment", vendorId:"vendor-id", payableId:"payable-id", bankTransactionId:"bank-transaction-id", paymentAccount:"payment-account", asOf:"as-of", minDays:"min-days" };
    let argv: string[], input: string | undefined;
    if (kind === "cli") {
      const cliArgs = { ...args }; delete cliArgs.confirm; delete cliArgs.idempotencyKey;
      if (cliArgs.asOfDate !== undefined && cliArgs.asOf === undefined) cliArgs.asOf = cliArgs.asOfDate; delete cliArgs.asOfDate;
      argv = ["payable", name, ...common, "--company", "synthetic", "--json", ...Object.entries(cliArgs).flatMap(([k,v])=>["--"+(flags[k]??k),String(v)])];
    } else {
      argv = common;
      input = [{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-original-payable146",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"payable_"+name,arguments:{company:"synthetic",...args}}}].map(v=>JSON.stringify(v)).join("\n")+"\n";
    }
    const result = spawnSync(join(work,kind==="cli"?"rentemester-rust":"rentemester-mcp"),argv,{input,encoding:"utf8",timeout:45000,maxBuffer:16*1024*1024});
    need(!result.error && result.status === (kind==="cli"&&denied?1:0) && !result.stdout.includes(key) && !result.stderr.includes(key),"Original payable client terminal/privacy");
    if(kind==="cli"){const value=JSON.parse(result.stdout);need(value.ok===!denied&&Array.isArray(value.errors),"Original payable CLI flat envelope");return value;}
    const reply=result.stdout.trim().split("\n").map(v=>JSON.parse(v)).find(v=>v.id===2), value=reply?.result?.structuredContent;
    need(!reply?.error&&reply.result.isError===denied&&value?.ok===!denied&&Array.isArray(value.errors)&&isDeepStrictEqual(JSON.parse(reply.result.content[0].text),value),"Original payable MCP exact envelope");
    return {ok:value.ok,...(value.data??{}),errors:value.errors,...(value.appliedRules?{appliedRules:value.appliedRules}:{}),...(value.code?{code:value.code}:{})};
  }
  async function listOriginal(expectedOpen: number, expectedPaid: number) {
    const future = new Date(today+"T00:00:00Z"); future.setUTCDate(future.getUTCDate()+3); const aged=future.toISOString().slice(0,10);
    const cases: [Record<string,any>,number,number][] = [[{},1,expectedOpen],[{supplier:" SYNTHETIC SUPPLIER "},1,expectedOpen],[{supplier:"absent-supplier146"},0,0],[{vendorId:9007199254740991},0,0],[{from:today,to:today},1,expectedOpen],[{from:aged,to:today},0,0],[{asOf:"1970-01-01"},0,0],[{asOf:today,asOfDate:"1970-01-01"},1,expectedOpen],[{asOfDate:today},1,expectedOpen],[{status:"overdue",asOf:aged,minDays:3},expectedOpen>0?1:0,expectedOpen],[{status:"overdue",asOf:aged,minDays:4},0,0],[{status:"paid"},expectedOpen===0?1:0,0]];
    const proof=[];
    for(const [query,count,total] of cases){const raw=await service().get(prefix+"payables/original/list?"+new URLSearchParams(Object.entries(query).map(([k,v])=>[k,String(v)])));const value=await raw.json();need(raw.ok()&&value.ok&&value.data.count===count&&value.data.totalOpenBalance===total,"Original filtered payable count/total");
      const flat={ok:value.ok,...value.data,errors:value.errors};need(isDeepStrictEqual(originalClient("mcp","list",query),flat)&&isDeepStrictEqual(originalClient("cli","list",query),flat),"Original payable query HTTP CLI MCP parity");
      if(count===1)need(value.data.rows[0].paidAmount===expectedPaid&&value.data.rows[0].payableId===payableId,"Original dated paid principal");proof.push({query,value});}
    return proof;
  }
  async function discoveryProof() {
    const invoke = (kind: "cli" | "mcp", name: string, args: Record<string, unknown> = {}) => {
      const common = ["--server", base, "--allow-loopback-http", "--service-key-file", keyFile];
      const input = kind === "cli" ? "" : [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-discovery", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: args } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const argv = kind === "cli" ? ["remote", name, ...common, ...Object.entries(args).flatMap(([k,v]) => ["--" + k, String(v)])] : common;
      const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), argv, { input, encoding: "utf8", timeout: 15000 });
      need(result.status === 0 && !result.stdout.includes(key) && !result.stderr.includes(key), "Discovery process/credential safety");
      if (kind === "cli") return JSON.parse(result.stdout).data;
      const reply = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      need(reply?.result?.isError === false, "MCP discovery read failed");
      return reply.result.structuredContent.data;
    };
    const get = async (path: string) => { const r = await ownerPage.request.get(base + path); need(r.ok(), "HTTP discovery refused"); return r.json(); };
    const about = await get("/api/agent-about");
    const capabilities = await get("/api/agent-capabilities?query=payable&limit=1");
    const workflow = await get("/api/agent-workflows/supplier-payable-handling");
    need(isDeepStrictEqual(invoke("cli", "about"), about), "CLI about differs from HTTP");
    need(isDeepStrictEqual(invoke("cli", "agent-capabilities", { query: "payable", limit: 1 }), capabilities), "CLI search differs");
    need(isDeepStrictEqual(invoke("mcp", "agent_capability_search", { query: "payable", limit: 1 }), capabilities), "MCP search differs");
    need(isDeepStrictEqual(invoke("cli", "agent-workflow", { id: "supplier-payable-handling" }), workflow), "CLI workflow differs");
    need(isDeepStrictEqual(invoke("mcp", "agent_workflow_describe", { id: "supplier-payable-handling" }), workflow), "MCP workflow differs");
    const catalogue = JSON.parse(spawnSync(join(work, "rentemester-mcp"), ["--catalogue"], { encoding: "utf8" }).stdout);
    const mcpAbout = invoke("mcp", "meta_about");
    need(mcpAbout.toolCount === catalogue.tools.length, "MCP about count not actual registry");
    mcpAbout.toolCount = about.toolCount;
    need(isDeepStrictEqual(mcpAbout, about), "MCP about identity differs");
    const help = JSON.parse(spawnSync(join(work, "rentemester-rust"), ["remote", "--help"], { encoding: "utf8" }).stdout);
    need(workflow.workflow.live === true && workflow.workflow.unresolvedOperations.length === 0, "Configured payable workflow unresolved");
    need(capabilities.count === 1 && capabilities.items[0].id === "supplier-purchases", "Payable capability not found");
    for (const step of workflow.workflow.steps) {
      need(step.operation.resolved === true, "HTTP step not registered");
      for (const alias of step.aliases) {
        need(alias.surface === "mcp" ? catalogue.tools.some((t: any) => t.name === alias.name) : help.commands.includes(alias.key.replace(/^remote /, "")), "Advertised alias absent from actual registry");
      }
      if (step.requiresConfirmation) need(step.retryClass === "unsafe-read-back" && step.expectedIdempotent === false, "Unsafe keyed retry claim");
    }
    for (const path of ["/api/agent-capabilities?limit=0", "/api/agent-capabilities?cursor=-1", "/api/agent-capabilities?query=a&query=b", "/api/agent-capabilities?extra=x"]) need((await ownerPage.request.get(base + path)).status() === 400, "Invalid discovery query accepted");
    need((await ownerPage.request.get(base + "/api/agent-workflows/unknown")).status() === 404, "Unknown workflow accepted");
    need(Number.isSafeInteger(about.catalogue.capabilityCount) && about.catalogue.capabilityCount > 0, "Advertised finite capability count");
    const empty = await get("/api/agent-capabilities?cursor=" + about.catalogue.capabilityCount + "&limit=1");
    need(empty.count === 0 && empty.hasMore === false && empty.nextCursor === null, "Pagination beyond end invalid");
    const anonymous = await request.newContext();
    try {
      for (const [method, path] of [["GET", "/payables"], ["POST", "/payables"], ["POST", "/payables/1/pay"]]) {
        const r = await anonymous.fetch(base + prefix + path.slice(1), { method, headers: { origin: base, "content-type": "application/json" }, ...(method === "POST" ? { data: {} } : {}) });
        need([401,403].includes(r.status()), "Described actual route missing or anonymous write/read admitted");
      }
    } finally { await anonymous.dispose(); }
    return { registerTool: workflow.workflow.steps.find((s: any) => s.id === "register-payable").aliases.find((a: any) => a.surface === "mcp").name as string, payCommand: workflow.workflow.steps.find((s: any) => s.id === "pay-payable").aliases.find((a: any) => a.surface === "cli").key.replace(/^remote /, "") as string, crossSurfaceIdentity: true, actualToolCount: catalogue.tools.length, workflow: workflow.workflow.id, actualAliasesResolved: true, invalidInputsRefused: true, anonymousAccountingRefused: true, unsafeReplayNotAdvertised: true };
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
    today = new Date().toISOString().slice(0, 10);
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
    const discovery = await discoveryProof();
    await unchanged(beforeUpload);
    const uploadReceipt = client("cli", "ingest_document", upload("cli")); documentId = uploadReceipt.document.id;
    need(Number.isSafeInteger(documentId) && documentId > 0, "CLI service required scan registered one document");
    const listed = client("mcp", "documents").documents;
    const document = listed.documents.find((value: any) => value.id === documentId);
    need(listed.documents.length === 1 && document?.sha256Hash === hash(original("cli")) && document.invoiceNo === upload("cli").metadata.invoiceNo, "MCP exact factual scanned document");
    const evidence = client("mcp", "document_original", undefined, documentId).original;
    need(evidence.contentType === "text/plain" && Buffer.from(evidence.bytes).equals(original("cli")), "MCP exact required-scanner original bytes");
    need(isDeepStrictEqual((await human("journal")).journal, beforeUpload.journal), "Scanned ingress creates no financial posting");
    const registration = { documentId, billDate: today, dueDate: today, expenseAccountNo: account.expense, confirm: true };
    originalRegistration = {documentId,billDate:today,dueDate:today,expenseAccount:account.expense,confirm:true,idempotencyKey:"synthetic-original-payable-register146"};
    const unconfirmed=originalClient("mcp","register",{...originalRegistration,confirm:false},true);need(unconfirmed.code==="CONFIRM_REQUIRED","Original payable confirmation precedes network");
    registerReceipt=originalClient("mcp","register",originalRegistration);const recognized=registerReceipt;payableId=recognized.payableId;
    const registerReplay=originalClient("mcp","register",originalRegistration);need(registerReceipt.idempotency?.replayed===false&&registerReplay.idempotency?.replayed===true&&registerReplay.idempotency.receiptId===registerReceipt.idempotency.receiptId&&registerReplay.payableId===payableId,"Original keyed recognition no extra posting");
    originalLists={registered:await listOriginal(125,0)};
    need(payableId > 0 && recognized.grossAmount === 125 && recognized.netAmount === 100 && recognized.vatAmount === 25, "MCP factual supplier bill recognition");
    const afterRecognition = await snapshot();
    const duplicateBill = await service().post(prefix + "payables", { data: registration });
    need(duplicateBill.status() === 422 && (await duplicateBill.json()).business === "not_committed", "Duplicate payable is a definite business refusal");
    await unchanged(afterRecognition);
    await human("account-roles", { role: "creditors", accountNo: account.replacement, confirm: true });
    const bytes = Buffer.from(`\uFEFFdate,text,amount,currency,reference,balance_after\r\n${today},Synthetic partial supplier payment,-50.00,DKK,synthetic-partial146,-50.00\r\n${today},Synthetic final supplier payment,-75.00,DKK,synthetic-final146,-125.00\r\n`, "utf8");
    const imported = client("mcp", "import_bank", { csvBase64: bytes.toString("base64"), account: "service-purchases", confirm: true }).import;
    need(imported.imported === 2 && imported.sourceFileHash === hash(bytes) && imported.bankAccountId === bankAccount.id, "Exact two-row BOM/CRLF mapped outgoing source import");
    const bank=(await human("bank")).bank,partial=bank.transactions.find((v:any)=>v.reference==="synthetic-partial146"),row=bank.transactions.find((v:any)=>v.reference==="synthetic-final146");
    need(bank.transactions.length===2&&partial?.amount===-50&&row?.amount===-75&&bank.transactions.every((v:any)=>v.sourceFileHash===hash(bytes)),"Two exact partial/full source rows");
    originalPartial={payableId,bankTransactionId:partial.id,amount:50,confirm:true,idempotencyKey:"synthetic-original-payable-partial146"};
    partialReceipt=originalClient("mcp","pay",originalPartial);need(partialReceipt.openBalance===75&&!partialReceipt.idempotency.replayed,"Original keyed partial bank payment");
    const replay=originalClient("mcp","pay",originalPartial);need(replay.idempotency.replayed&&replay.paymentId===partialReceipt.paymentId&&replay.idempotency.receiptId===partialReceipt.idempotency.receiptId,"Original partial payment known receipt");
    originalLists.partial=await listOriginal(75,50);
    const paymentBody={bankTransactionId:row.id,confirm:true};
    const payment=originalClient("cli","pay",{payableId,bankTransactionId:row.id,amount:75,date:today,paymentAccount:account.bank,note:"Synthetic original final payment146"});
    need(payment.paymentId>0&&payment.openBalance===0,"Original CLI full payment clears payable");
    stable=await snapshot();
    need(stable.journal.entries.length===3&&stable.trialBalance.balanced&&stable.incomeStatement.totalExpense===100&&stable.vat.inputVat===25,"Bill plus two payments preserves one expense/VAT");
    for(const [receipt,amount] of [[partialReceipt,50],[payment,75]] as const){const entry=stable.journal.entries.find((v:any)=>v.id===receipt.journalEntryId);need(entry?.lines.length===2&&entry.lines.some((v:any)=>v.accountNo===account.creditor&&v.debit===amount&&v.credit===0)&&entry.lines.some((v:any)=>v.accountNo===account.bank&&v.debit===0&&v.credit===amount),"Both payments relieve original creditor/bank only");}
    need(stable.trialBalance.rows.find((value: any) => value.accountNo === account.creditor)?.balance === 0
      && !stable.journal.entries.some((value: any) => value.lines.some((line: any) => line.accountNo === account.replacement)), "Original creditor reconciles to zero and replacement receives no posting");
    const paid = stable.payables.rows.find((value: any) => value.payableId === payableId);
    need(paid?.status === "paid" && paid.openBalance === 0 && stable.payables.totalOpenBalance === 0, "Canonical paid payable has zero open balance");
    need(stable.bank.transactions.every((v:any)=>v.reconciliationStatus === "matched") && stable.bank.bookedBalance === -125 && stable.bank.actualBalance === -125 && stable.bank.difference === 0, "Source statement and bank journal reconcile exactly");
    const duplicatePayment = await service().post(prefix + `payables/${payableId}/pay`, { data: paymentBody });
    need(duplicatePayment.status() === 422 && (await duplicatePayment.json()).business === "not_committed", "Duplicate payment is a definite business refusal");
    await unchanged(stable); await parity(stable);originalLists.paid=await listOriginal(0,125);
    const source=join(work,"workspace/synthetic/bank/processed",hash(bytes)+".txt"),statementBytes=readFileSync(source);writeFileSync(source,Buffer.from("Synthetic tampered original payable statement146\n"));
    try{originalClient("mcp","pay",originalPartial,true);originalSourceRefused=true;}finally{writeFileSync(source,statementBytes);}
    await unchanged(stable);
    if (backupProof) backupResult = await backupProof.run();
    return { originalPayableQualified:true,originalRegistration,originalPartial,registerReceipt,partialReceipt,originalLists,originalSourceRefused,backupDestinations: backupResult, discovery, serviceAccountId: serviceId, credentialId, documentId, payableId, requiredScanCliIngest: true, mcpOriginalAndList: true,
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
    if(!revoked){need(stable&&started&&!finished,"Paid source ready for pre-revocation receipt restart");await unchanged(stable);const registration=originalClient("mcp","register",originalRegistration),payment=originalClient("mcp","pay",originalPartial);need(registration.idempotency.replayed&&payment.idempotency.replayed&&registration.idempotency.receiptId===registerReceipt.idempotency.receiptId&&payment.idempotency.receiptId===partialReceipt.idempotency.receiptId&&registration.payableId===payableId&&payment.paymentId===partialReceipt.paymentId,"Exact principal-bound original receipts after restart");await unchanged(stable);receiptRestartProof={qualified:true,registration,payment,lists:await listOriginal(0,125)};return{originalReceiptRestartQualified:true,receiptRestartProof};}
    need(finished && revoked && receiptRestartProof?.qualified, "Current receipt restart and held revocation must finish before final verification");
    await unchanged(stable); const source = await sourceBytes();
    need((await service().get(prefix + "documents")).status() === 401, "Revoked service still refused after restart");
    client("cli", "payables", undefined, undefined, false);
    originalClient("cli","list",{},true);originalClient("mcp","register",originalRegistration,true);originalClient("mcp","pay",originalPartial,true);
    const backupReopen = backupProof ? await backupProof.reopen() : undefined;
    return { originalReceiptRestartQualified:true,receiptRestartProof,originalRevokedReceiptsRefused:true,backupDestinations: backupReopen, exactPaidStateAfterRestart: true, exactOriginalAfterRestart: source, revokedService401: true, noRescanRequested: true, canonical: stable };
  }
  return { run, holdBegin, revoke, holdFinish, reopen };
}
