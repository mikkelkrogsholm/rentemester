#!/usr/bin/env python3
"""Disposable macOS two-MFA-context qualification. Never opens an existing workspace."""
import argparse, base64, fcntl, hashlib, json, os, secrets, shutil, signal, socket, subprocess, time, urllib.request, uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
def run(command,**kwargs):return subprocess.run(command,check=True,**kwargs)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--skip-build',action='store_true');p.add_argument('--server-binary',type=Path,help='Explicit built native server; use --skip-build for composed-main verification');p.add_argument('--web-dir',type=Path,help='Explicit built cockpit directory; use --skip-build for composed-main verification');args=p.parse_args()
 revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()
 source_changes=subprocess.check_output(['git','diff','--name-only','--','crates','app/src','scripts/rust'],cwd=REPO,text=True).splitlines()
 if os.uname().sysname!='Darwin' or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError('Ordinary macOS user required')
 os.umask(0o077)
 if not args.skip_build:
  if not args.server_binary:run([str(Path.home()/'.cargo/bin/cargo'),'build','--locked','-p','rentemester-server'],cwd=REPO,env={**os.environ,'CARGO_INCREMENTAL':'0'})
  if not args.web_dir:run(['/opt/homebrew/bin/bun','run','build'],cwd=REPO/'app')
 parent=(Path.home()/'Library/Application Support').resolve();work=parent/('Rentemester-Synthetic-Review-'+uuid.uuid4().hex);work.mkdir(mode=0o700)
 root=work/'workspace';config_path=work/'server.json';binary=(args.server_binary or REPO/'target/debug/rentemester-server').resolve(strict=True);web=(args.web_dir or REPO/'app/dist').resolve(strict=True);child=None;log=None
 with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
 origin=f'http://127.0.0.1:{port}'
 key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
 config={'runtimeProfile':'local-user-macos','root':str(root),'brokerUid':os.getuid(),'workspace':'synthetic-reviewed-bank','companySlug':'synthetic','companyName':'Synthetic Reviewed Bank','cookieKey':key(),'mfaKey':key(),'operatorActors':['system:synthetic-installer'],'firstOwner':{'actor':'system:synthetic-installer','email':'owner@example.invalid','name':'Synthetic Owner','password':'synthetic-password-123'},'accounts':[{'accountNo':'1800','name':'Synthetic bank','type':'asset','normalBalance':'debit'},{'accountNo':'8000','name':'Synthetic equity','type':'equity','normalBalance':'credit'}]}
 def write():config_path.write_text(json.dumps(config));config_path.chmod(0o600)
 def start(create=False):
  nonlocal child,log
  log=open(work/'server.log','ab');command=[str(binary),'--config',str(config_path),'--web-dir',str(web),'--listen',f'127.0.0.1:{port}']+(['--create']if create else [])
  child=subprocess.Popen(command,cwd=REPO,stdout=log,stderr=log)
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
   if child.poll() is None:raise RuntimeError('Owner did not stop; offline fixture forbidden')
   child=None
  if log:log.close();log=None
 try:
  write();start(True);stop() # wait exact Popen child before touching SQLite
  fd=os.open(root,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
  try:
   fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB) # same physical root-directory lease as native owner
   run(['/opt/homebrew/bin/bun','run','scripts/rust/native-reviewed-fixture.ts','--seed'],cwd=REPO,pass_fds=(fd,),env={**os.environ,'RM_REVIEW_CONFIG':str(config_path),'RM_REVIEW_OFFLINE_FD':str(fd)})
  finally:os.close(fd)
  del config['firstOwner'];write();start()
  verification_files=list((root/'.rentemester').glob('verification-*.txt'))
  if len(verification_files)!=1:raise RuntimeError('Expected exactly one private synthetic verification link')
  verification=verification_files[0].read_text().strip()
  run(['/opt/homebrew/bin/bun','run','scripts/rust/native-reviewed-fixture.ts'],cwd=REPO,env={**os.environ,'RM_REVIEW_BASE':origin,'RM_REVIEW_VERIFY':verification})
  stop();start()
  run(['/opt/homebrew/bin/bun','run','scripts/rust/native-reviewed-fixture.ts','--reopen'],cwd=REPO,env={**os.environ,'RM_REVIEW_BASE':origin})
  report=Path('/tmp/rentemester-rust-reports/native-reviewed-bookkeeping.json');data=json.loads(report.read_text());data.update({'revision':revision,'sourceChangesAtStart':source_changes,'binaryPath':str(binary),'webDirectory':str(web),'suppliedArtifacts':bool(args.server_binary or args.web_dir),'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'webIndexSha256':hashlib.sha256((web/'index.html').read_bytes()).hexdigest(),'launcherSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'browserFixtureSha256':hashlib.sha256((REPO/'scripts/rust/native-reviewed-fixture.ts').read_bytes()).hexdigest(),'profile':'local-user-macos','stoppedBeforeOfflineSeed':True,'rootDirectoryLeaseHeldDuringSeed':True,'restartVerified':True});report.write_text(json.dumps(data,indent=2));report.chmod(0o600)
  print('PASS: isolated native two-user password/MFA bank review journey and restart; evidence '+str(report))
 finally:
  stop()
  report=Path('/tmp/rentemester-rust-reports/native-reviewed-bookkeeping.json')
  if report.exists():
   data=json.loads(report.read_text());data.pop('ownerRecovery',None);data.pop('reviewerRecovery',None);report.write_text(json.dumps(data,indent=2));report.chmod(0o600)
  # Only this newly created private synthetic directory is ever removed.
  if work.parent==parent and work.name.startswith('Rentemester-Synthetic-Review-'):shutil.rmtree(work)
if __name__=='__main__':main()
