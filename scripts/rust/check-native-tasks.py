#!/usr/bin/env python3
"""One disposable macOS task journey on the real shared Rust server; no desktop interaction."""
import argparse,base64,json,os,queue,secrets,shutil,socket,subprocess,tempfile,threading,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',required=True);p.add_argument('--routine',action='store_true',help='Run only the routine journey');p.add_argument('--reminders',action='store_true',help='Run only live authorized reminders');p.add_argument('--bank-order',action='store_true',help='Run the native bank source-order bookkeeping journey');p.add_argument('--knowledge',action='store_true',help='Run only protected task-linked knowledge');p.add_argument('--sources',action='store_true',help='Run only the authoritative task sources journey');p.add_argument('--boards',action='store_true',help='Run only the custom board journey');p.add_argument('--binary-dir',help='Existing native binary directory');p.add_argument('--web-dir',help='Existing read-only React build');p.add_argument('--resume',action='store_true',help='Resume the retained pre-owner fixture after a diagnosed setup error');p.add_argument('--finish-clients',action='store_true',help='Continue accepted UI state at the client boundary only');a=p.parse_args();out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True);os.chmod(out,0o700)
 binary_dir=Path(a.binary_dir).resolve() if a.binary_dir else Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'debug'
 binary=binary_dir/'rentemester-server'
 web_dir=Path(a.web_dir).resolve() if a.web_dir else Path(os.environ.get('RENTEMESTER_WEB_DIR',str(ROOT/'app/dist')))
 base=Path(json.loads((out/'retained-fixture.json').read_text())['config']).parent if (a.resume or a.finish_clients) else Path(tempfile.mkdtemp(prefix='Rentemester-Synthetic-Tasks-',dir=Path.home()/'Library/Application Support'));os.chmod(base,0o700)
 if base.parent!=Path.home()/'Library/Application Support' or not base.name.startswith('Rentemester-Synthetic-Tasks-'):raise RuntimeError('Refuse non-synthetic resume')
 server=None;accepted=False
 with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
 if a.resume or a.finish_clients:port=int(json.loads((out/'retained-fixture.json').read_text())['origin'].rsplit(':',1)[1])
 origin=f'http://127.0.0.1:{port}';workspace=base/'workspace';config=base/'server.json'
 key=lambda:base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
 config_value=json.loads(config.read_text()) if a.resume or a.finish_clients else dict(root=str(workspace),brokerUid=os.getuid(),runtimeProfile='local-user-macos',workspace='synthetic-tasks',companySlug='synthetic',companyName='Synthetic tasks',cookieKey=key(),mfaKey=key(),accounts=[])
 config_value.update(setupActor='system:synthetic-installer',operatorActors=['system:synthetic-installer']);config.write_text(json.dumps(config_value));os.chmod(config,0o600)
 def start(create,worker=False):
  cmd=['/bin/sh','-c','umask 077; exec "$@"','task-acceptance',str(binary),'--listen',f'127.0.0.1:{port}','--config',str(config),'--web-dir',str(web_dir)]
  if create:cmd+=['--create']
  if worker:cmd+=['--task-worker','--task-service-key-stdin']
  proc=subprocess.Popen(cmd,cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,stdin=subprocess.PIPE if worker else subprocess.DEVNULL);q=queue.Queue()
  if worker:
   proc.stdin.write((out/'transient-worker.key').read_text());proc.stdin.close()
  def reader():
   for line in proc.stdout:q.put(line)
   q.put(None)
  threading.Thread(target=reader,daemon=True).start();setup=None;lines=[]
  try:
   while True:
    line=q.get(timeout=60)
    if line is None:raise RuntimeError('Synthetic server exited before readiness: '+''.join(lines))
    if line.startswith('Workspace setup: '):setup=line.strip().removeprefix('Workspace setup: ')
    else:lines.append(line)
    if 'Native workspace connected: true' in line:break
  except BaseException:proc.terminate();proc.wait(timeout=15);raise
  (out/('reopen-server.log' if not create else 'server.log')).write_text(''.join(lines));return proc,setup
 def stop(proc):
  proc.terminate();code=proc.wait(timeout=30)
  if code!=0:raise RuntimeError(f'Synthetic server failed drain: {code}')
 try:
  server,setup=start(not (a.resume or a.finish_clients),worker=a.reminders and (a.finish_clients or a.resume))
  env={**os.environ,'RM_TASK_ORIGIN':origin,'RM_TASK_SETUP':setup or '', 'RM_TASK_WORKSPACE':str(workspace),'RM_TASK_OUTPUT':str(out),'RM_TASK_PHASE':'read' if a.knowledge and a.finish_clients else 'reopen' if a.reminders and a.resume else ('worker' if a.reminders else 'clients') if a.finish_clients else 'first','RM_TASK_MODE':'routine' if a.routine else 'manual','RM_TASK_BINARY_DIR':str(binary_dir)}
  subprocess.run(['bun','run','scripts/rust/check-native-bank-order.ts' if a.bank_order else 'scripts/rust/check-native-task-knowledge.ts' if a.knowledge else 'scripts/rust/check-native-task-reminders.ts' if a.reminders else 'scripts/rust/check-native-task-sources.ts' if a.sources else 'scripts/rust/check-native-task-boards.ts' if a.boards else 'scripts/rust/check-native-tasks.ts'],cwd=ROOT,env=env,check=True)
  stop(server);server=None
  if a.knowledge and not a.finish_clients:
   subprocess.run(['python3','scripts/rust/cargo-guard.py','cargo','test','-p','rentemester-storage','--lib','knowledge_pages::tests::provision_owned_synthetic_knowledge_fixture','--','--ignored','--nocapture'],cwd=ROOT,env=env,check=True)
   server,_=start(False);env['RM_TASK_PHASE']='read';env.pop('RM_TASK_SETUP',None)
   subprocess.run(['bun','run','scripts/rust/check-native-task-knowledge.ts'],cwd=ROOT,env=env,check=True)
   stop(server);server=None
  if a.reminders and not (a.finish_clients or a.resume):
   server,_=start(False,worker=True);env['RM_TASK_PHASE']='worker';env.pop('RM_TASK_SETUP',None)
   subprocess.run(['bun','run','scripts/rust/check-native-task-reminders.ts'],cwd=ROOT,env=env,check=True)
   stop(server);server=None
  if not (a.reminders and a.resume):
   server,_=start(False,worker=a.reminders);env['RM_TASK_PHASE']='reopen';env.pop('RM_TASK_SETUP',None)
   subprocess.run(['bun','run','scripts/rust/check-native-bank-order.ts' if a.bank_order else 'scripts/rust/check-native-task-knowledge.ts' if a.knowledge else 'scripts/rust/check-native-task-reminders.ts' if a.reminders else 'scripts/rust/check-native-task-sources.ts' if a.sources else 'scripts/rust/check-native-task-boards.ts' if a.boards else 'scripts/rust/check-native-tasks.ts'],cwd=ROOT,env=env,check=True)
   stop(server);server=None
  if a.reminders:(out/'transient-worker.key').unlink()
  if a.knowledge:(out/'transient-reader.key').unlink()
  accepted=True
  (out/'runtime.json').write_text(json.dumps(dict(origin=origin,profile='local-user-macos',synthetic=True,stopped=True,workspaceRemoved=True),indent=2)+'\n')
 finally:
  if server is not None:server.terminate();server.wait(timeout=30)
  if accepted:shutil.rmtree(base)
  else:(out/"retained-fixture.json").write_text(json.dumps(dict(root=str(workspace),config=str(config),origin=origin,stopped=True)))
if __name__=='__main__':main()
