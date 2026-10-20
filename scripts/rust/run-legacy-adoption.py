#!/usr/bin/env python3
"""Owned actual TS -> native accounting -> signed source-free recovery, including an archived ledger."""
import argparse, base64, hashlib, json, os, secrets, shutil, signal, socket, sqlite3, subprocess, tempfile, time, urllib.request
from pathlib import Path
REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Adoption-'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--server-binary',type=Path,required=True);p.add_argument('--web-dir',type=Path,required=True);p.add_argument('--evidence',type=Path,required=True);args=p.parse_args()
 if os.uname().sysname!='Darwin' or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError('Ordinary macOS user required')
 if os.path.lexists(args.evidence):raise RuntimeError('Evidence must be new')
 os.umask(0o077);parent=(Path.home()/'Library/Application Support').resolve(strict=True);work=Path(tempfile.mkdtemp(prefix=PREFIX,dir=parent));identity=work.stat();child=None;log=None
 def owned():
  st=work.lstat();assert work.parent==parent and work.name.startswith(PREFIX) and not work.is_symlink() and (st.st_dev,st.st_ino)==(identity.st_dev,identity.st_ino) and st.st_uid==os.getuid()
 def stop():
  nonlocal child,log
  if child is not None:
   if child.poll() is None:child.send_signal(signal.SIGTERM)
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   child=None
  if log:log.close();log=None
 report={'increment':90,'status':'running','wholeMissionComplete':False,'work':str(work),'sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'phases':[]}
 def save():args.evidence.parent.mkdir(mode=0o700,parents=True,exist_ok=True);args.evidence.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
 def capture(root):
  result={};manifest=json.loads((root/'workspace.json').read_text());result['manifest']=manifest
  for c in manifest['companies']:
   slug=c['slug'];db=sqlite3.connect('file:'+str(root/slug/'data/ledger.sqlite')+'?mode=ro',uri=True);db.row_factory=sqlite3.Row
   result[slug]={t:[dict(r) for r in db.execute('SELECT * FROM '+t+' ORDER BY id')] for t in ['journal_entries','journal_lines','documents']}
   result[slug]['ledgerUuid']=db.execute('SELECT ledger_uuid FROM ledger_identity WHERE id=1').fetchone()[0]
   for doc in result[slug]['documents']:assert sha(root/slug/doc['stored_path'])==doc['sha256_hash']
   db.close()
  db=sqlite3.connect('file:'+str(root/'.rentemester/workspace-control.sqlite')+'?mode=ro',uri=True);db.row_factory=sqlite3.Row
  result['adoptionAudit']=[dict(r)for r in db.execute("SELECT * FROM workspace_audit WHERE event_type IN ('legacy_adoption_receipt','legacy_control_adoption') ORDER BY id")];db.close();return result
 try:
  owned();binary=work/'rentemester-server';shutil.copyfile(args.server_binary.resolve(strict=True),binary);binary.chmod(0o700);web=work/'web';shutil.copytree(args.web_dir.resolve(strict=True),web)
  report.update(serverBinarySha256=sha(binary),webFilesSha256={str(f.relative_to(web)):sha(f)for f in sorted(web.rglob('*'))if f.is_file()},helperSha256={name:sha(REPO/'scripts/rust'/name)for name in ['create-legacy-adoption-fixture.ts','check-legacy-adoption.ts','run-legacy-adoption.py']});save()
  env={**os.environ,'RM_ADOPTION_WORK':str(work)};subprocess.run(['bun','run','scripts/rust/create-legacy-adoption-fixture.ts'],cwd=REPO,env=env,check=True)
  corpus=json.loads((work/'legacy-corpus.json').read_text());report['legacyCorpusSha256']=sha(work/'legacy-corpus.json')
  subprocess.run(['bun','-e',"const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});",str(work/'signing.der'),str(work/'trusted.pub')],check=True)
  key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
  def config(root,workspace):return{'root':str(root),'runtimeProfile':'local-user-macos','brokerUid':os.getuid(),'workspace':workspace,'companySlug':'synthetic','companyName':'Synthetic Adoption Company','cookieKey':key(),'mfaKey':key(),'operatorActors':['system:synthetic-installer'],'firstOwner':{'actor':'system:synthetic-installer','email':('restored'if root.name=='restored'else'source')+'-owner@example.invalid','name':'Synthetic Owner','password':key()}}
  current=config(work/'adopted','synthetic-adoption-source');current['backupSigningKeyFile']=str(work/'signing.der');config_file=work/'server.json'
  def write():owned();config_file.write_text(json.dumps(current));config_file.chmod(0o600)
  write()
  cmd=[str(binary),'--adopt-legacy',str(work/'legacy'),'--config',str(config_file),'--confirm-target',str(work/'adopted'),'--legacy-source-stopped']
  with (work/'adoption.stderr').open('w')as error:
   child=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=error,text=True,bufsize=1);reviews=[]
   with (work/'adoption.stdout').open('w')as output:
    for line in child.stdout:
     output.write(line)
     if not line.startswith('{'):continue
     value=json.loads(line)
     if value.get('stage')in['retire-source','publish-target']:
      assert hashlib.sha256(json.dumps(value['review'],separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==value['reviewHash']
      reviews.append({'stage':value['stage'],'sha256':value['reviewHash']});child.stdin.write(value['reviewHash']+'\n');child.stdin.flush()
   if child.wait(timeout=60):raise RuntimeError('Adoption refused: '+(work/'adoption.stderr').read_text())
   child=None
  assert [r['stage']for r in reviews]==['retire-source','publish-target'];report['reviews']=reviews
  adopted=capture(work/'adopted')
  for company in corpus['companies']:
   rows=adopted[company['slug']];assert rows['journal_entries']==company['journalEntries'] and rows['journal_lines']==company['journalLines'] and rows['ledgerUuid']==company['ledgerUuid']
  assert len(adopted['adoptionAudit'])==2;report['adoptedBusinessSha256']=hashlib.sha256(json.dumps(adopted,sort_keys=True).encode()).hexdigest();save()
  with socket.socket()as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
  origin=f'http://127.0.0.1:{port}'
  def start(extra=()):
   nonlocal child,log
   owned();log=(work/'server.log').open('ab');child=subprocess.Popen([str(binary),'--config',str(config_file),'--web-dir',str(web),'--listen',f'127.0.0.1:{port}',*extra],cwd=REPO,stdout=log,stderr=log)
   for _ in range(300):
    if child.poll()is not None:raise RuntimeError('Owned server exited: '+(work/'server.log').read_text()[-4000:])
    try:
     with urllib.request.urlopen(origin+'/api/health',timeout=.2)as response:
      if json.load(response).get('workspaceReady'):return
    except Exception:pass
    time.sleep(.1)
   raise RuntimeError('Readiness timed out')
  def check(phase):
   subprocess.run(['bun','run','scripts/rust/check-legacy-adoption.ts'],cwd=REPO,env={**env,'RM_ADOPTION_BASE':origin,'RM_ADOPTION_PHASE':phase},check=True);report['phases'].append(json.loads((work/(phase+'-pass.json')).read_text()));save()
  start();check('source');stop();start();check('source-reopen');stop();frozen=capture(work/'adopted');(work/'business-before-recovery.json').write_text(json.dumps(frozen,ensure_ascii=False,indent=2))
  archive=work/'portable.native-backup.json';assert archive.stat().st_size>0;report['backupSha256']=sha(archive);report['trustedPublicKeySha256']=sha(work/'trusted.pub')
  # Only these exact owned synthetic roots are removed. Recovery has no source/key fallback.
  for name in ['legacy','adopted']:owned();shutil.rmtree(work/name)
  (work/'signing.der').unlink();assert not(work/'legacy').exists()and not(work/'adopted').exists()and not(work/'signing.der').exists();report['sourceAndSigningKeyUnavailableBeforeVerification']=True;save()
  subprocess.run([str(binary),'--verify-backup',str(archive),'--trusted-backup-key',str(work/'trusted.pub')],cwd=REPO,check=True)
  package=json.loads(archive.read_text());manifest=next(e for e in package['entries']if e['path']=='workspace.json');assert json.loads(base64.b64decode(manifest['bytesBase64'],validate=True))==frozen['manifest']
  current=config(work/'restored','synthetic-adoption-source');current['restoreActor']='system:synthetic-installer';write();start(['--restore-backup',str(archive),'--trusted-backup-key',str(work/'trusted.pub'),'--confirm-target',str(work/'restored'),'--source-unavailable']);check('restored');stop()
  restored=capture(work/'restored');assert restored['manifest']==frozen['manifest'];assert restored['synthetic-archived']==frozen['synthetic-archived'];assert restored['adoptionAudit']==frozen['adoptionAudit'];assert restored['synthetic']['journal_entries'][:2]==frozen['synthetic']['journal_entries'];assert len(restored['synthetic']['journal_entries'])==3
  start();check('restored-reopen');stop();assert capture(work/'restored')==restored;report.update(status='pass',preservedArchivedLedger=True,sourceFreeRestoredAdoptionAuditExact=True,normalFreshOwnerMfaBothInstallations=True,newNativeAccountingAfterAdoptionAndRestore=True,noDuplicateOldIntent=True,ownedProcessesStopped=True,backupIndependentVerification=True);save();print('PASS: actual legacy adoption, new native accounting, archived signed backup, source-free restore, fresh owner/MFA and restart; '+str(args.evidence))
 except Exception as error:
  stop();report.update(status='failed',error=str(error),ownedProcessesStopped=True);save();raise
 finally:stop()
if __name__=='__main__':main()
