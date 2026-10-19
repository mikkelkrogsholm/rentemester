#!/usr/bin/env python3
"""Owned synthetic J05: live signed snapshot, source unavailable, independent verification and fresh restore."""
import argparse,base64,hashlib,json,os,secrets,shutil,signal,socket,subprocess,time,urllib.request,uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--server-binary',type=Path,required=True);p.add_argument('--web-dir',type=Path,required=True);p.add_argument('--with-web-download',action='store_true');args=p.parse_args()
 if os.uname().sysname!='Darwin' or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError('Ordinary macOS user required')
 os.umask(0o077);parent=(Path.home()/'Library/Application Support').resolve();work=parent/('Rentemester-Synthetic-Portable-'+uuid.uuid4().hex);work.mkdir(mode=0o700)
 binary=work/'rentemester-server';shutil.copyfile(args.server_binary.resolve(strict=True),binary);binary.chmod(0o700);web=work/'web';shutil.copytree(args.web_dir.resolve(strict=True),web);config_path=work/'server.json';child=None;log=None
 # Explicit synthetic operator provisioning OUTSIDE either workspace. Product never generates keys.
 subprocess.run(['/opt/homebrew/bin/bun','-e',"const c=require('node:crypto'),f=require('node:fs');const k=c.generateKeyPairSync('ed25519');const seed=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),pub=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),seed,Buffer.from('812100','hex'),pub]),{mode:0o600});f.writeFileSync(process.argv[2],pub,{mode:0o600});",str(work/'signing.der'),str(work/'trusted.pub')],check=True)
 with socket.socket()as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
 origin=f'http://127.0.0.1:{port}';key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
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
 def stop():
  nonlocal child,log
  if child is not None:
   if child.poll()is None:child.send_signal(signal.SIGTERM)
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   child=None
  if log:log.close();log=None
 def check(phase):subprocess.run(['/opt/homebrew/bin/bun','run','scripts/rust/check-native-portable-backup.ts'],cwd=REPO,env={**os.environ,'RM_PORTABLE_BASE':origin,'RM_PORTABLE_WORK':str(work),'RM_PORTABLE_PHASE':phase,'RM_PORTABLE_WEB_DOWNLOAD':'1'if args.with_web_download else'0'},check=True)
 def refuse(argv,label):
  r=subprocess.run([str(binary)]+argv,cwd=REPO,capture_output=True,text=True,timeout=30)
  if r.returncode==0:raise RuntimeError(label+' unexpectedly succeeded')
  return{'check':label,'exit':r.returncode}
 evidence={'profile':'local-user-macos','noOfflineIdentitySeed':True,'globalSingleUseClaim':False,'webDownload':args.with_web_download,'checks':[]}
 try:
  write();start(['--create']);check('source');stop()
  archive=work/'portable.native-backup.json';trusted=work/'trusted.pub'
  # The source is physically gone before either verify or restore starts.
  shutil.rmtree(work/'source');(work/'signing.der').unlink();evidence['sourceUnavailableBeforeVerifyAndRestore']=True
  verify=['--verify-backup',str(archive),'--trusted-backup-key',str(trusted)]
  subprocess.run([str(binary)]+verify,cwd=REPO,check=True)
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
  state=json.loads((work/'private-state.json').read_text());evidence.update({'normalVerificationAndMfa':True,'restart':True,'oldKeyRefusedWithoutEffect':True,'freshKeyNewIntentPosted':True,'download':state.get('download'),'companyUuids':{s:v['uuid']for s,v in state['snapshot'].items()},'originalSha256':state['documentHash'],'invoicePdfSha256':state['pdfHash'],'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'webIndexSha256':hashlib.sha256((web/'index.html').read_bytes()).hexdigest()})
  out=Path('/tmp/rentemester-rust-reports/native-portable-backup.json');out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(evidence,indent=2));out.chmod(0o600)
  print('PASS: J05 live portable backup, independent trust, fresh restore, normal owner/MFA, originals and receipt boundary/restart; '+str(out))
 finally:
  stop()
  if work.parent==parent and work.name.startswith('Rentemester-Synthetic-Portable-'):shutil.rmtree(work)
if __name__=='__main__':main()
