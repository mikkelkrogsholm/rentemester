#!/usr/bin/env python3
"""Owned synthetic J05: live signed snapshot, source unavailable, independent verification and fresh restore."""
import argparse,base64,hashlib,json,os,secrets,shutil,signal,socket,subprocess,time,urllib.request,uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def ownership_fixture_evidence(package,source,previous):
 """Compare fixture facts AFTER the native verifier accepted this signed artifact."""
 entries=[entry for entry in package['entries']if entry['path']=='control/ownership.json']
 if len(entries)!=1:raise RuntimeError('Verified package missing one ownership section')
 section=json.loads(base64.b64decode(entries[0]['bytesBase64'],validate=True))
 tables=['rm_ownership_source_snapshots','rm_ownership_snapshot_events','rm_ownership_facts','rm_ownership_fact_events']
 if section.get('format')!='rentemester-native-ownership/1'or set(section['tables'])!=set(tables):raise RuntimeError('Unsupported ownership fixture codec')
 if previous is not None and section!=previous:raise RuntimeError('Independent archives changed ownership source facts/IDs')
 def rows(name):
  table=section['tables'][name]
  return[dict(zip(table['columns'],[cell.get('value')for cell in row],strict=True))for row in table['rows']]
 snapshots=source['ownership']['synthetic']['history']['history'];expected={s['snapshotId']:s for s in snapshots}
 observed=rows(tables[0])
 if len(observed)!=len(expected)or{r['snapshot_id']for r in observed}!=set(expected):raise RuntimeError('Ownership snapshot inventory differs')
 for row in observed:
  s=expected[row['snapshot_id']]
  if any(row[col]!=s[field]for col,field in [('source','source'),('observed_at','observedAt'),('snapshot_hash','snapshotHash'),('diff_hash','diffHash'),('actor','actor')])or row['principal_kind']!=s['principal']['kind']or row['principal_id']!=s['principal']['id']or json.loads(row['diff_json'])!=s['diff']or json.loads(row['canonical_facts'])!=source['ownershipSnapshotFacts'][s['snapshotId']]or row['created_at']!=s['history'][0]['created_at']:raise RuntimeError('Ownership canonical observation/diff/hash/provenance differs')
 events=rows(tables[1]);history={s['snapshotId']:[]for s in snapshots}
 for row in events:
  if row['snapshot_id']not in history:raise RuntimeError('Ownership history refers to unknown snapshot')
  history[row['snapshot_id']].append({key:row[key]for key in ['event_type','actor','principal_kind','principal_id','created_at']})
 if any(history[s['snapshotId']]!=s['history']for s in snapshots):raise RuntimeError('Ownership full native event history differs')
 facts=rows(tables[2]);additions=[(snapshot_id,fact)for snapshot_id,items in source['ownershipAppliedAdditions'].items()for fact in items]
 if len(facts)!=len(additions):raise RuntimeError('Ownership applied fact inventory differs')
 for row in facts:
  fact=json.loads(row['canonical_fact']);key=(row['snapshot_id'],fact)
  if key not in additions or row['review_state']!='approved':raise RuntimeError('Ownership canonical applied observation differs')
  additions.remove(key)
 if additions:raise RuntimeError('Ownership applied observations missing')
 endings=rows(tables[3]);expected_endings=source['ownershipEndings']
 actual=[{'factHash':r['fact_hash'],'effectiveToExclusive':r['effective_to_exclusive'],'successorFactHash':r['successor_fact_hash']}for r in endings]
 if actual!=expected_endings:raise RuntimeError('Ownership explicit append-only endings differ')
 for row in endings:
  applied=next((h for h in expected[row['snapshot_id']]['history']if h['event_type']=='applied'),None)
  if applied is None or row['event_type']!='ended'or any(row[k]!=applied[k]for k in ['actor','principal_kind','principal_id','created_at']):raise RuntimeError('Ownership ending apply provenance differs')
 return section,{'sourceSectionSha256':hashlib.sha256(json.dumps(section,sort_keys=True,separators=(',',':')).encode()).hexdigest(),'tableRows':{name:len(section['tables'][name]['rows'])for name in tables},'exactNativeObservationsHistoryAndEndings':True}
def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--server-binary',type=Path,required=True);p.add_argument('--web-dir',type=Path,required=True)
 p.add_argument('--with-web-download',action='store_true');p.add_argument('--with-company-knowledge',action='store_true')
 p.add_argument('--with-parties',action='store_true',help='Include canonical party history and party-backed knowledge in all phases')
 p.add_argument('--with-inbox',action='store_true',help='Include unassigned/approved/completed native inbox sources and target originals in all phases')
 p.add_argument('--with-corporate',action='store_true',help='Include three corporate original sensitivities and full history; independently restore/restart each supplied archive; implies parties/knowledge')
 p.add_argument('--with-ownership',action='store_true',help='Include ordinary independent invited MFA reviewer and all ownership states/endings; implies corporate/inbox/parties/knowledge')
 p.add_argument('--cli-binary',type=Path);p.add_argument('--mcp-binary',type=Path)
 p.add_argument('--evidence',type=Path,help='NEW report path; never overwrite. Default is a unique private /tmp report.')
 args=p.parse_args()
 if args.with_ownership:args.with_corporate=True;args.with_inbox=True
 if args.with_corporate:args.with_parties=True
 if args.with_parties:args.with_company_knowledge=True
 if bool(args.cli_binary)!=bool(args.mcp_binary):p.error('--cli-binary and --mcp-binary must be supplied together')
 if os.uname().sysname!='Darwin' or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError('Ordinary macOS user required')
 os.umask(0o077)
 out=args.evidence or Path('/tmp/rentemester-rust-reports')/('native-portable-backup-'+uuid.uuid4().hex+'.json')
 out=out.absolute();out.parent.mkdir(parents=True,exist_ok=True)
 # Retain this exclusively created descriptor, including for failure evidence.
 with out.open('x',encoding='utf8')as report:
  parent=(Path.home()/'Library/Application Support').resolve();work=parent/('Rentemester-Synthetic-Portable-'+uuid.uuid4().hex)
  child=None;log=None;owned=None
  helper=REPO/'scripts/rust/check-native-portable-backup.ts';runner=Path(__file__).resolve()
  evidence={'status':'running','profile':'local-user-macos','noOfflineIdentitySeed':True,'globalSingleUseClaim':False,'webDownload':args.with_web_download,'companyKnowledge':args.with_company_knowledge,'parties':args.with_parties,'inbox':args.with_inbox,'corporate':args.with_corporate,'ownership':args.with_ownership,'clientAdapters':bool(args.cli_binary),'sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'helperSha256':{'browser':sha(helper),'runner':sha(runner)},'checks':[],'phases':[]}
  def save():report.seek(0);report.truncate();json.dump(evidence,report,indent=2);report.flush();os.fsync(report.fileno())
  def stop():
   nonlocal child,log
   if child is not None:
    if child.poll()is None:child.send_signal(signal.SIGTERM)
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
    child=None
   if log:log.close();log=None
  try:
   save();work.mkdir(mode=0o700);st=work.lstat();owned=(st.st_dev,st.st_ino)
   binary=work/'rentemester-server';shutil.copyfile(args.server_binary.resolve(strict=True),binary);binary.chmod(0o700)
   web=work/'web';shutil.copytree(args.web_dir.resolve(strict=True),web)
   evidence.update({'binarySha256':sha(binary),'webIndexSha256':sha(web/'index.html'),'webFilesSha256':{str(f.relative_to(web)):sha(f)for f in sorted(web.rglob('*'))if f.is_file()}})
   if args.cli_binary:
    for source,name in [(args.cli_binary,'rentemester-rust'),(args.mcp_binary,'rentemester-mcp')]:
     target=work/name;shutil.copyfile(source.resolve(strict=True),target);target.chmod(0o700)
    evidence['clientBinarySha256']={'cli':sha(work/'rentemester-rust'),'mcp':sha(work/'rentemester-mcp')}
   save()
   # Explicit synthetic operator provisioning OUTSIDE either workspace. Product never generates keys.
   subprocess.run(['/opt/homebrew/bin/bun','-e',"const c=require('node:crypto'),f=require('node:fs');const k=c.generateKeyPairSync('ed25519');const seed=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),pub=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),seed,Buffer.from('812100','hex'),pub]),{mode:0o600});f.writeFileSync(process.argv[2],pub,{mode:0o600});",str(work/'signing.der'),str(work/'trusted.pub')],check=True)
   with socket.socket()as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
   origin=f'http://127.0.0.1:{port}';key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
   config_path=work/'server.json'
   config={'runtimeProfile':'local-user-macos','root':str(work/'source'),'brokerUid':os.getuid(),'workspace':'synthetic-native-portable','companySlug':'synthetic','companyName':'Synthetic Portable Source','cookieKey':key(),'mfaKey':key(),'backupSigningKeyFile':str(work/'signing.der'),'operatorActors':['system:synthetic-installer'],'firstOwner':{'actor':'system:synthetic-installer','email':'source-owner@example.invalid','name':'Synthetic Source Owner','password':'synthetic-portable-password-123'},'accounts':[]}
   def write():config_path.write_text(json.dumps(config));config_path.chmod(0o600)
   def command(extra):return[str(binary),'--config',str(config_path),'--web-dir',str(web),'--listen',f'127.0.0.1:{port}']+extra
   def start(extra):
    nonlocal child,log
    log=open(work/'server.log','ab');child=subprocess.Popen(command(extra),cwd=REPO,stdout=log,stderr=log)
    for _ in range(300):
     if child.poll()is not None:raise RuntimeError('Owned synthetic server exited before readiness: '+(work/'server.log').read_text()[-5000:])
     try:
      with urllib.request.urlopen(origin+'/api/health',timeout=.2)as r:
       if json.load(r).get('workspaceReady'):return
     except Exception:pass
     time.sleep(.1)
    raise RuntimeError('Owned synthetic server readiness timed out')
   def check(phase):
    if sha(helper)!=evidence['helperSha256']['browser']:raise RuntimeError('Pinned browser helper changed during qualification')
    subprocess.run(['/opt/homebrew/bin/bun','run',str(helper)],cwd=REPO,env={**os.environ,'RM_PORTABLE_BASE':origin,'RM_PORTABLE_WORK':str(work),'RM_PORTABLE_PHASE':phase,'RM_PORTABLE_WEB_DOWNLOAD':'1'if args.with_web_download else'0','RM_PORTABLE_KNOWLEDGE':'1'if args.with_company_knowledge else'0','RM_PORTABLE_PARTIES':'1'if args.with_parties else'0','RM_PORTABLE_INBOX':'1'if args.with_inbox else'0','RM_PORTABLE_CORPORATE':'1'if args.with_corporate else'0','RM_PORTABLE_OWNERSHIP':'1'if args.with_ownership else'0','RM_PORTABLE_CLIENTS':'1'if args.cli_binary else'0'},check=True)
    evidence['phases'].append(phase);save()
   def refuse(argv,label):
    r=subprocess.run([str(binary)]+argv,cwd=REPO,capture_output=True,text=True,timeout=30)
    if r.returncode==0:raise RuntimeError(label+' unexpectedly succeeded')
    return{'check':label,'exit':r.returncode}
   write();start(['--create']);check('source');stop()
   archive=work/'portable.native-backup.json';trusted=work/'trusted.pub';evidence['trustedPublicKeySha256']=sha(trusted)
   shutil.rmtree(work/'source');(work/'signing.der').unlink();evidence['sourceUnavailableBeforeVerifyAndRestore']=True
   # Verify each separate timestamped export, never demand two exports be identical.
   names=['portable.native-backup.json']+(['cli.native-backup.json','mcp.native-backup.json']if args.cli_binary else[])
   evidence['verifiedArtifacts']={}
   source_state=(work/'private-state.json').read_bytes();source_facts=json.loads(source_state)
   corporate_inputs=source_facts.get('corporateInputBytes')if args.with_corporate else None
   if args.with_corporate and(not isinstance(corporate_inputs,dict)or len(corporate_inputs)!=3):raise RuntimeError('Expected three corporate original inputs')
   corporate_section=None;ownership_section=None
   inbox_inputs=source_facts.get('inboxInputBytes')if args.with_inbox else None
   if args.with_inbox and(not isinstance(inbox_inputs,dict)or len(inbox_inputs)!=3):raise RuntimeError('Expected three original native inbox inputs')
   for name in names:
    path=work/name
    subprocess.run([str(binary),'--verify-backup',str(path),'--trusted-backup-key',str(trusted)],cwd=REPO,check=True)
    evidence['verifiedArtifacts'][name]={'bytes':path.stat().st_size,'sha256':sha(path),'verifiedAfterSourceUnavailable':True}
    if args.with_inbox:
     # The server independently verified the signed package first. Compare the
     # actual embedded BLOB bytes with known fixture inputs after source deletion;
     # unassigned/approved sources deliberately have no document-file API yet.
     package=json.loads(path.read_text());entries=[entry for entry in package['entries']if entry['path']=='control/workspace-inbox.json']
     if len(entries)!=1:raise RuntimeError('Verified package missing one inbox section')
     section=json.loads(base64.b64decode(entries[0]['bytesBase64'],validate=True))
     if section.get('format')!='rentemester-native-workspace-inbox/1':raise RuntimeError('Unsupported inbox fixture codec')
     table=section['tables']['rm_workspace_inbox_sources'];columns=table['columns'];row_id=columns.index('source_id');row_blob=columns.index('original_bytes');row_hash=columns.index('sha256');actual={}
     for row in table['rows']:
      source_id=row[row_id].get('value')
      if source_id not in inbox_inputs:continue
      cell=row[row_blob]
      if cell.get('type')!='BlobBase64'or source_id in actual:raise RuntimeError('Inbox original BLOB shape/identity differs')
      raw=base64.b64decode(cell['value'],validate=True);expected=base64.b64decode(inbox_inputs[source_id],validate=True)
      digest=hashlib.sha256(raw).hexdigest()
      if raw!=expected or row[row_hash].get('value')!=digest:raise RuntimeError('Verified inbox source bytes differ from actual uploaded fixture original')
      actual[source_id]={'bytes':len(raw),'sha256':digest,'exactInputBytes':True}
     if set(actual)!=set(inbox_inputs):raise RuntimeError('Verified inbox original inventory incomplete')
     evidence['verifiedArtifacts'][name]['inboxOriginals']=actual
    if args.with_corporate:
     # Parsing follows independent native signature/inventory verification. The
     # three archives may differ in audit time, but these source facts must not.
     package=json.loads(path.read_text());entries=[entry for entry in package['entries']if entry['path']=='control/corporate-records.json']
     if len(entries)!=1:raise RuntimeError('Verified package missing one corporate section')
     section=json.loads(base64.b64decode(entries[0]['bytesBase64'],validate=True))
     if section.get('format')!='rentemester-native-corporate-records/1':raise RuntimeError('Unsupported corporate fixture codec')
     if corporate_section is not None and section!=corporate_section:raise RuntimeError('Independent archives changed corporate source facts')
     corporate_section=section
     table=section['tables']['rm_corporate_record_bytes'];columns=table['columns'];actual={}
     for row in table['rows']:
      record_id=row[columns.index('record_id')].get('value')
      if record_id not in corporate_inputs or record_id in actual:raise RuntimeError('Unexpected/duplicate corporate original identity')
      cell=row[columns.index('original_bytes')]
      if cell.get('type')!='BlobBase64':raise RuntimeError('Corporate original is not a BLOB')
      raw=base64.b64decode(cell['value'],validate=True);expected=base64.b64decode(corporate_inputs[record_id],validate=True);digest=hashlib.sha256(raw).hexdigest()
      if raw!=expected or row[columns.index('sha256')].get('value')!=digest:raise RuntimeError('Verified corporate original differs from actual uploaded input')
      actual[record_id]={'bytes':len(raw),'sha256':digest,'exactInputBytes':True}
     if set(actual)!=set(corporate_inputs):raise RuntimeError('Corporate original inventory incomplete')
     events=section['tables']['rm_corporate_record_events'];cols=events['columns'];payloads=source_facts['corporatePayloads'];enriched=[];superseded=[]
     for row in events['rows']:
      if row[cols.index('record_id')].get('value')!=payloads['originalId']:continue
      event=row[cols.index('event_type')].get('value');payload=json.loads(row[cols.index('canonical_payload')]['value'])
      if event=='enriched':enriched.append(payload)
      if event=='superseded':superseded.append(payload)
     if enriched!=[{'assertion':payloads['assertion']}]or superseded!=[{'replacementRecordId':payloads['replacementRecordId'],'reason':reason}for reason in payloads['reasons']]:raise RuntimeError('Corporate enrichment/repeated supersession payloads changed')
     evidence['verifiedArtifacts'][name]['corporateOriginals']=actual
     evidence['verifiedArtifacts'][name]['corporateSourceSectionSha256']=hashlib.sha256(json.dumps(section,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    if args.with_ownership:
     ownership_section,ownership_evidence=ownership_fixture_evidence(json.loads(path.read_text()),source_facts,ownership_section)
     evidence['verifiedArtifacts'][name]['ownership']=ownership_evidence
   save()
   tamper=work/'tampered.json';wire=json.loads(archive.read_text());wire['signatureBase64']=base64.b64encode(bytes(64)).decode();tamper.write_text(json.dumps(wire))
   evidence['checks'].append(refuse(['--verify-backup',str(tamper),'--trusted-backup-key',str(trusted)],'tampered signature'))
   wrong=work/'wrong.pub';wrong.write_bytes(secrets.token_bytes(32));evidence['checks'].append(refuse(['--verify-backup',str(archive),'--trusted-backup-key',str(wrong)],'wrong separate key'))
   config['root']=str(work/'restored');config['cookieKey']=key();config['mfaKey']=key();config.pop('backupSigningKeyFile');config['restoreActor']='system:synthetic-installer';config['firstOwner']['email']='restored-owner@example.invalid';config['firstOwner']['name']='Synthetic Restored Owner';write()
   flags=['--restore-backup',str(archive),'--trusted-backup-key',str(trusted),'--confirm-target',config['root'],'--source-unavailable']
   evidence['checks'].append(refuse(command(flags[:-1])[1:],'missing source-unavailable declaration'))
   (work/'restored').mkdir(mode=0o700);sentinel=work/'restored/sentinel';sentinel.write_text('unchanged synthetic existing target')
   evidence['checks'].append(refuse(command(flags)[1:],'existing target'))
   if sentinel.read_text()!='unchanged synthetic existing target':raise RuntimeError('Existing target changed')
   shutil.rmtree(work/'restored')
   evidence['restoredArtifacts']={}
   restored_identity=None
   for index,name in enumerate(names if args.with_corporate else names[:1]):
    # Every artifact gets a fresh target, ordinary new-owner verification/MFA,
    # fresh cookie/MFA keys and the original source expectations. Never adopt
    # prior restored authority or carry a prior test's journal mutation forward.
    if index:
     target=work/'restored';st=target.lstat()
     if target.is_symlink()or(st.st_dev,st.st_ino)!=restored_identity or target.parent!=work:raise RuntimeError('Restored fixture cleanup identity changed')
     shutil.rmtree(target)
    (work/'private-state.json').write_bytes(source_state)
    config['cookieKey']=key();config['mfaKey']=key();config['firstOwner']={'actor':'system:synthetic-installer','email':'restored-owner@example.invalid','name':'Synthetic Restored Owner','password':'synthetic-portable-password-123'};write()
    flags=['--restore-backup',str(work/name),'--trusted-backup-key',str(trusted),'--confirm-target',config['root'],'--source-unavailable']
    evidence['activeRestoreArtifact']=name;save()
    if(work/'source').exists()or(work/'signing.der').exists()or sha(trusted)!=evidence['trustedPublicKeySha256']or sha(work/name)!=evidence['verifiedArtifacts'][name]['sha256']:raise RuntimeError('Source unavailable or independent trust/artifact boundary changed')
    start(flags);st=(work/'restored').lstat();restored_identity=(st.st_dev,st.st_ino);check('restored');stop()
    config.pop('firstOwner');write();start([]);check('reopen');stop()
    restored_state=json.loads((work/'private-state.json').read_text())
    evidence['restoredArtifacts'][name]={'normalNewOwnerMfa':True,'restart':True,'oldKeyRefusedWithoutEffect':True,'freshKeyNewIntentPosted':True,'corporate':restored_state.get('corporateEvidence'),'ownership':restored_state.get('ownershipEvidence'),'parties':restored_state.get('partiesEvidence'),'knowledge':restored_state.get('knowledgeEvidence'),'inbox':restored_state.get('inboxEvidence'),'originalSha256':restored_state['documentHash'],'invoicePdfSha256':restored_state['pdfHash'],'companyUuids':{slug:v['uuid']for slug,v in restored_state['snapshot'].items()}}
    save()
   evidence.pop('activeRestoreArtifact',None)
   state=json.loads((work/'private-state.json').read_text());evidence.update({'status':'passed','normalVerificationAndMfa':True,'restart':True,'oldKeyRefusedWithoutEffect':True,'freshKeyNewIntentPosted':True,'download':state.get('download'),'knowledge':state.get('knowledgeEvidence'),'partyEvidence':state.get('partiesEvidence'),'inboxEvidence':state.get('inboxEvidence'),'corporateEvidence':state.get('corporateEvidence'),'ownershipEvidence':state.get('ownershipEvidence'),'clients':state.get('clientEvidence'),'companyUuids':{s:v['uuid']for s,v in state['snapshot'].items()},'originalSha256':state['documentHash'],'invoicePdfSha256':state['pdfHash']})
   save()
  except BaseException as error:
   evidence.update({'status':'failed','failureType':type(error).__name__});save();raise
  finally:
   try:
    stop()
    if owned is not None:
     st=work.lstat()
     if work.is_symlink()or(st.st_dev,st.st_ino)!=owned or work.parent!=parent or not work.name.startswith('Rentemester-Synthetic-Portable-'):raise RuntimeError('Owned cleanup identity changed; refused deletion')
     shutil.rmtree(work)
    evidence['ownedCleanupCompleted']=True;save()
   except BaseException:
    evidence.update({'status':'failed','ownedCleanupCompleted':False});save();raise
  if evidence['status']=='passed':print('PASS: J05 live portable backup, independent trust, fresh restore, normal owner/MFA, originals and receipt boundary/restart; '+str(out))
if __name__=='__main__':main()
