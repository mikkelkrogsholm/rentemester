/**
 * Source-linked, read-only CFO analytics (#581).
 *
 * This is deliberately a query facade, not a warehouse: every row is read
 * directly from a legal company's immutable journal or its retained import
 * archive.  Portfolio is a juxtaposition; legal consolidation remains the
 * existing reviewed group-report contract.
 */
import { existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { buildConsolidatedReport } from "./consolidated-reports";
import { companyPaths } from "./paths";
import { companyRootForSlug, findWorkspaceCompany, listWorkspaceCompanies } from "./workspace";
import { openWorkspaceControlReadOnlyDb } from "./workspace-control";
import { openLedgerReadOnly } from "./ledger-inspection";
import { importedReceivableBalanceOre } from "./imported-receivables";

export const CFO_ANALYTICS_SCHEMA_VERSION = "rentemester-cfo-analytics-v1";
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const LIMIT_MAX = 200;

export type CfoAnalyticsInput = {
  scope: "company" | "portfolio" | "group";
  companySlug?: string;
  companySlugs?: string[];
  groupProfileId?: string;
  from: string;
  to: string;
  account?: string;
  party?: string;
  /** Recorded dimensions are not yet present on canonical journal lines. */
  dimension?: string;
  currency?: string;
  /** Portfolio is juxtaposed by default; a caller must request a sum. */
  aggregate?: "none" | "sum";
  cursor?: string;
  limit?: number;
};

type Row = {
  companySlug: string; sourceType: "ledger" | "archive"; sourceHash: string;
  sourceId: string; journalEntryId: number | null; journalEntryNo: string | null;
  documentId: number | null; documentHash: string | null; partyId: string | null;
  /** Canonical name from the immutable snapshot captured with the current link. */
  partyName: string | null;
  /** Original document extraction, retained separately from party identity. */
  documentPartyName: string | null;
  /** Hash-bound, immutable link evidence for the reported canonical party. */
  partyLinkProvenance: {linkId:number;role:string;evidenceKind:string;planHash:string;linkedAt:string} | null;
  accountNo: string; accountName: string | null;
  transactionDate: string; currency: string; amount: number; text: string | null;
  /** Approved classifications are a sidecar: never part of journal bytes. */
  dimensions: Array<{dimensionId:string;memberId:string;amountMinor:number;currency:string}>;
};

function sha(value: string): string { return createHash("sha256").update(value).digest("hex"); }
function validDate(value: string, label: string): string { if (!DATE.test(value) || Number.isNaN(Date.parse(`${value}T00:00:00Z`))) throw new Error(`${label} must be YYYY-MM-DD`); return value; }
function normal(value: string | undefined): string | undefined { const v=value?.trim(); return v ? v : undefined; }
function canonicalCursor(row: Row): string { return [row.transactionDate,row.sourceType,row.companySlug,row.sourceId,row.accountNo].join("|"); }
function decodeCursor(cursor: string | undefined): string | undefined { if (!cursor) return undefined; try { const result=Buffer.from(cursor,"base64url").toString("utf8"); if (result.split("|").length !== 5) throw new Error(); return result; } catch { throw new Error("cursor is invalid"); } }
function encodeCursor(row: Row): string { return Buffer.from(canonicalCursor(row)).toString("base64url"); }

function canonicalPartyLink(row: any): Pick<Row,"partyId"|"partyName"|"partyLinkProvenance"> {
  if (row.partyLinkId == null) return {partyId:null,partyName:null,partyLinkProvenance:null};
  try {
    const snapshot=JSON.parse(String(row.partySnapshotJson));
    // The snapshot is the legal, immutable statement at link time. Do not
    // resolve a name through the mutable workspace registry, and fail closed
    // if the event's identity and its captured snapshot disagree.
    if (!snapshot || snapshot.partyId !== String(row.partyId) || typeof snapshot.name !== "string" || !snapshot.name.trim()) throw new Error("invalid party snapshot");
    return {partyId:String(row.partyId),partyName:snapshot.name,partyLinkProvenance:{linkId:Number(row.partyLinkId),role:String(row.partyRole),evidenceKind:String(row.partyEvidenceKind),planHash:String(row.partyPlanHash),linkedAt:String(row.partyLinkedAt)}};
  } catch { return {partyId:null,partyName:null,partyLinkProvenance:null}; }
}

function sourceRows(companySlug: string, companyRoot: string, input: Required<Pick<CfoAnalyticsInput,"from"|"to">> & Pick<CfoAnalyticsInput,"account"|"party"|"currency"|"dimension">): Row[] {
  const dbPath=companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) return [];
  const db=openLedgerReadOnly(dbPath);
  try {
    db.exec("PRAGMA query_only=ON");
    const ledger=db.query(`SELECT je.id AS journalEntryId,je.entry_no AS journalEntryNo,je.entry_hash AS sourceHash,je.transaction_date AS transactionDate,je.currency AS currency,je.text AS entryText,je.document_id AS documentId,d.sha256_hash AS documentHash,d.supplier_name AS documentPartyName,jl.id AS lineId,jl.debit_amount-jl.credit_amount AS amount,jl.text AS lineText,a.account_no AS accountNo,a.name AS accountName,(SELECT allocations_json FROM current_accounting_dimension_assignments da WHERE da.journal_line_id=jl.id ORDER BY da.id DESC LIMIT 1) AS dimensionsJson,pl.id AS partyLinkId,pl.party_id AS partyId,pl.party_role AS partyRole,pl.evidence_kind AS partyEvidenceKind,pl.party_snapshot_json AS partySnapshotJson,pl.plan_hash AS partyPlanHash,pl.created_at AS partyLinkedAt FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id JOIN accounts a ON a.id=jl.account_id LEFT JOIN documents d ON d.id=je.document_id LEFT JOIN current_document_party_links pl ON pl.id=(SELECT candidate.id FROM current_document_party_links candidate WHERE candidate.document_id=je.document_id ORDER BY CASE candidate.party_role WHEN 'supplier' THEN 0 WHEN 'issuer' THEN 1 WHEN 'vendor' THEN 2 WHEN 'customer' THEN 3 WHEN 'recipient' THEN 4 ELSE 99 END,candidate.id DESC LIMIT 1) WHERE je.status='posted' AND je.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=je.id) AND je.transaction_date BETWEEN ? AND ? ORDER BY je.transaction_date,je.id,jl.id`).all(input.from,input.to) as any[];
    const archive=db.query(`SELECT y.id AS archiveId,y.source_system AS sourceSystem,y.fiscal_year AS fiscalYear,p.id AS postingId,p.line_no AS lineNo,p.account_no AS accountNo,p.account_name AS accountName,p.transaction_date AS transactionDate,p.voucher AS voucher,p.text AS text,p.amount AS amount,(SELECT archive_sha256 FROM dinero_import_archive_evidence e WHERE e.fiscal_year=y.fiscal_year ORDER BY e.id DESC LIMIT 1) AS sourceHash FROM import_archive_years y JOIN import_archive_postings p ON p.archive_year_id=y.id WHERE p.transaction_date BETWEEN ? AND ? ORDER BY p.transaction_date,y.id,p.line_no`).all(input.from,input.to) as any[];
    const account=normal(input.account), partyId=normal(input.party), currency=normal(input.currency)?.toUpperCase();
    // Journal lines are the legal ledger's DKK base amounts.  `je.currency`
    // describes the original posting context, not this line's unit, and must
    // never be used to label or aggregate base amounts as foreign currency.
    const dimensions=(raw:unknown):Row["dimensions"]=>{try{return Array.isArray(JSON.parse(String(raw??"[]")))?JSON.parse(String(raw??"[]")):[];}catch{return [];}};
    const live:Row[]=ledger.map(row=>({companySlug,sourceType:"ledger",sourceHash:String(row.sourceHash),sourceId:`journal:${row.journalEntryId}:line:${row.lineId}`,journalEntryId:Number(row.journalEntryId),journalEntryNo:String(row.journalEntryNo),documentId:row.documentId==null?null:Number(row.documentId),documentHash:row.documentHash??null,...canonicalPartyLink(row),documentPartyName:row.documentPartyName??null,accountNo:String(row.accountNo),accountName:row.accountName??null,transactionDate:String(row.transactionDate),currency:"DKK",amount:Number(row.amount),text:row.lineText??row.entryText??null,dimensions:dimensions(row.dimensionsJson)}));
    const archived:Row[]=archive.map(row=>{
      // Older imported years may predate retained archive evidence. In that
      // case expose a canonical hash of the immutable archive row content,
      // never an identity-only synthetic digest.
      const sourceHash=row.sourceHash && /^[a-f0-9]{64}$/i.test(String(row.sourceHash)) ? String(row.sourceHash) : sha(JSON.stringify([row.sourceSystem,row.fiscalYear,row.archiveId,row.postingId,row.lineNo,row.accountNo,row.accountName,row.transactionDate,row.voucher,row.text,row.amount]));
      return {companySlug,sourceType:"archive" as const,sourceHash,sourceId:`archive:${row.archiveId}:posting:${row.postingId}`,journalEntryId:null,journalEntryNo:row.voucher??null,documentId:null,documentHash:null,partyId:null,partyName:null,documentPartyName:null,partyLinkProvenance:null,accountNo:String(row.accountNo),accountName:row.accountName??null,transactionDate:String(row.transactionDate),currency:"DKK",amount:Number(row.amount),text:row.text??null,dimensions:[]};
    });
    const dimension=normal(input.dimension);
    // `party` is an identity filter, not fuzzy text search. Historical names
    // and journal text are evidence, but must never select a different
    // canonical party merely because a string happens to match.
    return [...live,...archived].filter(row => (!account || row.accountNo===account) && (!currency || row.currency===currency) && (!partyId || row.partyId===partyId) && (!dimension || row.dimensions.some(item=>dimension===item.dimensionId || dimension===`${item.dimensionId}:${item.memberId}`)));
  } finally { db.close(); }
}

function readEvidenceCompleteness(companySlug:string,companyRoot:string,from:string,to:string) {
  const dbPath=companyPaths(companyRoot).db; if(!existsSync(dbPath)) return {companySlug,status:"unavailable" as const,reason:"company ledger not found"};
  const db=openLedgerReadOnly(dbPath); try {
    const journal=db.query("SELECT COUNT(*) AS count FROM journal_entries WHERE status='posted' AND reversal_of_entry_id IS NULL AND transaction_date BETWEEN ? AND ? AND document_id IS NULL").get(from,to) as {count:number};
    const exceptions=db.query("SELECT COUNT(*) AS count FROM exceptions WHERE status='open'").get() as {count:number};
    const imported=db.query("SELECT COUNT(*) AS count FROM imported_receivable_headers WHERE invoice_date<=?").get(to) as {count:number};
    const controls=db.query("SELECT DISTINCT control_account_no AS accountNo FROM imported_receivable_headers ORDER BY control_account_no").all() as Array<{accountNo:string}>;
    const importedOpenDkk=controls.reduce((sum,row)=>sum+Number(importedReceivableBalanceOre(db,to,row.accountNo).total)/100,0);
    return {companySlug,status:"ready" as const,postedWithoutDocument:journal.count,openExceptions:exceptions.count,importedReceivableCount:imported.count,importedReceivableOpenDkk:importedOpenDkk};
  } finally { db.close(); }
}

function analyticsForCompanies(workspaceRoot:string, companySlugs:string[], input:CfoAnalyticsInput) {
  const rows=companySlugs.flatMap(slug=>sourceRows(slug,companyRootForSlug(workspaceRoot,slug),input as Required<Pick<CfoAnalyticsInput,"from"|"to">> & Pick<CfoAnalyticsInput,"account"|"party"|"currency"|"dimension">)).sort((a,b)=>canonicalCursor(a).localeCompare(canonicalCursor(b)));
  const after=decodeCursor(input.cursor); const page=rows.filter(row=>!after || canonicalCursor(row)>after).slice(0,Math.min(Math.max(input.limit??100,1),LIMIT_MAX));
  const totalsByCurrency:Record<string,number>={}; for(const row of rows) totalsByCurrency[row.currency]=(totalsByCurrency[row.currency]??0)+row.amount;
  const sourceHashes=[...new Set(rows.map(row=>row.sourceHash))].sort();
  const freshnessBySource=new Map<string,{source:Row["sourceType"];companySlug:string;latestTransactionDate:string}>();
  for (const row of rows) { const key=`${row.companySlug}:${row.sourceType}`, current=freshnessBySource.get(key); if (!current || row.transactionDate>current.latestTransactionDate) freshnessBySource.set(key,{source:row.sourceType,companySlug:row.companySlug,latestTransactionDate:row.transactionDate}); }
  const freshness=[...freshnessBySource.values()].sort((a,b)=>`${a.companySlug}:${a.source}`.localeCompare(`${b.companySlug}:${b.source}`));
  return { rows:page, page:{limit:Math.min(Math.max(input.limit??100,1),LIMIT_MAX),nextCursor:page.length && rows.findIndex(r=>canonicalCursor(r)===canonicalCursor(page.at(-1)!))<rows.length-1?encodeCursor(page.at(-1)!):null}, freshness, reconciliation:{rowCount:rows.length,amountByCurrency:totalsByCurrency,sourceHashes,method:"sum(all matching rows.amount) by original currency; every row is directly linked to its source journal/archive"} };
}

/** One deterministic, versioned schema shared by CLI, MCP and HTTP. */
export function queryCfoAnalytics(workspaceRoot:string,input:CfoAnalyticsInput, visibleCompanySlugs?: readonly string[]) {
  const from=validDate(input.from,"from"),to=validDate(input.to,"to"); if(from>to) throw new Error("from must be on or before to");
  const requested=input.scope==="company" ? [normal(input.companySlug) ?? ""] : (input.companySlugs?.length ? [...new Set(input.companySlugs)].sort() : listWorkspaceCompanies(workspaceRoot).filter(c=>!c.archived).map(c=>c.slug));
  if(requested.some(slug=>!slug || !findWorkspaceCompany(workspaceRoot,slug))) throw new Error("companySlug references an unknown workspace company");
  const visible=visibleCompanySlugs ? new Set(visibleCompanySlugs) : null;
  const companies=requested.filter(slug=>!visible || visible.has(slug));
  const hidden=requested.filter(slug=>visible && !visible.has(slug));
  if(input.scope==="company" && hidden.length) throw new Error("company is not accessible");
  if(input.scope==="group") {
    if (!input.groupProfileId) throw new Error("groupProfileId is required for group scope");
    const control=openWorkspaceControlReadOnlyDb(workspaceRoot); try {
      const report=buildConsolidatedReport(control,workspaceRoot,new Set(companies),input.groupProfileId,from,to);
      if(report.status!=="ready") return {schemaVersion:CFO_ANALYTICS_SCHEMA_VERSION,scope:"group",status:"unsupported",asOf:to,limitations:["The approved consolidated report is blocked; no analytics totals are manufactured."],group:report};
      return {schemaVersion:CFO_ANALYTICS_SCHEMA_VERSION,scope:"group",status:"ready",asOf:to,limitations:["Group analytics delegates to the approved consolidation profile; it does not infer ownership, FX or eliminations."],group:report};
    } finally { control.close(); }
  }
  const result=analyticsForCompanies(workspaceRoot,companies,{...input,from,to});
  const evidenceCompleteness=companies.map(slug=>readEvidenceCompleteness(slug,companyRootForSlug(workspaceRoot,slug),from,to));
  const aggregate=input.aggregate??"none";
  const allowTotals=input.scope!=="portfolio" || (aggregate==="sum"&&!hidden.length);
  return {schemaVersion:CFO_ANALYTICS_SCHEMA_VERSION,scope:input.scope,status:hidden.length?"incomplete":"ready",asOf:to,from,to,companies,partial:hidden.length>0,mode:input.scope==="portfolio"?"juxtaposed-non-consolidated":"legal-company",aggregate,limitations:input.scope==="portfolio"?["Portfolio is a juxtaposed, non-consolidated view. No eliminations or inferred currency conversion are applied.",...(hidden.length?["One or more requested companies are hidden; aggregate totals are intentionally omitted."]:[])]:["Rows with no recorded party link expose partyId:null; no party identity is inferred."],rows:result.rows,page:result.page,freshness:result.freshness,evidenceCompleteness,reconciliation:allowTotals?result.reconciliation:{rowCount:result.reconciliation.rowCount,sourceHashes:result.reconciliation.sourceHashes,method:result.reconciliation.method,omitted:"portfolio aggregate not requested or access is partial"}};
}
