/** Actual synthetic TS writer corpus for the paired native adoption runner. */
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {existsSync,lstatSync,readFileSync,writeFileSync} from 'node:fs';
import {basename,join,resolve} from 'node:path';
import {createCompany} from '../../src/core/company';
import {migrate, openDb} from '../../src/core/db';
import {postIssuedInvoiceToLedger} from '../../src/core/invoice-booking';
import {issueInvoice} from '../../src/core/issued-invoices';
import {initWorkspace,setWorkspaceCompanyArchived} from '../../src/core/workspace';
import {openWorkspaceControlDb} from '../../src/core/workspace-control';

const work=resolve(process.env.RM_ADOPTION_WORK??''),meta=lstatSync(work);
assert(basename(work).startsWith('Rentemester-Synthetic-Adoption-')&&meta.isDirectory()&&!meta.isSymbolicLink()&&(meta.mode&0o077)===0&&meta.uid===process.getuid?.());
const source=join(work,'legacy');assert(!existsSync(source));
initWorkspace(source);openWorkspaceControlDb(source).close();
const actor={createdBy:'agent:synthetic-adoption',createdByProgram:'synthetic-adoption-fixture'};
const date=new Date().toLocaleDateString('sv-SE',{timeZone:'Europe/Copenhagen'});
const companies=[];
for(const [slug,net] of [['synthetic',200],['synthetic-archived',40]] as const){
 const company=createCompany(source,{slug,name:`Synthetic Adoption ${slug}`,cvr:'DK87654321',address:'Synthetic Seller Street 1',postalCode:'1000',city:'Synthetic City',fiscalYearStartMonth:1,fiscalYearLabelStrategy:'end-year',vatPeriodType:'quarter',onboardingActor:actor.createdBy});
 const db=openDb(company.dbPath);
 try{
  migrate(db);
  const issued=issueInvoice(db,company.companyRoot,{invoiceType:'full',vatTreatment:'standard',issueDate:date,buyer:{name:'Synthetic Customer',address:'Synthetic Buyer Street 2'},lines:[{description:'Synthetic service',quantity:1,unitPriceExVat:net,lineTotalExVat:net}],totals:{netAmount:net,vatRate:.25,vatAmount:net*.25,grossAmount:net*1.25},currency:'DKK'},actor);
  assert(issued.ok&&issued.documentId,JSON.stringify(issued));
  const posted=postIssuedInvoiceToLedger(db,{invoiceDocumentId:issued.documentId,revenueAccountNo:'1000',...actor});assert(posted.ok,JSON.stringify(posted));
  const originals=(db.query('SELECT id,stored_path,sha256_hash FROM documents ORDER BY id').all() as {id:number;stored_path:string;sha256_hash:string}[]).map(r=>{assert.equal(createHash('sha256').update(readFileSync(r.stored_path)).digest('hex'),r.sha256_hash);return{documentId:r.id,sourcePath:r.stored_path,sha256:r.sha256_hash};});
  companies.push({slug,archived:slug.endsWith('-archived'),ledgerUuid:(db.query('SELECT ledger_uuid FROM ledger_identity WHERE id=1').get() as {ledger_uuid:string}).ledger_uuid,journalEntries:db.query('SELECT * FROM journal_entries ORDER BY id').all(),journalLines:db.query('SELECT * FROM journal_lines ORDER BY id').all(),originals});
 }finally{db.close();}
}
setWorkspaceCompanyArchived(source,'synthetic-archived',true);
writeFileSync(join(work,'legacy-corpus.json'),JSON.stringify({source,date,companies},null,2),{mode:0o600,flag:'wx'});
console.log('PASS: actual public TS issued/posted invoices in active and archived company ledgers');
