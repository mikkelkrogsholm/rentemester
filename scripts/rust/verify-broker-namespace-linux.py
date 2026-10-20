#!/usr/bin/env python3
"""Isolated Linux candidate qualification; Docker and the pinned Rust image required.
Copies source only. Does not mount or open real workspaces. No deployment gate.
"""
import argparse, hashlib, json, queue, shutil, subprocess, tempfile, threading, time, uuid
from pathlib import Path
from types import SimpleNamespace
IMAGE="rust@sha256:cf9dd0ec73e75f827fe59123fff9dc65af1a1c8363c3c31ee8d7f8ad0b6a5fb2"
BASE="namespace::linux::tests::"

def run(args, expected=0):
    result=subprocess.run(args,capture_output=True,text=True,timeout=180)
    assert result.returncode==expected,(args,result.returncode,result.stdout,result.stderr)
    return result.stdout

def helpers(container,exe):
    def root_case(label):
        root="/qualified/"+label
        run(["docker","exec","-e","RM_BROKER_ROOT="+root,container,exe,"--ignored","--exact",BASE+"fixture","--nocapture"])
        run(["docker","exec",container,"chown","-R","10001:10001",root])
        return root
    def command(root,test="probe",uid=10001,extra=None,interactive=False):
        args=["docker","exec"]+(["-i"]if interactive else[])
        for key,value in {"RM_BROKER_ROOT":root,"RM_BROKER_UID":"10001",**(extra or{})}.items():args += ["-e",key+"="+value]
        return args+[container,"setpriv","--bounding-set=-all","--reuid="+str(uid),"--regid="+str(uid),"--clear-groups","--no-new-privs","sh","-c",'umask 077; exec "$@"',"rm-broker",exe,"--ignored","--exact",BASE+test,"--nocapture"]
    def probe(root,expected="accepted",**kw):
        output=run(command(root,**kw))
        assert "RM_BROKER_RESULT="+expected in output,output
    return SimpleNamespace(C=container,EXE=exe,BASE=BASE,run=run,root_case=root_case,command=command,probe=probe)

def qualify(h,prefix,build_volume):
    P=prefix;checks=[]
    def record(id):checks.append({'id':id});print(id,flush=True)
    def case(id):return h.root_case(P+'-'+id)
    def root(*args):return h.run(['docker','exec',h.C,*args])
    def accepted(path):h.probe(path);record(Path(path).name+'/accepted')
    def refused(path):h.probe(path,'unsafe_broker_namespace');record(Path(path).name+'/refused')
    def helper(path,name,**kw):return h.run(h.command(path,test=name,**kw))
    class Hold:
     def __init__(self,args,marker):
      self.p=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,bufsize=1);self.q=queue.Queue();self.log=[]
      def reader():
       for line in self.p.stdout:self.q.put(line)
       self.q.put(None)
      threading.Thread(target=reader,daemon=True).start();self.until(marker)
     def until(self,marker):
      deadline=time.monotonic()+15
      while True:
       line=self.q.get(timeout=max(.01,deadline-time.monotonic()));self.log.append(line)
       assert line is not None,(marker,self.log)
       if marker in line:return line
     def send(self,s):self.p.stdin.write(s+'\n');self.p.stdin.flush()
     def finish(self,s):
      self.send(s);self.p.stdin.close();assert self.p.wait(timeout=15)==0,self.log

    def contention(path,expected):
     args=h.command(path,test='probe');start=args.index('--exact')+1;args[start]='local::layout::connection_tests::lock_child'
     args[2:2]=['-e','RM_IDENTITY_LOCK_PATH='+path+'/a/data/ledger.sqlite']
     out=h.run(args);assert 'RM_IDENTITY_LOCK='+expected in out,out

    r=case('valid');accepted(r)
    assert 'unsafe_broker_process' in helper(r,'probe',uid=10002);record('wrong-current-uid/refused')
    assert 'unsafe_broker_process' in helper(r,'probe',extra={'RM_BROKER_UID':'0'});record('root-profile/refused')
    args=h.command(r);args[args.index('umask 077; exec "$@"')]='umask 022; exec "$@"'
    assert 'unsafe_broker_process' in h.run(args);record('wrong-umask/refused')
    for id,mutation in [
     ('rootmode',lambda r:root('chmod','0755',r)),
     ('filemode',lambda r:root('chmod','0644',r+'/workspace.json')),
     ('dirowner',lambda r:root('chown','10002:10002',r+'/a/data')),
     ('dirmode',lambda r:root('chmod','0750',r+'/a/data')),
     ('fileowner',lambda r:root('chown','10002:10002',r+'/.rentemester/workspace-control.sqlite')),
     ('symlinkdir',lambda r:(root('mv',r+'/a/data',r+'/saved'),root('ln','-s',r+'/saved',r+'/a/data'))),
     ('missing',lambda r:root('rm',r+'/.rentemester/workspace-control.sqlite')),
     ('hardlink',lambda r:root('ln',r+'/workspace.json',r+'/alias')),
     ('sidecarlink',lambda r:root('ln','-s',r+'/a/data/ledger.sqlite',r+'/a/data/ledger.sqlite-wal')),
     ('sidecaralias',lambda r:root('ln',r+'/.rentemester/workspace-control.sqlite',r+'/a/data/ledger.sqlite-shm')),
     ('sidecarbadmode',lambda r:(root('touch',r+'/a/data/ledger.sqlite-journal'),root('chown','10001:10001',r+'/a/data/ledger.sqlite-journal'),root('chmod','0644',r+'/a/data/ledger.sqlite-journal'))),
     ]:
     r=case(id);mutation(r);refused(r)
    for kind in ['access','default']:
     r=case('acl-'+kind)
     out=h.run(['docker','exec','-e','RM_BROKER_ROOT='+r,'-e','RM_BROKER_ACL_KIND='+kind,h.C,h.EXE,'--ignored','--exact',h.BASE+'acl_fixture','--nocapture'])
     assert 'RM_BROKER_ACL=installed-and-read-back' in out;refused(r);record('actual-ACL/'+kind+'/read-back')
    for mode in ['WAL','DELETE']:
     r=case('held-'+mode)
     hold=Hold(h.command(r,extra={'RM_BROKER_HOLD':'1','RM_BROKER_MODE':mode},interactive=True),'RM_BROKER_RESULT=held')
     try:
      out=helper(r,'client',uid=10002);assert 'RM_BROKER_CLIENT_DENIED=66' in out,out;record(mode+'/66-real-different-UID-denials')
      out=helper(r,'client',extra={'RM_BROKER_CLIENT_POSITIVE':'1'});assert 'RM_BROKER_CLIENT=positive' in out;record(mode+'/sameUID-positive-control')
      h.probe(r,'workspace_busy');record(mode+'/second-broker-lease-busy')
      contention(r,'busy');record(mode+'/sqlite-lock-before')
      hold.send('check');hold.until('RM_BROKER_CHECK=accepted');contention(r,'busy');record(mode+'/locks-after-validation')
      # Alias an observed sidecar name to the active main DB; observe/refuse without opening it.
      alias=r+'/a/data/ledger.sqlite-journal' if mode=='WAL' else r+'/a/data/ledger.sqlite-shm'
      root('ln',r+'/a/data/ledger.sqlite',alias)
      hold.send('check');hold.until('RM_BROKER_CHECK=broker_namespace_changed');contention(r,'busy');record(mode+'/alias-refusal-keeps-lock')
      root('rm',alias);hold.send('check');hold.until('RM_BROKER_CHECK=broker_namespace_changed');record(mode+'/restoration-remains-retired')
      hold.finish('commit');contention(r,'acquired');accepted(r);record(mode+'/explicit-close-releases-lease-and-lock')
     finally:
      if hold.p.poll()is None:hold.p.stdin.close();hold.p.wait(timeout=15)
    # A permissive filesystem control proves client opening is possible when modes are wrong.
    r=case('permissive');root('chmod','0755',r,r+'/a',r+'/a/data');root('chmod','0666',r+'/a/data/ledger.sqlite')
    assert 'RM_BROKER_CLIENT=positive' in helper(r,'client',uid=10002,extra={'RM_BROKER_CLIENT_POSITIVE':'1'});refused(r);record('permissive-client-success-control')
    # Current modes cannot revoke an FD obtained earlier: explicit offline-provisioning gate.
    prior=Hold(h.command(r,test='prior_descriptor',uid=10002,interactive=True),'RM_BROKER_PRIOR=open')
    root('chmod','0700',r,r+'/a',r+'/a/data');root('chmod','0600',r+'/a/data/ledger.sqlite');before=int(root('stat','-c','%s',r+'/a/data/ledger.sqlite').strip())
    prior.send('write');prior.until('RM_BROKER_PRIOR=write-survived-mode-change');prior.p.stdin.close();assert prior.p.wait(timeout=15)==0
    assert int(root('stat','-c','%s',r+'/a/data/ledger.sqlite').strip())>before;record('prior-FD-survives-chmod-negative-control')
    # A separate container intentionally starts with leader NoNewPrivs=0 for thread-local oracle.
    image='rust@sha256:cf9dd0ec73e75f827fe59123fff9dc65af1a1c8363c3c31ee8d7f8ad0b6a5fb2'
    out=h.run(['docker','run','--rm','--name',prefix+'-thread','--user','10001:10001','--cap-drop','ALL','--mount','type=volume,src='+build_volume+',dst=/build,readonly','-e','RM_BROKER_UID=10001',image,'sh','-c','umask 077; exec "$@"','rm-broker',h.EXE,'--ignored','--exact',h.BASE+'thread_profile','--nocapture'])
    assert 'RM_BROKER_THREAD=current-thread-confirmed' in out;record('actual-thread-NNP-differs-from-leader')
    return checks

def cleanup(container,build_volume,data_volume):
    # Only names generated by this invocation; no blanket prune or shared work.
    for name in [container+"-thread",container]:
        current=run(["docker","container","ls","-a","--filter","name="+name,"--format","{{.Names}}"])
        if name in current.splitlines():run(["docker","rm","-f",name])
        assert name not in run(["docker","container","ls","-a","--filter","name="+name,"--format","{{.Names}}"]).splitlines(),"Owned container remains"
    for name in [build_volume,data_volume]:
        current=run(["docker","volume","ls","--filter","name="+name,"--format","{{.Name}}"])
        if name in current.splitlines():run(["docker","volume","rm",name])
        assert name not in run(["docker","volume","ls","--filter","name="+name,"--format","{{.Name}}"]).splitlines(),"Owned volume remains"

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists(): raise SystemExit("Refusing to overwrite an existing qualification report")
    repo=Path(__file__).resolve().parents[2]
    prefix="rm-broker-"+uuid.uuid4().hex[:12]
    build_volume=prefix+"-build"
    data_volume=prefix+"-data"
    with tempfile.TemporaryDirectory(prefix=prefix)as temp:
        source=Path(temp)/"source";source.mkdir()
        for name in ["Cargo.toml","Cargo.lock"]:shutil.copyfile(repo/name,source/name)
        for name in ["crates","vendor"]:shutil.copytree(repo/name,source/name)
        anchors={str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted(source.rglob("*"))if p.is_file()}
        image=json.loads(run(["docker","image","inspect",IMAGE]))[0]
        try:
            run(["docker","run","-d","--name",prefix,"--security-opt","no-new-privileges","--mount","type=volume,src="+data_volume+",dst=/qualified","--mount","type=volume,src="+build_volume+",dst=/build","--mount","type=bind,src="+str(source)+",dst=/work,readonly","--workdir","/work","--env","CARGO_HOME=/build/cargo-home","--env","CARGO_TARGET_DIR=/build/target","--env","RENTEMESTER_CARGO_CACHE=/build/target",IMAGE,"sleep","infinity"])
            build=run(["docker","exec",prefix,"cargo","test","--locked","-p","rentemester-runtime","--no-run","--message-format=json"])
            artifacts=[]
            for line in build.splitlines():
                try:row=json.loads(line)
                except ValueError:continue
                if row.get("executable")and row["target"]["name"]=="rentemester_runtime":artifacts.append(row["executable"])
            assert len(artifacts)==1,artifacts
            h=helpers(prefix,artifacts[0])
            checks=qualify(h,prefix,build_volume)
            assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==digest for p,digest in anchors.items()),"Source changed during qualification"
            report={"ok":True,"checks":len(checks),"observations":checks,"different_uid_operations_denied":132,"image_repository_digest":IMAGE,"image_configuration_digest":image["Id"],"architecture":image["Architecture"],"filesystem":run(["docker","exec",prefix,"stat","-f","-c","%T","/qualified"]).strip(),"source_files_sha256":anchors,"build_log_sha256":hashlib.sha256(build.encode()).hexdigest(),"limits":["New path access only; a prior open descriptor survives chmod","Same UID and root/operator remain trusted/enrolled","No host owner, closeBUSY, canonical domain schema, deployment or macOS profile qualification"]}
        finally:
            cleanup(prefix,build_volume,data_volume)
        report["owned_resources_removed"]=True
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps({"ok":True,"checks":len(checks),"filesystem":report["filesystem"],"cleanup_verified":True}),flush=True)

if __name__=="__main__":main()
