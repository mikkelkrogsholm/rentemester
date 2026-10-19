#!/usr/bin/env python3
"""Disposable ordinary native invitation journey, without offline identity or access fixtures."""
import argparse,base64,hashlib,json,os,secrets,shutil,signal,socket,subprocess,time,urllib.request,uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--server-binary',type=Path,required=True);p.add_argument('--web-dir',type=Path,required=True);p.add_argument('--with-ownership',action='store_true');p.add_argument('--cli-binary',type=Path);p.add_argument('--mcp-binary',type=Path);args=p.parse_args()
 if args.with_ownership and (not args.cli_binary or not args.mcp_binary):p.error('--with-ownership requires explicit --cli-binary and --mcp-binary')
 if os.uname().sysname!='Darwin' or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError('Ordinary macOS user required')
 os.umask(0o077);parent=(Path.home()/'Library/Application Support').resolve();work=parent/('Rentemester-Synthetic-Invitations-'+uuid.uuid4().hex);work.mkdir(mode=0o700)
 binary=work/'rentemester-server';shutil.copyfile(args.server_binary.resolve(strict=True),binary);binary.chmod(0o700);web=work/'web';shutil.copytree(args.web_dir.resolve(strict=True),web);root=work/'workspace';config_path=work/'server.json';child=None;log=None
 with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
 origin=f'http://127.0.0.1:{port}';key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
 config={'runtimeProfile':'local-user-macos','root':str(root),'brokerUid':os.getuid(),'workspace':'synthetic-native-invitations','companySlug':'synthetic','companyName':'Synthetic Invitation Company','cookieKey':key(),'mfaKey':key(),'operatorActors':['system:synthetic-installer'],'firstOwner':{'actor':'system:synthetic-installer','email':'owner@example.invalid','name':'Synthetic Owner','password':'synthetic-owner-password-123'},'accounts':[]}
 def write():config_path.write_text(json.dumps(config));config_path.chmod(0o600)
 def start(create=False):
  nonlocal child,log
  log=open(work/'server.log','ab');child=subprocess.Popen([str(binary),'--config',str(config_path),'--web-dir',str(web),'--listen',f'127.0.0.1:{port}']+(['--create']if create else []),cwd=REPO,stdout=log,stderr=log)
  for _ in range(300):
   if child.poll() is not None:raise RuntimeError('Owned synthetic server exited before readiness')
   try:
    with urllib.request.urlopen(origin+'/api/health',timeout=.2) as r:
     if json.load(r).get('workspaceReady'):return
   except Exception:pass
   time.sleep(.1)
  raise RuntimeError('Owned synthetic server readiness timed out')
 def stop():
  nonlocal child,log
  if child is not None:
   if child.poll() is None:child.send_signal(signal.SIGTERM)
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   child=None
  if log:log.close();log=None
 try:
  write();start(True)
  env={**os.environ,'RM_INVITATION_BASE':origin,'RM_INVITATION_WORK':str(work)}
  if args.with_ownership:
   env['RM_INVITATION_OWNERSHIP']='1'
   for name,source in [('CLI',args.cli_binary),('MCP',args.mcp_binary)]:
    destination=work/('rentemester-'+name.lower());shutil.copyfile(source.resolve(strict=True),destination);destination.chmod(0o700);env['RM_NATIVE_'+name+'_BIN']=str(destination)
  subprocess.run(['/opt/homebrew/bin/bun','run','scripts/rust/check-native-workspace-invitations.ts'],cwd=REPO,env=env,check=True)
  stop();del config['firstOwner'];write();start()
  subprocess.run(['/opt/homebrew/bin/bun','run','scripts/rust/check-native-workspace-invitations.ts','--reopen'],cwd=REPO,env=env,check=True)
  source_evidence=work/'invitations-evidence.json' if args.with_ownership else Path('/tmp/rentemester-rust-reports/native-workspace-invitations.json');evidence=Path('/tmp/rentemester-rust-reports/native-ownership-integrated37.json') if args.with_ownership else source_evidence;data=json.loads(source_evidence.read_text());data.update({'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'webIndexSha256':hashlib.sha256((web/'index.html').read_bytes()).hexdigest(),'noOfflineIdentityFixture':True,'profile':'local-user-macos'});
  if args.with_ownership:
   data['ownershipBrowser']=json.loads(Path('/tmp/rentemester-rust-reports/native-ownership-browser.json').read_text());data['ownershipClients']=json.loads(Path('/tmp/rentemester-rust-reports/native-ownership-clients.json').read_text());data['clientArtifacts']={name:hashlib.sha256(Path(env['RM_NATIVE_'+name+'_BIN']).read_bytes()).hexdigest() for name in ['CLI','MCP']}
  evidence.write_text(json.dumps(data,indent=2));evidence.chmod(0o600)
  print('PASS: separate native invitation users, normal verification/MFA, access changes and restart; '+str(evidence))
 finally:
  stop()
  if work.parent==parent and work.name.startswith('Rentemester-Synthetic-Invitations-'):shutil.rmtree(work)
if __name__=='__main__':main()
