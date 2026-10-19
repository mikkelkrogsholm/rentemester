import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-bank-correction-clients.json";
type Evidence = { year: string; bankId: number; original: number; reversal: number; replacement: number; replacementNo: string; body: string; currentId: string; planHash: string; receipt: { receiptId: string; createdAt: string; expiresAt: string }; journal: Array<{id: number}>; history: unknown };

/** Dedicated synthetic correction: actual original→initial link→authoritative reversal child→replacement. No accounting is synthesized by correction. */
export async function checkNativeBankCorrectionClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-bank-correction-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native bank-correction CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native bank-correction CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native bank-correction CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-bank-correction-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native bank-correction MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native bank-correction MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native bank-correction MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical bank-correction read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic bank-correction setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence: Evidence;
    let journalBefore: unknown;
    let payablesBefore: unknown;
    if (!restarted) {
      for (const [accountNo,name,type,normalBalance] of [["1764","Synthetic agent correction bank","asset","debit"],["8764","Synthetic agent original equity","equity","credit"],["8765","Synthetic agent replacement equity","equity","credit"]]) await post("accounts",{accountNo,name,type,normalBalance});
      cli("create-bank-account",JSON.stringify({name:"Synthetic agent correction bank",slug:"synthetic-client-correction",currency:"DKK",ledgerAccountNo:"1764"}));
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));
      const originalText="Synthetic agent bank correction original", replacementText="Synthetic agent bank correction replacement";
      async function journalPost(text:string,equity:string,key:string){
        const posting=JSON.parse(state.postedBody);
        posting.input.expectedRuleVersion=(await get("journal")).journal.nativePosting.ruleVersion;
        // Explicit fixture keys belong to the established accounting writer, independently of correction.
        posting.input.idempotencyKey=key;
        posting.input.request={transactionDate:date,text,currency:"DKK",lines:[{accountNo:"1764",debitAmount:"24.14",creditAmount:"0.00"},{accountNo:equity,debitAmount:"0.00",creditAmount:"24.14"}]};
        await post("manual-journal",posting);
      }
      await journalPost(originalText,"8764","synthetic-agent-bank-correction-original");
      cli("import-bank",JSON.stringify({csvContent:`transaction_date,text,amount,currency\n${date},Synthetic agent bank correction,24.14,DKK\n`,account:"synthetic-client-correction",confirm:true}));
      const bank=(await get("bank")).bank;
      const row=bank.transactions.find((r:{text:string})=>r.text==="Synthetic agent bank correction");
      const entries=(await get(`journal?year=${encodeURIComponent(bank.selectedYear)}`)).journal.entries;
      const original=entries.find((e:{text:string})=>e.text===originalText);
      if(!row||!original)throw new Error("Own agent correction source/accounting fixture missing");
      const initialPlan=(await get(`bank/reconciliation-candidates?bankTransactionId=${row.id}`)).reconciliationCandidates.plans.find((p:{journalEntryId:number})=>p.journalEntryId===original.id);
      if(!initialPlan)throw new Error("Initial reviewed source plan missing");
      const initial=cli("bank-reconciliation-link",JSON.stringify({bankTransactionId:row.id,journalEntryId:original.id,planHash:initialPlan.planHash,note:"Synthetic agent reviewed initial original",confirm:true})).reconciliation;
      await journalPost(replacementText,"8765","synthetic-agent-bank-correction-replacement");
      const replacement=(await get(`journal?year=${encodeURIComponent(bank.selectedYear)}`)).journal.entries.find((e:{text:string})=>e.text===replacementText);
      const planFlags=["--bank-transaction-id",String(row.id),"--replacement-journal-entry-id",String(replacement.id)];
      const planArgs={bankTransactionId:row.id,replacementJournalEntryId:replacement.id};
      // An independently posted replacement is insufficient while the current original is active.
      cli("bank-reconciliation-correction-plan",undefined,planFlags,true);
      mcp("bank_reconciliation_correction_plan",planArgs,true);
      await post("manual-journal-reversal",{confirm:true,input:{format:"rentemester-manual-journal-reversal/1",expectedRuleVersion:(await get("journal")).journal.nativePosting.ruleVersion,idempotencyKey:"synthetic-agent-bank-correction-reversal",request:{entryId:String(original.id),transactionDate:date,reason:"Synthetic original equity correction"}}});
      const afterReversal=(await get(`journal?year=${encodeURIComponent(bank.selectedYear)}`)).journal.entries;
      const reversal=afterReversal.find((e:{reversalOfEntryId:number})=>e.reversalOfEntryId===original.id);
      if(!reversal)throw new Error("Authoritative reversal endpoint did not create a linked child");
      const http=await get(`bank/reconciliation-correction-plan?bankTransactionId=${row.id}&replacementJournalEntryId=${replacement.id}`);
      if(!isDeepStrictEqual(cli("bank-reconciliation-correction-plan",undefined,planFlags),http)||!isDeepStrictEqual(mcp("bank_reconciliation_correction_plan",planArgs),http))throw new Error("Complete nested correction plan differs across HTTP/CLI/MCP");
      const plan=http.plan.plan;
      if(http.plan.ok!==true||plan.schemaVersion!=="rentemester-bank-reconciliation-correction-v1"||plan.effect!=="correction_evidence_only_no_posting"||plan.currentJournalEntryId!==original.id||plan.reversalJournalEntryId!==reversal.id||plan.replacementJournalEntryId!==replacement.id||plan.reconciliationId!==initial.reconciliationId)throw new Error("Actual source/reversal/replacement/current-chain proof mismatch");
      const request={bankTransactionId:row.id,replacementJournalEntryId:replacement.id,expectedReconciliationId:plan.reconciliationId,planHash:plan.planHash,reason:"  Synthetic agent reviewed actual reversal and replacement proof  ",idempotencyKey:"synthetic-agent-bank-correction-apply",confirm:true};
      journalBefore=afterReversal;payablesBefore=(await get("payables?status=all")).payables.rows;
      // The initial-link hash is real source evidence, but cannot authorize a different correction schema.
      mcp("bank_reconciliation_correction",{body:JSON.stringify({...request,planHash:initialPlan.planHash,idempotencyKey:"synthetic-agent-bank-correction-wrong-proof"}),confirm:true},true);
      const body=JSON.stringify(request);
      const correction=cli("bank-reconciliation-correction",body).correction;
      if(correction.ok!==true||correction.effect!=="correction_evidence_only_no_posting"||correction.idempotent!==false||correction.receipt?.replayed!==false)throw new Error("CLI correction did not return a fresh durable evidence receipt");
      const current=(await get(`bank?year=${encodeURIComponent(bank.selectedYear)}`)).bank.transactions.find((r:{id:number})=>r.id===row.id);
      evidence={year:bank.selectedYear,bankId:row.id,original:original.id,reversal:reversal.id,replacement:replacement.id,replacementNo:replacement.entryNo,body,currentId:correction.reconciliationId,planHash:correction.planHash,receipt:correction.receipt,journal:afterReversal.filter((e:{id:number})=>[original.id,reversal.id,replacement.id].includes(e.id)),history:current.reconciliationHistory};
      writeFileSync(saved,JSON.stringify(evidence));
      cli("bank-reconciliation-correction",JSON.stringify({...request,idempotencyKey:"synthetic-agent-bank-correction-stale"}),[],true);
      mcp("bank_reconciliation_correction",{body:JSON.stringify({...request,reason:"Different raw payload under same key"}),confirm:true},true);
    } else {
      evidence=JSON.parse(readFileSync(saved,"utf8"));
      journalBefore=(await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
      payablesBefore=(await get("payables?status=all")).payables.rows;
    }
    // Read canonical evidence before the explicit receipt-recovery assertion. This is not an adapter retry.
    const bank=(await get(`bank?year=${encodeURIComponent(evidence.year)}`)).bank;
    const current=bank.transactions.find((r:{id:number})=>r.id===evidence.bankId);
    if(current?.reconciliationId!==evidence.currentId||current.reconciliationStatus!=="matched"||current.journalEntryNo!==evidence.replacementNo||current.reconciliationProof!==evidence.planHash||current.reconciliationNote!==JSON.parse(evidence.body).reason.trim()||!isDeepStrictEqual(current.reconciliationHistory,evidence.history))throw new Error("Canonical corrected proof/reason/history missing or changed");
    const replay=mcp("bank_reconciliation_correction",{body:evidence.body,confirm:true}).correction;
    if(replay.ok!==true||replay.idempotent!==true||replay.receipt?.replayed!==true||replay.reconciliationId!==evidence.currentId||replay.planHash!==evidence.planHash||replay.receipt.receiptId!==evidence.receipt.receiptId||replay.receipt.createdAt!==evidence.receipt.createdAt||replay.receipt.expiresAt!==evidence.receipt.expiresAt)throw new Error("Explicit same-principal exact MCP replay did not recover original durable receipt");
    const cliReplay=cli("bank-reconciliation-correction",evidence.body).correction;
    if(!isDeepStrictEqual(cliReplay,replay))throw new Error("Explicit CLI and MCP receipt replay differ");
    for(const view of [cli("bank",undefined,["--year",evidence.year]).bank,mcp("bank",{year:evidence.year}).bank])if(!isDeepStrictEqual(view.transactions.find((r:{id:number})=>r.id===evidence.bankId),current))throw new Error("Client canonical correction/history differs from HTTP");
    const journalAfter=(await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
    if(!isDeepStrictEqual(journalAfter,journalBefore)||!isDeepStrictEqual((await get("payables?status=all")).payables.rows,payablesBefore))throw new Error("Correction/replay changed accounting or payable effects");
    for(const original of evidence.journal)if(!isDeepStrictEqual(journalAfter.find((e:{id:number})=>e.id===original.id),original))throw new Error("Own original/reversal/replacement facts changed across restart");
    console.log(restarted?"PASS: CLI/MCP canonical correction/history and explicit original durable receipt survive restart; no accounting effects":"PASS: own initial link → active-original plan refusal → actual reversal → full correction plan parity → keyed CLI evidence correction → canonical reads → explicit MCP/CLI receipt replay; wrong-proof/stale/conflict refused; no accounting effects");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
