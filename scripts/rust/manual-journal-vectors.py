"""Independent authored journal expectations; Decimal used only for fixed balance vectors."""
import copy,hashlib,json,sys
from decimal import Decimal,ROUND_HALF_UP,localcontext
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'docs/architecture/rust-target/implementation/manual-journal'
bundles=json.loads((OUT/'rule-bundles.json').read_text());version=';'.join(b['name']+'='+b['version'] for b in sorted(bundles,key=lambda b:b['name']))
B='DK-BOOKKEEPING-';base_rules=[B+'BALANCED-001',B+'APPEND-ONLY-001',B+'PERIOD-LOCK-001']
accounts=[{'accountNo':n,'kind':k,'active':True,'vatControl':c,'vatSettlement':s} for n,k,c,s in [('cash','asset',False,False),('capital','equity',False,False),('sale','income',False,False),('cost','expense',False,False),('tax','vat',True,False),('settle','liability',False,True),('æ🦀\0','asset',False,False)]]
request={'transactionDate':'2026-02-01','text':'Synthetic journal','currency':'DKK','lines':[{'accountNo':'cash','debitAmount':'10.00'},{'accountNo':'capital','creditAmount':'10.00'}]}
facts={'transactionDate':'2026-02-01','latestAllowedDate':'2026-03-18','accounts':accounts,'documents':[],'banks':[],'periods':[]}
vs=[]
def case(name,mutate=None,errors=(),totals=('10.00','10.00'),fx=False,doc=False,legacy='match',error=None):
 r=copy.deepcopy(request);f=copy.deepcopy(facts);v={'id':name,'op':'manual','request':r,'facts':f,'legacy':legacy}
 if mutate:mutate(r,f,v)
 v['expected']={'error':error} if error else {'ok':not errors,'ruleVersion':version,'requiredRules':base_rules+([B+'FX-001'] if fx else [])+([B+'DOCUMENT-001'] if doc else []),'requiredStorageRules':[B+'APPEND-ONLY-001'],'totalDebit':totals[0],'totalCredit':totals[1],'errors':[{'code':c,'field':p} for c,p in errors]}
 vs.append(v)
def line(r,i,**kw):r['lines'][i].update(kw)
def period(id='1',start='2026-01-01',end='2026-12-31',state='closed',kind='custom',reference=None):return {'id':id,'start':start,'end':end,'kind':kind,'effectiveState':state,'reference':reference}
case('balanced')
case('blank-text',lambda r,f,v:r.update(text=' \t\n\ufeff'),[('text_required','text')])
case('non-js-whitespace',lambda r,f,v:r.update(text='\u0085'))
case('missing-lines',lambda r,f,v:r.update(lines=[]),[('lines_required','lines')],('0.00','0.00'))
case('one-line',lambda r,f,v:r.update(lines=r['lines'][:1]),[('lines_required','lines'),('unbalanced','lines')],('10.00','0.00'))
case('imbalance',lambda r,f,v:line(r,1,creditAmount='9.99'),[('unbalanced','lines')],('10.00','9.99'))
case('zero-line',lambda r,f,v:line(r,0,debitAmount='0.00'),[('line_side','lines[0]'),('unbalanced','lines')],('0.00','10.00'))
case('both-sides',lambda r,f,v:line(r,0,creditAmount='1.00'),[('line_side','lines[0]'),('unbalanced','lines')],('10.00','11.00'))
case('negative',lambda r,f,v:line(r,0,debitAmount='-10.00'),[('negative_amount','lines[0]'),('unbalanced','lines')],('-10.00','10.00'))
case('unknown-skips',lambda r,f,v:line(r,0,accountNo='missing',debitAmount='-10.00',vatCode='NO'),[('account_missing','lines[0].accountNo'),('unbalanced','lines')],('0.00','10.00'))
case('inactive-order',lambda r,f,v:(f['accounts'][0].update(active=False),line(r,0,vatCode='NO',debitAmount='-10.00')),[('vat_code_unsupported','lines[0].vatCode'),('account_inactive','lines[0].accountNo'),('negative_amount','lines[0]'),('unbalanced','lines')],('-10.00','10.00'))
case('opaque-account',lambda r,f,v:line(r,0,accountNo='æ🦀\0'))
case('reserved-program',lambda r,f,v:r.update(createdByProgram='rentemester-import-postings'),[('reserved_program','createdByProgram')])
case('program-prefix',lambda r,f,v:r.update(createdByProgram='rentemester-import-postings-extra'))
case('future-date',lambda r,f,v:f.update(latestAllowedDate='2026-01-31'),[('future_date','transactionDate')])
case('period-lock',lambda r,f,v:f.update(periods=[period(reference='x\0æ')]),[('period_locked','transactionDate')])
case('period-reopened',lambda r,f,v:f.update(periods=[period(state='open')]))
case('period-outside',lambda r,f,v:f.update(periods=[period(start='2026-03-01')]))
case('period-order',lambda r,f,v:f.update(periods=[period('2',end='2026-06-30'),period('-1',end='2026-06-30',state='reported',kind='fiscal_year'),period('3',end='2026-12-31')]),[('period_locked','transactionDate')])
case('period-vat-kind',lambda r,f,v:f.update(periods=[period(kind='vat_period')]),[('period_locked','transactionDate')])
case('period-legacy-vat-kind',lambda r,f,v:f.update(periods=[period(kind='vat_quarter')]),[('period_locked','transactionDate')])
case('future-and-period',lambda r,f,v:f.update(latestAllowedDate='2026-01-31',periods=[period()]),[('future_date','transactionDate'),('period_locked','transactionDate')])
case('vat-control-no-base',lambda r,f,v:line(r,0,accountNo='tax'),[('vat_base_required','lines')])
case('vat-transfer',lambda r,f,v:(line(r,0,accountNo='tax'),line(r,1,accountNo='settle')))
case('settlement-not-pure',lambda r,f,v:r['lines'].extend([{'accountNo':'tax','debitAmount':'1.00'},{'accountNo':'settle','creditAmount':'1.00'}]),[('vat_base_required','lines')],('11.00','11.00'))
case('expense-document-required',lambda r,f,v:line(r,0,accountNo='cost'),[('document_required','documentId')],doc=True)
case('expense-document-missing',lambda r,f,v:(line(r,0,accountNo='cost'),r.update(documentId='2')),[('document_missing','documentId')],doc=True)
for id in ['1','0','-1','9007199254740993','9223372036854775807','-9223372036854775808']:
 case('document-'+id,lambda r,f,v,id=id:(line(r,0,accountNo='cost'),r.update(documentId=id),f['documents'].append({'id':id,'existingCreditNoteEntry':None})),doc=True,legacy='difference' if id in ['0','9007199254740993','9223372036854775807','-9223372036854775808'] else 'match')
 case('bank-missing-'+id,lambda r,f,v,id=id:r.update(sourceBankTransactionId=id),[('bank_missing','sourceBankTransactionId')],legacy='difference' if id=='0' else 'match')
 case('bank-linked-'+id,lambda r,f,v,id=id:(r.update(sourceBankTransactionId=id),f['banks'].append({'id':id,'linkedJournal':'-9223372036854775808'})),[('bank_reconciled','sourceBankTransactionId')],legacy='difference' if id in ['0','9007199254740993','9223372036854775807','-9223372036854775808'] else 'match')
case('credit-note-all-accounts',lambda r,f,v:(r.update(documentId='0'),f['documents'].append({'id':'0','existingCreditNoteEntry':'credit-æ\0'})),[('credit_note_already_posted','documentId')])
for code in ['DK_SALE_25','DK_PURCHASE_25','EU_SERVICE_REVERSE_CHARGE','EU_GOODS_ACQUISITION','NON_EU_SERVICE_REVERSE_CHARGE','REPRESENTATION_SPECIAL','REPRESENTATION_NON_DEDUCTIBLE_VAT','DK_BAD_DEBT_25','REVERSE_CHARGE_EXEMPT','DOMESTIC_REVERSE_CHARGE_EXEMPT','OSS_EU_CONSUMER','DK_PURCHASE_EXEMPT','DK_SALE_EXEMPT']:
 case('vat-base-'+code,lambda r,f,v,code=code:(line(r,0,accountNo='cost',vatCode='\ufeff'+code+'\ufeff'),line(r,1,accountNo='tax'),r.update(documentId='1'),f['documents'].append({'id':'1','existingCreditNoteEntry':None})),doc=True)
case('vat-case-sensitive',lambda r,f,v:(line(r,0,accountNo='cost',vatCode='dk_sale_25'),line(r,1,accountNo='tax')),[('vat_code_unsupported','lines[0].vatCode'),('vat_base_required','lines'),('document_required','documentId')],doc=True)
case('code-on-control-not-base',lambda r,f,v:line(r,0,accountNo='tax',vatCode='DK_SALE_25'),[('vat_base_required','lines')])
case('blank-vat-code',lambda r,f,v:line(r,0,vatCode='\ufeff\t '))
case('non-js-vat-whitespace',lambda r,f,v:line(r,0,vatCode='\u0085'),[('vat_code_unsupported','lines[0].vatCode')])
case('fx-valid',lambda r,f,v:r.update(currency='EUR',amountForeign='10.00',amountDkk='74.60',fxRateToDkk='7.460000'),fx=True)
case('fx-line-total-independent',lambda r,f,v:r.update(currency='EUR',amountForeign='1.00',amountDkk='7.46',fxRateToDkk='7.460000'),fx=True)
case('fx-all-missing',lambda r,f,v:r.update(currency='EUR'),[('foreign_amount_required','amountForeign'),('dkk_amount_required','amountDkk'),('fx_rate_required','fxRateToDkk')],fx=True)
case('fx-negative',lambda r,f,v:r.update(currency='EUR',amountForeign='-1.00',amountDkk='-1.00',fxRateToDkk='1.000000'),[('foreign_amount_required','amountForeign'),('dkk_amount_required','amountDkk')],fx=True)
case('fx-mismatch',lambda r,f,v:r.update(currency='EUR',amountForeign='1.00',amountDkk='7.45',fxRateToDkk='7.460000'),[('fx_amount_mismatch','amountDkk')],fx=True)
case('fx-exact-half-correction',lambda r,f,v:r.update(currency='USD',amountForeign='1.15',amountDkk='0.12',fxRateToDkk='0.100000'),fx=True,legacy='difference')
case('fx-binary-old-half-refused',lambda r,f,v:r.update(currency='USD',amountForeign='1.15',amountDkk='0.11',fxRateToDkk='0.100000'),[('fx_amount_mismatch','amountDkk')],fx=True,legacy='difference')
case('dkk-ignores-typed-fx',lambda r,f,v:r.update(amountForeign='-1.00',amountDkk='0.00',fxRateToDkk='1.000000'))
for name,change in [('snapshot-date',lambda r,f,v:f.update(transactionDate='2026-02-02')),('duplicate-account',lambda r,f,v:f['accounts'].append(copy.deepcopy(f['accounts'][0]))),('duplicate-document',lambda r,f,v:f.update(documents=[{'id':'0','existingCreditNoteEntry':None}]*2)),('duplicate-bank',lambda r,f,v:f.update(banks=[{'id':'0','linkedJournal':None}]*2)),('duplicate-period',lambda r,f,v:f.update(periods=[period(),period()])),('period-range',lambda r,f,v:f.update(periods=[period(start='2027-01-01')]))]:case(name,change,error='invalid_context',legacy='native_only')
for name,change in [('numeric-money',lambda r,f,v:line(r,0,debitAmount=10)),('numeric-id',lambda r,f,v:r.update(documentId=1)),('bad-date',lambda r,f,v:r.update(transactionDate=' 2026-02-01')),('bad-currency',lambda r,f,v:r.update(currency='eur')),('zero-rate',lambda r,f,v:r.update(fxRateToDkk='0.000000')),('unknown-field',lambda r,f,v:r.update(historicalImport=True)),('empty-account',lambda r,f,v:line(r,0,accountNo=''))]:case(name,change,error='invalid_input',legacy='native_only')
# Known exact sums; Decimal expectation is independent of both implementations.
for i in range(1,101):
 with localcontext() as c:
  c.prec=200
  values=[Decimal(i)/100,Decimal(i*7)/100,Decimal(i*17)/100]
  if i==100:values=[Decimal('9007199254740993.01'),Decimal('0.01'),Decimal('0.01')]
  total=sum(values);money=lambda x:format(x.quantize(Decimal('.01'),rounding=ROUND_HALF_UP),'f')
  lines=[{'accountNo':'cash','debitAmount':money(x)} for x in values]+[{'accountNo':'capital','creditAmount':money(total)}]
  case('exact-sum-'+str(i),lambda r,f,v,lines=lines:r.update(lines=lines),totals=(money(total),money(total)),legacy='difference' if i==100 else 'match')
for value in ['0','-1','9007199254740993','9223372036854775807','-9223372036854775808','+1','-0','01','1.0','1e0',' 1','9223372036854775808','-9223372036854775809']:
 valid=value in ['0','-1','9007199254740993','9223372036854775807','-9223372036854775808'];vs.append({'id':'id-'+value,'op':'identifier','input':value,'legacy':'native_only','expected':value if valid else {'error':'invalid_identifier'}})
for value in ['','æ\0🦀',' 100 ']:vs.append({'id':'account-'+value,'op':'account','input':value,'legacy':'native_only','expected':value if value else {'error':'invalid_account_no'}})
for name,mutate,error in [('reverse-order',lambda b:b.reverse(),None),('duplicate-bundle',lambda b:b.append(copy.deepcopy(b[0])),'invalid_rules'),('missing-bookkeeping',lambda b:b.__setitem__(slice(None),[x for x in b if x['name']!='bookkeeping']),'invalid_rules'),('missing-rule',lambda b:next(x for x in b if x['name']=='bookkeeping')['ruleIds'].remove(B+'BALANCED-001'),'invalid_rules'),('unknown-bookkeeping',lambda b:next(x for x in b if x['name']=='bookkeeping').update(version='dk-bookkeeping-v99'),'unsupported_rule_version'),('unknown-vat',lambda b:next(x for x in b if x['name']=='vat').update(version='dk-vat-v99'),'unsupported_rule_version'),('duplicate-rule',lambda b:next(x for x in b if x['name']=='bookkeeping')['ruleIds'].append(B+'BALANCED-001'),'invalid_rules'),('empty-version',lambda b:b[0].update(version=''),'invalid_rules')]:
 b=copy.deepcopy(bundles);mutate(b);vs.append({'id':'rules-'+name,'op':'rules','bundles':b,'legacy':'native_only','expected':{'error':error} if error else version})
vs.append({'id':'rules-stale','op':'rules','expectedVersion':'old','legacy':'native_only','expected':{'error':'rule_version_mismatch'}})
case('manual-stale',lambda r,f,v:v.update(expectedVersion='old'),error='rule_version_mismatch',legacy='native_only')
for v in vs:
 if v['id']=='period-order':v['expectedMessages']=['transactionDate 2026-02-01 falls in reported period fiscal_year 2026-01-01..2026-06-30']
 if v['id']=='period-lock':v['expectedMessages']=['transactionDate 2026-02-01 falls in closed period custom 2026-01-01..2026-12-31 ref x\0æ']
raw=''.join(json.dumps(v,ensure_ascii=False,separators=(',',':'))+'\n' for v in vs).encode();manifest={'format':'rentemester-manual-journal-vectors/1','checks':len(vs),'generator_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'vectors_sha256':hashlib.sha256(raw).hexdigest(),'rule_bundles_sha256':hashlib.sha256((OUT/'rule-bundles.json').read_bytes()).hexdigest(),'legacy_policies':{p:sum(v['legacy']==p for v in vs) for p in ['match','difference','native_only']}}
if '--check' in sys.argv:
 assert (OUT/'vectors.jsonl').read_bytes()==raw
 assert json.loads((OUT/'vectors-manifest.json').read_text())==manifest
else:
 (OUT/'vectors.jsonl').write_bytes(raw);(OUT/'vectors-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest))
