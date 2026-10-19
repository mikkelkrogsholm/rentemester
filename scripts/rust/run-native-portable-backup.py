#!/usr/bin/env python3
"""Owned synthetic J05: live signed snapshot, source unavailable, independent verification and fresh restore."""
import argparse,base64,hashlib,json,os,secrets,shutil,signal,socket,subprocess,time,urllib.request,uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--server-binary',type=Path,required=True);p.add_argument('--web-dir',type=Path,required=True)
 p.add_argument('--with-web-download',action='store_true');p.add_argument('--with-company-knowledge',action='store_true')
 p.add_argument('--with-parties',action='store_true',help='Include canonical party history and party-backed knowledge in all phases')
 p.add_argument('--cli-binary',type=Path);p.add_argument('--mcp-binary',type=Path)
 p.add_argument('--evidence',type=Path,help='NEW report path; never overwrite. Default is a unique private /tmp report.')
 args=p.parse_args()
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
  evidence={'status':'running','profile':'local-user-macos','noOfflineIdentitySeed':True,'globalSingleUseClaim':False,'webDownload':args.with_web_download,'companyKnowledge':args.with_company_knowledge,'parties':args.with_parties,'clientAdapters':bool(args.cli_binary),'sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'helperSha256':{'browser':sha(helper),'runner':sha(runner)},'checks':[],'phases':[]}
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
    subprocess.run(['/opt/homebrew/bin/bun','run',str(helper)],cwd=REPO,env={**os.environ,'RM_PORTABLE_BASE':origin,'RM_PORTABLE_WORK':str(work),'RM_PORTABLE_PHASE':phase,'RM_PORTABLE_WEB_DOWNLOAD':'1'if args.with_web_download else'0','RM_PORTABLE_KNOWLEDGE':'1'if args.with_company_knowledge else'0','RM_PORTABLE_PARTIES':'1'if args.with_parties else'0','RM_PORTABLE_CLIENTS':'1'if args.cli_binary else'0'},check=True)
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
   for name in names:
    path=work/name
    subprocess.run([str(binary),'--verify-backup',str(path),'--trusted-backup-key',str(trusted)],cwd=REPO,check=True)
    evidence['verifiedArtifacts'][name]={'bytes':path.stat().st_size,'sha256':sha(path),'verifiedAfterSourceUnavailable':True}
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
   shutil.rmtree(work/'restored');start(flags);check('restored');stop()
   config.pop('firstOwner');write();start([]);check('reopen');stop()
   state=json.loads((work/'private-state.json').read_text());evidence.update({'status':'passed','normalVerificationAndMfa':True,'restart':True,'oldKeyRefusedWithoutEffect':True,'freshKeyNewIntentPosted':True,'download':state.get('download'),'knowledge':state.get('knowledgeEvidence'),'partyEvidence':state.get('partiesEvidence'),'clients':state.get('clientEvidence'),'companyUuids':{s:v['uuid']for s,v in state['snapshot'].items()},'originalSha256':state['documentHash'],'invoicePdfSha256':state['pdfHash']})
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
