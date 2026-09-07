/** Read-only owner projection for a canonical party.  It deliberately joins
 * only durable party IDs: names and aliases are search/display evidence, never
 * an accounting-link heuristic. */
import type { Database } from "bun:sqlite";
import { inspectVisibleParty, searchParties } from "./party-registry";

export type PartyHubSort = "name" | "role" | "activity" | "spend";
const date = (value?: string) => value && /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : new Date().toISOString().slice(0, 10);
const money = (value: unknown) => Number.isFinite(Number(value)) ? Number(value) : 0;

function activity(ledger: Database, partyId: string, asOf: string) {
  const rows = ledger.query(`SELECT d.id,d.document_no,d.invoice_date,d.amount_inc_vat,d.currency,l.party_role,l.evidence_kind,l.created_at
    FROM current_document_party_links l JOIN documents d ON d.id=l.document_id
    WHERE l.party_id=? AND (d.invoice_date IS NULL OR d.invoice_date<=?)
    ORDER BY COALESCE(d.invoice_date,substr(l.created_at,1,10)) DESC,d.id DESC`).all(partyId, asOf) as any[];
  const purchase = rows.filter(row => ["supplier", "vendor", "payee"].includes(row.party_role)).reduce((sum,row) => sum + money(row.amount_inc_vat), 0);
  const sales = rows.filter(row => ["customer", "recipient", "payer"].includes(row.party_role)).reduce((sum,row) => sum + money(row.amount_inc_vat), 0);
  const latest = rows[0] ? (rows[0].invoice_date ?? String(rows[0].created_at).slice(0,10)) : null;
  return { rows, purchase, sales, latest };
}

export function partyProfile(ledger: Database, control: Database, input: {companySlug:string; partyId:string; visibleCompanies:ReadonlySet<string>; from?:string; asOf?:string}) {
  const asOf=date(input.asOf), from=date(input.from ?? `${asOf.slice(0,4)}-01-01`);
  const party=inspectVisibleParty(control,input.partyId,input.visibleCompanies);
  if(!party) return null;
  const a=activity(ledger,input.partyId,asOf);
  const warnings=[...party.assertions,...party.aliases].filter((item:any)=>item.review_state === "proposed").map((item:any)=>`Foreslået kilde kræver review: ${item.source}`);
  return { party:{partyId:party.partyId,name:party.name,kind:party.kind,roles:party.roles,aliases:party.aliases,assertions:party.assertions},
    computed:{period:{from,to:asOf},asOf,sourceCoverage:{documents:a.rows.length,bankTransactions:0,journalEntries:0,invoices:0,unmappedLedgerReferences:0},purchase:{amount:a.purchase,currency:"mixed"},sales:{amount:a.sales,currency:"mixed"},recentActivity:a.latest},
    research:{assertions:party.assertions,aliases:party.aliases,warnings},
    links:{documents:a.rows.map(row=>({kind:"document",id:row.id,label:row.document_no??`Bilag ${row.id}`,role:row.party_role,evidenceKind:row.evidence_kind})),relations:party.roles.map((role:any)=>({kind:"company_role",type:role.role,companySlug:role.companySlug,href:`/companies/${encodeURIComponent(role.companySlug)}/workspace-register`}))}
  };
}

export function partyHub(ledger:Database,control:Database,input:{companySlug:string;visibleCompanies:ReadonlySet<string>;query?:string;sort?:PartyHubSort;from?:string;asOf?:string;cursor?:number;limit?:number}) {
  const asOf=date(input.asOf), from=date(input.from ?? `${asOf.slice(0,4)}-01-01`), sort=input.sort ?? "name";
  const candidates=searchParties(control,{query:input.query,companySlugs:input.visibleCompanies,limit:100,cursor:0}).rows;
  const rows=candidates.map((party:any)=>{const a=activity(ledger,party.partyId,asOf);return {partyId:party.partyId,name:party.name,kind:party.kind,roles:party.roles.map((r:any)=>r.role),recentActivity:a.latest,computedSpend:a.purchase,sourceCoverage:{documents:a.rows.length},partyLink:{partyId:party.partyId,href:`/companies/${encodeURIComponent(input.companySlug)}/parter/${encodeURIComponent(party.partyId)}`}};});
  rows.sort((a,b)=> sort === "spend" ? b.computedSpend-a.computedSpend || a.name.localeCompare(b.name,"da") : sort === "activity" ? String(b.recentActivity??"").localeCompare(String(a.recentActivity??"")) || a.name.localeCompare(b.name,"da") : sort === "role" ? a.roles.join(",").localeCompare(b.roles.join(","),"da") || a.name.localeCompare(b.name,"da") : a.name.localeCompare(b.name,"da"));
  const limit=Math.min(Math.max(input.limit??25,1),100),cursor=Math.max(input.cursor??0,0);
  return {period:{from,to:asOf},asOf,sort,rows:rows.slice(cursor,cursor+limit),count:rows.length,nextCursor:cursor+limit<rows.length?cursor+limit:null};
}
