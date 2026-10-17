"""Independent source-adapter counterexamples layered on the frozen manual oracle."""
from pathlib import Path
import copy,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'docs/architecture/rust-target/implementation/journal-preview'
manual=[json.loads(s) for s in (OUT.parent/'manual-journal/vectors.jsonl').read_text().rstrip('\n').split('\n')]
base=manual[0];cases=[];B='DK-BOOKKEEPING-';vat_message='manual journal entries that affect a VAT amount account require an explicit vatCode on the VAT base line (for example DK_SALE_25, DK_PURCHASE_25, EU_SERVICE_REVERSE_CHARGE, or REPRESENTATION_SPECIAL)'
def add(id,sql=(),account='cash',codes=(),changes=None,messages=(),legacy_codes=None,error=None):
 v=copy.deepcopy(base);v['id']=id;v['sql']=list(sql);v['request']['lines'][0]['accountNo']=account
 if changes:changes(v)
 v['expected']['errors']=[{'code':code,'field':field} for code,field in codes];v['expected']['ok']=not codes
 v['expectedMessages']=list(messages);v['legacyCodes']=[c for c,f in (codes if legacy_codes is None else legacy_codes)]
 if error:v['expected']={'error':error};v['legacyCodes']=None
 cases.append(v)
def stmt(sql,*params):return {'sql':sql,'params':list(params)}
for number,active,normal,kind,control in [('64000',1,'credit','liability',True),('64020',1,'credit','liability',True),('64040',1,'credit','liability',True),('64060',1,'credit','liability',False),('64099',1,'credit','liability',False),('64000',0,'credit','liability',False),('64000',2,'credit','liability',False),('64000',-1,'credit','liability',False),('64000',1,'debit','liability',False),('64000',0,'credit','vat',True)]:
 codes=([('account_inactive','lines[0].accountNo')] if active==0 else [])+([('vat_base_required','lines')] if control else [])
 add('chart-'+number+'-'+str(active)+'-'+normal+'-'+kind,[stmt('INSERT INTO accounts(account_no,name,type,normal_balance,active) VALUES(?,?,?,?,?)',number,'Synthetic source chart',kind,normal,active)],number,codes)
for active in [0,2,-1]:
 add('general-active-'+str(active),[stmt('UPDATE accounts SET active=? WHERE account_no=?',active,'cash')],codes=[('account_inactive','lines[0].accountNo')] if active==0 else [])
for status,active,kind,normal in [('confirmed',1,'liability','credit'),('superseded',1,'liability','credit'),('inactive',1,'liability','credit'),('confirmed',0,'liability','credit'),('confirmed',2,'liability','credit'),('confirmed',1,'asset','credit'),('confirmed',1,'liability','debit')]:
 valid=status=='confirmed' and active==1 and kind=='liability' and normal=='credit';codes=([('account_inactive','lines[1].accountNo')] if active==0 else [])+([] if valid else [('vat_base_required','lines')])
 add('settlement-'+status+'-'+str(active)+'-'+kind+'-'+normal,[stmt('UPDATE account_role_mappings SET status=? WHERE role=?',status,'vat_settlement'),stmt('UPDATE accounts SET active=?,type=?,normal_balance=? WHERE account_no=?',active,kind,normal,'settle')],'tax',codes,lambda v:v['request']['lines'][1].update(accountNo='settle'))
add('confirmed-not-max-version',[stmt("INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('vat_settlement','cash','superseded',99,'user:synthetic')")],'tax',changes=lambda v:v['request']['lines'][1].update(accountNo='settle'))
add('proposal-not-confirmation',[stmt("DELETE FROM account_role_mappings WHERE role='vat_settlement'"),stmt("INSERT INTO account_role_proposals(role,account_no,source) VALUES('vat_settlement','settle','synthetic')")],'tax',[('vat_base_required','lines')],lambda v:v['request']['lines'][1].update(accountNo='settle'))
period={'id':'0','start':'2026-01-01','end':'2026-12-31','kind':'custom','effectiveState':'closed','reference':'source-æ\0'}
def use_period(v):v['facts']['periods']=[copy.deepcopy(period)]
def event(id,name,entity='0'):return stmt("INSERT INTO audit_log(id,event_type,entity_type,entity_id,message) VALUES(? ,?,'accounting_period',?,'Synthetic lifecycle')",{'i64':str(id)},name,entity)
lock=[('period_locked','transactionDate')]
lockmessage='transactionDate 2026-02-01 falls in closed period custom 2026-01-01..2026-12-31 ref source-æ\0'
add('audit-order-not-insertion',[event(200,'period_reopen'),event(100,'period_close')],changes=use_period)
add('unknown-audit-event',[event(1,'other_event')],codes=lock,changes=use_period,messages=[lockmessage])
add('reported-is-terminal',[event(1,'period_report'),event(2,'period_reopen')],codes=lock,changes=use_period,messages=[lockmessage.replace('closed period','reported period')])
add('stored-open-not-candidate',[stmt("UPDATE accounting_periods SET status='open' WHERE id=0"),event(1,'period_close')],changes=use_period)
add('full-width-period-reopen',[event(1,'period_reopen','9007199254740993')],changes=lambda v:(use_period(v),v['facts']['periods'][0].update(id='9007199254740993')),legacy_codes=lock)
add('exact-audit-id-order',[event(9007199254740993,'period_close'),event(9007199254740992,'period_reopen')],codes=lock,changes=use_period,messages=[lockmessage])
add('credit-note-earliest-signed-id',[stmt("INSERT INTO journal_entries(id,entry_no,transaction_date,text,document_id,rule_version,entry_hash,status) VALUES(-5,'earliest-æ','2026-01-01','Synthetic',1,'synthetic',?,'reversed')",'0'*64)],codes=[('credit_note_already_posted','documentId')],changes=lambda v:(v['request'].update(documentId='1'),v['facts']['documents'].append({'id':'1','existingCreditNoteEntry':'later'})),messages=['credit note document 1 already has accounting journal earliest-æ; issue a new credit note document for another correction'])
add('direct-bank-occupancy-is-not-link',[stmt("INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,rule_version,entry_hash) VALUES(2,'direct','2026-01-01','Synthetic',1,'synthetic',?)",'0'*64)],changes=lambda v:(v['request'].update(sourceBankTransactionId='1'),v['facts']['banks'].append({'id':'1','linkedJournal':None})))
add('foreign-key-corruption',[stmt('INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(999,999,10,0)')],error='invalid_ledger')
add('malformed-active-blob',[stmt("UPDATE accounts SET active=x'01' WHERE account_no='cash'")],error='invalid_ledger')
add('malformed-active-real',[stmt("UPDATE accounts SET active=1.5 WHERE account_no='cash'")],error='invalid_ledger')
add('malformed-period-date',[stmt("UPDATE accounting_periods SET period_start=' 2026-01-01' WHERE id=0")],changes=use_period,error='invalid_ledger')
add('unknown-table',[stmt('CREATE TABLE unexpected_extension(id INTEGER)')],error='unsupported_schema')
add('pending-history',[stmt('DELETE FROM schema_migrations WHERE id=36')],error='unsupported_schema')
add('bad-history-checksum',[stmt("UPDATE schema_migrations SET checksum='bad' WHERE id=36")],error='unsupported_schema')
raw=(json.dumps(cases,ensure_ascii=False,indent=2)+'\n').encode();manifest={'format':'rentemester-journal-preview-source-cases/1','cases':len(cases),'generator_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases_sha256':hashlib.sha256(raw).hexdigest(),'manual_vectors_sha256':hashlib.sha256((OUT.parent/'manual-journal/vectors.jsonl').read_bytes()).hexdigest()}
if '--check' in sys.argv:assert (OUT/'source-cases.json').read_bytes()==raw;assert json.loads((OUT/'source-cases-manifest.json').read_text())==manifest
else:(OUT/'source-cases.json').write_bytes(raw);(OUT/'source-cases-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest))
