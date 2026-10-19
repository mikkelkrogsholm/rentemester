#!/usr/bin/env python3
"""Disposable current-company SDK/CLI/MCP setup via ordinary owner verification and MFA.

Requires explicit built artifacts. No offline identity/access edits or composed fixture mutation.
"""
import argparse, base64, hashlib, json, os, secrets, shutil, signal, socket, subprocess, tempfile, time, urllib.request
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX="Rentemester-Synthetic-Company-Setup-"
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for flag in ["server-binary","cli-binary","mcp-binary","web-dir"]:p.add_argument("--"+flag,type=Path,required=True)
 p.add_argument("--evidence",type=Path,help="New, nonexistent JSON evidence file; default unique path under /tmp/rentemester-rust-reports")
 group=p.add_mutually_exclusive_group()
 group.add_argument("--saft-privacy-clients",action="store_true",help="Also qualify isolated GDPR overlay through actual SAF-T downloads and pinned clients")
 group.add_argument("--company-creation-clients",action="store_true",help="Also qualify new company creation through CLI/MCP; requires the multi-company server and matching clients")
 args=p.parse_args()
 if args.saft_privacy_clients and args.evidence is None:raise RuntimeError("SAF-T privacy requires an explicit new evidence path")
 if args.evidence is not None and os.path.lexists(args.evidence):raise RuntimeError("Evidence output must not already exist")
 if os.uname().sysname!="Darwin" or os.getuid()==0 or os.getuid()!=os.geteuid():raise RuntimeError("Ordinary macOS user required")
 os.umask(0o077)
 artifacts={name:getattr(args,name.replace("-","_")).resolve(strict=True) for name in ["server-binary","cli-binary","mcp-binary","web-dir"]}
 if any(not artifacts[n].is_file() for n in ["server-binary","cli-binary","mcp-binary"]) or not (artifacts["web-dir"]/"index.html").is_file():raise RuntimeError("Explicit native binaries and built web directory required")
 parent=(Path.home()/"Library/Application Support").resolve(strict=True)
 work=Path(tempfile.mkdtemp(prefix=PREFIX,dir=parent));work.chmod(0o700)
 identity=work.stat();child=None;log=None
 def owned():
  s=work.lstat()
  return work.parent==parent and work.name.startswith(PREFIX) and not work.is_symlink() and s.st_uid==os.getuid() and (s.st_dev,s.st_ino)==(identity.st_dev,identity.st_ino)
 def stop():
  nonlocal child,log
  if child is not None:
   if child.poll() is None:child.send_signal(signal.SIGTERM)
   try:child.wait(timeout=10)
   except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
   child=None
  if log:log.close();log=None
 try:
  pinned={}
  for source,name in [("server-binary","rentemester-server"),("cli-binary","rentemester-rust"),("mcp-binary","rentemester-mcp")]:
   dest=work/name
   with dest.open("xb") as f:f.write(artifacts[source].read_bytes())
   dest.chmod(0o700);pinned[source]=dest
  web=work/"web";shutil.copytree(artifacts["web-dir"],web)
  with socket.socket() as sock:sock.bind(("127.0.0.1",0));port=sock.getsockname()[1]
  origin=f"http://127.0.0.1:{port}";root=work/"workspace";config_path=work/"server.json"
  key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip("=")
  config={"runtimeProfile":"local-user-macos","root":str(root),"brokerUid":os.getuid(),"workspace":"synthetic-native-company-setup","companySlug":"synthetic","companyName":"Synthetic Client Setup Company","cookieKey":key(),"mfaKey":key(),"operatorActors":["system:synthetic-installer"],"firstOwner":{"actor":"system:synthetic-installer","email":"owner@example.invalid","name":"Synthetic Owner","password":"synthetic-owner-password-123"},"accounts":[]}
  def write():
   if not owned():raise RuntimeError("Owned fixture identity changed")
   config_path.write_text(json.dumps(config));config_path.chmod(0o600)
  def start(create=False):
   nonlocal child,log
   if child is not None or not owned():raise RuntimeError("Fresh owned child start required")
   log=open(work/"server.log","ab");child=subprocess.Popen([str(pinned["server-binary"]),"--config",str(config_path),"--web-dir",str(web),"--listen",f"127.0.0.1:{port}"]+(["--create"] if create else []),cwd=REPO,stdout=log,stderr=log)
   for _ in range(300):
    if child.poll() is not None:raise RuntimeError("Owned server exited before readiness")
    try:
     with urllib.request.urlopen(origin+"/api/health",timeout=.2) as r:
      if json.load(r).get("workspaceReady"):return
    except Exception:pass
    time.sleep(.1)
   raise RuntimeError("Owned server readiness timed out")
  write();start(True)
  env={**os.environ,"RM_SETUP_BASE":origin,"RM_SETUP_WORK":str(work),"RM_NATIVE_CLI_BIN":str(pinned["cli-binary"]),"RM_NATIVE_MCP_BIN":str(pinned["mcp-binary"])}
  for flag in ["RM_NATIVE_COMPANY_CREATION","RM_NATIVE_SAFT_PRIVACY"]:env.pop(flag,None)
  if args.company_creation_clients:env["RM_NATIVE_COMPANY_CREATION"]="1"
  if args.saft_privacy_clients:env["RM_NATIVE_SAFT_PRIVACY"]="1"
  command=["/opt/homebrew/bin/bun","run","scripts/rust/check-native-company-setup-clients.ts"]
  subprocess.run(command,cwd=REPO,env=env,check=True)
  stop();del config["firstOwner"];write();start()
  subprocess.run(command+["--reopen"],cwd=REPO,env=env,check=True);stop()
  data=json.loads((work/"journey.json").read_text())
  if args.company_creation_clients:data["companyCreationClients"]=json.loads((work/"company-creation-journey.json").read_text())
  if args.saft_privacy_clients:
   data["saftPrivacy"]=json.loads((work/"saft-privacy.json").read_text())
   data["saftPrivacyHelperSha256"]=hashlib.sha256((REPO/"scripts/rust/check-native-saft-privacy.ts").read_bytes()).hexdigest()
  data.update({"artifacts":{name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in pinned.items()},"webIndexSha256":hashlib.sha256((web/"index.html").read_bytes()).hexdigest(),"helperSha256":hashlib.sha256((REPO/"scripts/rust/check-native-company-setup-clients.ts").read_bytes()).hexdigest(),"sourceRevision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=REPO,text=True).strip(),"normalVerificationAndMfa":True,"restartFreshPasswordAndMfa":True,"noOfflineIdentityFixture":True,"inactiveHttpFixtureAvailable":False,"profile":"local-user-macos"})
  directory=Path("/tmp/rentemester-rust-reports");directory.mkdir(mode=0o700,exist_ok=True)
  output=args.evidence or directory/("native-company-setup-clients-"+work.name.removeprefix(PREFIX)+".json")
  # Never overwrite an existing evidence path, including a symlink or another run's file.
  with output.open("x") as f:json.dump(data,f,indent=2)
  output.chmod(0o600);print("PASS: isolated current-company setup, normal MFA, explicit roles and restart; "+str(output))
 finally:
  stop()
  if owned():shutil.rmtree(work)
if __name__=="__main__":main()
