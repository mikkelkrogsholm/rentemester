"""Independent fixed access expectations; no product imports or generated outputs as oracle."""
from pathlib import Path
from itertools import product
import json, hashlib, sys
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'docs/architecture/rust-target/implementation/access-policy'
PERMISSIONS = ['public.read','public.invitation.claim','workspace.read','workspace.group.read','workspace.manage','workspace.members.read','workspace.members.manage','company.read','company.documents.read','company.documents.upload','company.master-data','company.draft.write','company.ledger.post','company.review','company.period.force-close','company.export','company.external-lookup','company.external-send','company.admin','company.knowledge.read','company.knowledge.manage','company.ownership.read','company.ownership.manage']
WORKSPACE = {'workspace_owner': set(PERMISSIONS[:7]), 'member': {'workspace.read'}}
COMPANY = {
 'owner': set(PERMISSIONS[7:]),
 'bookkeeper': {'company.read','company.documents.read','company.documents.upload','company.master-data','company.draft.write','company.ledger.post','company.export','company.external-lookup','company.knowledge.read','company.knowledge.manage','company.ownership.read','company.ownership.manage'},
 'reviewer': {'company.read','company.documents.read','company.review','company.export','company.knowledge.read','company.knowledge.manage','company.ownership.read','company.ownership.manage'},
 'reader': {'company.read','company.documents.read','company.export','company.knowledge.read','company.ownership.read'},
 None: set(),
}
STATES = {
 'human': dict(kind='human', authenticated=True, verified=True, mfa=True, service=False, live=False),
 'unverified': dict(kind='human', authenticated=True, verified=False, mfa=True, service=False, live=False),
 'no-mfa': dict(kind='human', authenticated=True, verified=True, mfa=False, service=False, live=False),
 'stale-session': dict(kind='human', authenticated=False, verified=True, mfa=True, service=False, live=False),
 'service': dict(kind='service', authenticated=True, verified=False, mfa=False, service=True, live=True),
 'unregistered-service': dict(kind='service', authenticated=True, verified=False, mfa=False, service=False, live=True),
 'revoked-credential': dict(kind='service', authenticated=True, verified=False, mfa=False, service=True, live=False),
 'missing-identity': dict(kind='missing', authenticated=False, verified=False, mfa=False, service=False, live=False),
 'human-service-bypass': dict(kind='human', authenticated=True, verified=False, mfa=False, service=True, live=False),
}
FAULTS = ['', 'identity-workspace','access-workspace','access-subject','company-workspace','membership-workspace','membership-subject','membership-company','request-company','empty-workspace','empty-subject','no-workspace','disabled','no-company','unregistered','archived','revoked','bad-slug','no-request-company']
def expected(p, wr, cr, state, fault, native):
 if p.startswith('public.'): return True
 ready = state['kind'] != 'missing' and ((state['verified'] and state['mfa']) or state['service'])
 if native:
  ready = state['authenticated'] and (state['verified'] and state['mfa'] if state['kind']=='human' else state['kind']=='service' and state['service'] and state['live'])
  if fault in {'identity-workspace','access-workspace','access-subject','empty-workspace','empty-subject'}: ready=False
 if not ready or fault in {'no-workspace','disabled'}: return False
 if p.startswith('workspace.'): return p in WORKSPACE[wr]
 if fault in {'request-company','no-company','unregistered','archived','revoked','bad-slug','no-request-company'}: return False
 if native and fault in {'company-workspace','membership-workspace','membership-subject','membership-company'}: return False
 return p in COMPANY[cr]
vectors=[]
for p,wr,cr,state in product(PERMISSIONS,WORKSPACE,COMPANY,STATES):
 v=dict(id=f'matrix-{len(vectors)}',permission=p,workspaceRole=wr,companyRole=cr,state=state,**STATES[state],fault='')
 v.update(expected=expected(p,wr,cr,STATES[state],'',True),legacyExpected=expected(p,wr,cr,STATES[state],'',False));vectors.append(v)
for p, fault in product(PERMISSIONS,FAULTS[1:]):
 v=dict(id=f'binding-{len(vectors)}',permission=p,workspaceRole='workspace_owner',companyRole='owner',state='human',**STATES['human'],fault=fault)
 v.update(expected=expected(p,'workspace_owner','owner',STATES['human'],fault,True),legacyExpected=expected(p,'workspace_owner','owner',STATES['human'],fault,False));vectors.append(v)
for v in vectors:
 v.update(workspaceRoleExpected=v['permission'] in WORKSPACE[v['workspaceRole']],companyRoleExpected=v['permission'] in COMPANY[v['companyRole']])
text=''.join(json.dumps(v,ensure_ascii=False,separators=(',',':'))+'\n' for v in vectors)
manifest={'format':'rentemester-access-vectors/1','checks':len(vectors),'vectors_sha256':hashlib.sha256(text.encode()).hexdigest(),'generator_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'legacy_core_differences':sum(v['expected']!=v['legacyExpected'] for v in vectors),'source_sha256':{p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in ['src/core/access-permissions.ts','src/core/workspace-access.ts','src/core/workspace-control.ts']}}
files={'vectors.jsonl':text,'vectors-manifest.json':json.dumps(manifest,indent=2)+'\n'}
for name,data in files.items():
 if '--check' in sys.argv: assert (OUT/name).read_text()==data,name
 else: (OUT/name).write_text(data)
print(json.dumps({'ok':True,'checks':len(vectors),'legacy_core_differences':manifest['legacy_core_differences']}))
