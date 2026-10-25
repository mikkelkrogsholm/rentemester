#!/usr/bin/env python3
"""Owned HTTPS auth-mail/web proof, without real delivery or offline identity seeds.

Pins supplied artifacts. All addresses, mail recipients and certificates are private
synthetic fixtures. Run only after integration; this runner never builds software.
"""
import argparse
import base64
import hashlib
import http.client
import http.server
import json
import os
import re
import secrets
import selectors
import shutil
import signal
import socket
import ssl
import subprocess
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-HostedAuth-'
BROWSER = r'''
import {createHmac} from 'node:crypto';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_AUTH_PLAYWRIGHT);
const base=process.env.RM_AUTH_BASE,work=process.env.RM_AUTH_WORK;
if(!/^https:\/\/localhost:\d+$/.test(base))throw Error('owned canonical HTTPS origin required');
const owner={email:'owner@example.invalid',password:'synthetic-owner-password-123',codes:[]};
const member={email:'invited@example.invalid',password:'synthetic-invited-password-456',codes:[]};
const replacement='synthetic-replacement-password-789';
let browser,ownerPage,memberPage,step='boot',detail=null,baseline,ownerId,memberId,invitationId,lastLogin=0;const trace=[];let diagnosticPage;
const need=(v,s)=>{if(!v){detail=s;throw Error(s);}};
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'MFA setup key syntax');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function cooldown(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();}
async function fetchJson(page,path,data){return page.evaluate(async({path,data})=>{const r=await fetch(path,data===undefined?{}:{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(data)});let body;try{body=await r.json();}catch{body=null;}return{status:r.status,body};},{path,data});}
async function get(page,path){const r=await fetchJson(page,path);need(r.status===200,`canonical ${path} HTTP${r.status}`);return r.body;}
function mails(kind,to){return readdirSync(join(work,'mail')).filter(n=>n.endsWith('.json')).map(n=>JSON.parse(readFileSync(join(work,'mail',n),'utf8'))).filter(m=>m.kind===kind&&m.to===to);}
async function mail(kind,to,previous=0){for(let i=0;i<150;i++){const found=mails(kind,to);if(found.length===previous+1){const m=found[previous];need(new URL(m.actionUrl).origin===base,'mail action canonical HTTPS');return m;}need(found.length<=previous+1,'unexpected additional mail token');await new Promise(r=>setTimeout(r,100));}throw Error('bounded private gateway receipt wait expired');}
async function login(page,user,password=user.password){await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(user.email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('button',{name:'Brug recovery code',exact:true}).click();need(user.codes.length,'unused existing MFA recovery code');await page.getByLabel('Recovery code',{exact:true}).fill(user.codes.shift());await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();const me=await get(page,'/api/me');need(me.user.email===user.email&&me.user.twoFactorEnabled,'normal current verified MFA identity');const cookies=await page.context().cookies(base);need(cookies.some(c=>c.secure&&c.httpOnly),'HTTPS Secure HttpOnly session cookie');return me;}
async function enroll(page,user){const label=user===owner?'owner':'invited';step=label+'-enroll-login';await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(user.email);await page.getByLabel('Adgangskode',{exact:true}).fill(user.password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step=label+'-enroll-heading';await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse',exact:true}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(user.password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();step=label+'-enroll-setup-key';const key=await page.getByLabel('Opsætningsnøgle',{exact:true}).inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(key));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();step=label+'-enroll-recovery-inventory';const codes=page.getByRole('region',{name:'Recovery codes'}).locator('li');await codes.first().waitFor();user.codes=await codes.allTextContents();need(user.codes.length>=4,'existing MFA recovery inventory');await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();step=label+'-enroll-acknowledgement';await page.context().clearCookies();return login(page,user);}
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});return page;}
async function setup(){
 browser=await chromium.launch({headless:true,args:['--ignore-certificate-errors-spki-list='+process.env.RM_AUTH_SPKI]});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 step='web-forgot-password-timeout-after-acceptance';const resetPage=await newPage();step='forgot-new-login-page';await resetPage.goto(base);step='forgot-open-link';await resetPage.getByRole('link',{name:'Glemt adgangskode?',exact:true}).click();await resetPage.getByRole('heading',{name:'Nulstil adgangskode',exact:true}).waitFor();step='forgot-fill-email';await resetPage.getByLabel('E-mail',{exact:true}).fill(member.email);const requested=resetPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/auth/request-password-reset');requested.catch(()=>{});step='forgot-submit';await resetPage.getByRole('button',{name:'Send reset-link',exact:true}).click();step='forgot-generic-status';await resetPage.getByRole('status').waitFor();need(trace.some(r=>r.kind==='request'&&r.path==='/api/auth/request-password-reset'),'reset client issued actual request');step='forgot-response';const rr=await requested;need(rr.status()===200,'generic reset request response');need((await get(memberPage,'/api/me')).user.id===memberId,'request alone cannot revoke current session');const reset=await mail('password-reset',member.email);
 step='web-reset-through-delivered-link';await resetPage.goto(reset.actionUrl);await resetPage.getByLabel('Ny adgangskode',{exact:true}).waitFor();need(!new URL(resetPage.url()).search&&!new URL(resetPage.url()).hash,'reset token removed from rendered address');await resetPage.getByLabel('Ny adgangskode',{exact:true}).fill(replacement);const completed=resetPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/auth/reset-password'&&r.request().method()==='POST');await resetPage.getByRole('button',{name:'Opdater adgangskode',exact:true}).click();const receipt=await completed;need(receipt.status()===200&&(await receipt.json()).status===true,'password reset canonical receipt');const body=receipt.request().postDataJSON();need(typeof body.token==='string'&&body.token.length>0,'actual submitted one-time reset bearer');await resetPage.getByText('Din adgangskode er opdateret. Log ind igen.',{exact:true}).waitFor();need(!(await resetPage.context().cookies(base)).some(c=>c.name.includes('session')),'reset did not mint authenticated session');
 step='old-session-and-token-refusal';need((await fetchJson(memberPage,'/api/me')).status===401,'old password session revoked');const replay=await fetchJson(resetPage,'/api/auth/reset-password',{token:body.token,newPassword:'synthetic-replayed-password-000'});need(replay.status===400&&replay.body?.code==='INVALID_TOKEN','consumed reset token refused');
 step='old-password-web-refusal';const denied=await newPage();await cooldown();await denied.goto(base);await denied.getByLabel('E-mail',{exact:true}).fill(member.email);await denied.getByLabel('Adgangskode',{exact:true}).fill(member.password);const bad=denied.waitForResponse(r=>new URL(r.url()).pathname==='/api/auth/sign-in/email');await denied.getByRole('button',{name:'Log ind',exact:true}).click();need((await bad).status()===401,'old password refused by ordinary web login');need((await fetchJson(denied,'/api/me')).status===401,'failed old password no session');
 step='new-password-with-pre-existing-mfa';member.password=replacement;await memberPage.context().clearCookies();const fresh=await login(memberPage,member);need(fresh.user.id===memberId,'password reset preserves user identity');need(isDeepStrictEqual(await get(memberPage,'/api/companies/synthetic/trial-balance'),baseline),'reset preserves ledger and UUID');need((await get(ownerPage,'/api/me')).user.id===ownerId,'unrelated owner session remains valid');
 return {ownerId,memberId,invitationId,ledgerUuid:baseline.trialBalance.ledgerUuid,normalEmailVerification:true,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true,secureHttpOnlyCookie:true,resetRequestDidNotRevokeSession:true,timeoutAcceptedResetLinkUsable:true,oldSessionRefused:true,oldPasswordRefused:true,replayedResetTokenRefused:true,newPasswordWithExistingMfa:true,noResetSession:true};
}
async function reopen(){step='restart-existing-identities-and-mfa';await ownerPage.context().clearCookies();await memberPage.context().clearCookies();const a=await login(ownerPage,owner),b=await login(memberPage,member);need(a.user.id===ownerId&&b.user.id===memberId,'same identities after restart');need(isDeepStrictEqual(await get(memberPage,'/api/companies/synthetic/trial-balance'),baseline),'canonical ledger UUID/full balance after restart');const invitations=await get(ownerPage,'/api/workspace/invitations');need(invitations.invitations.some(i=>i.invitationId===invitationId&&i.status==='accepted'),'accepted invitation history after restart');return{restartVerified:true,existingMfaAfterRestart:true,canonicalReadbackUnchanged:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const command=JSON.parse(line);const value=command==='setup'?await setup():command==='reopen'?await reopen():null;if(command==='finish'){await browser?.close();console.log(JSON.stringify({ok:true,value:{}}));break;}need(value,'known controller phase');console.log(JSON.stringify({ok:true,value}));}catch(error){let pageState=null;try{pageState=await diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),emailInputs:document.querySelectorAll('input[type=email]').length,emailValid:[...document.querySelectorAll('input[type=email]')].every(e=>e.checkValidity()),emailLengths:[...document.querySelectorAll('input[type=email]')].map(e=>e.value.length),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length}));}catch{}console.log(JSON.stringify({ok:false,step,detail,errorType:error?.name,operation:error?.message?.match(/^(?:locator|page)\.[A-Za-z]+/)?.[0],trace,pageState}));await browser?.close();process.exitCode=1;break;}}
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--server-binary', type=Path)
    mode.add_argument('--image')
    parser.add_argument('--web-dir', type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    args = parser.parse_args()
    if bool(args.server_binary) != bool(args.web_dir):
        parser.error('--server-binary requires --web-dir; --image excludes --web-dir')
    if args.evidence.exists():
        parser.error('Evidence path must be new')
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error('Ordinary macOS fixture owner required')
    os.umask(0o077)
    parent = (Path.home() / 'Library/Application Support').resolve()
    work = parent / (PREFIX + uuid.uuid4().hex)
    work.mkdir(mode=0o700)
    identity = work.stat()
    binary = work / 'rentemester-server'
    web = work / 'web'
    if not args.image:
        shutil.copyfile(args.server_binary.resolve(strict=True), binary)
        binary.chmod(0o700)
        shutil.copytree(args.web_dir.resolve(strict=True), web)
    mail_dir = work / 'mail'
    mail_dir.mkdir(mode=0o700)
    cert, private, ca = work / 'tls.pem', work / 'tls.key', work / 'ca.pem'
    child = controller = None
    log = None
    servers = []
    gateway_rows = {}
    gateway_requests = []
    gateway_lock = threading.Lock()
    errors = []
    bearer = secrets.token_urlsafe(32)
    backend_port, proxy_port, mail_port = free_port(), free_port(), free_port()
    if len({backend_port, proxy_port, mail_port}) != 3:
        raise RuntimeError('Distinct owned ports required; no automatic restart')
    origin = f'https://localhost:{proxy_port}'
    authority = f'localhost:{proxy_port}'
    report = {'ok': False, 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentityFixture': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False}

    container = 'rm-hosted-auth-' + uuid.uuid4().hex[:12]
    volumes = [container + '-state', container + '-config']
    created_volumes = []
    container_created = False
    image_id = None

    def docker(*argv, **kwargs):
        return subprocess.run(['docker', *argv], check=True, capture_output=True,
                              timeout=kwargs.pop('timeout', 75), **kwargs).stdout

    class QuietServer(http.server.ThreadingHTTPServer):
        daemon_threads = True

    class QuietHandler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

    class Gateway(QuietHandler):
        def do_POST(self):
            length = int(self.headers.get('content-length', '0'))
            if self.path != '/mail' or not 0 < length <= 32768 or self.headers.get('authorization') != 'Bearer ' + bearer:
                self.send_error(403)
                return
            try:
                body = json.loads(self.rfile.read(length))
                event = self.headers.get('x-rentemester-auth-event')
                key = self.headers.get('idempotency-key', '')
                if not re.fullmatch('[0-9a-f]{64}', key) or body.get('version') != 'rentemester-auth-email-v1' or body.get('kind') != event or event not in ('verification', 'workspace-invitation', 'password-reset') or body.get('to') not in ('owner@example.invalid', 'invited@example.invalid') or body.get('from') != 'auth@example.invalid' or not body.get('actionUrl', '').startswith(origin + '/'):
                    raise ValueError('Invalid fixture gateway envelope')
                with gateway_lock:
                    previous = gateway_rows.get(key)
                    if previous is not None and previous != body:
                        raise ValueError('Idempotency key changed payload')
                    first_reset = event == 'password-reset' and not any(r['kind'] == event for r in gateway_requests)
                    gateway_requests.append({'kind': event, 'idempotencyHash': hashlib.sha256(key.encode()).hexdigest(), 'timeoutAfterAcceptance': first_reset})
                    if previous is None:
                        gateway_rows[key] = body
                        temporary = mail_dir / (str(len(gateway_rows)) + '.pending')
                        with temporary.open('x') as output:
                            json.dump(body, output)
                        temporary.rename(temporary.with_suffix('.json'))
                if first_reset:
                    time.sleep(9.5)  # Native eight-second budget expires after durable fixture acceptance.
                self.send_response(202)
                self.send_header('content-length', '0')
                self.end_headers()
            except (BrokenPipeError, ConnectionResetError):
                pass
            except Exception:
                errors.append('gateway-envelope-or-idempotency-refusal')
                self.send_error(400)

    class Proxy(QuietHandler):
        def forward(self):
            if self.headers.get('host') != authority or not self.path.startswith('/') or self.path.startswith('//'):
                self.send_error(403)
                return
            length = int(self.headers.get('content-length', '0'))
            if not 0 <= length <= 2 * 1024 * 1024:
                self.send_error(413)
                return
            connection = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=45)
            try:
                headers = {k: v for k, v in self.headers.items() if k.lower() not in ('host', 'connection', 'transfer-encoding', 'content-length')}
                headers['host'] = authority
                payload = self.rfile.read(length) if length else None
                connection.request(self.command, self.path, payload, headers)
                response = connection.getresponse()
                body = response.read(16 * 1024 * 1024 + 1)
                if len(body) > 16 * 1024 * 1024:
                    raise ValueError('Bounded proxy response exceeded')
                self.send_response(response.status)
                for key, value in response.getheaders():
                    if key.lower() not in ('connection', 'transfer-encoding', 'content-length'):
                        self.send_header(key, value)
                self.send_header('content-length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass
            except Exception:
                errors.append('proxy-upstream-failure')
                self.send_error(502)
            finally:
                connection.close()
        do_GET = do_POST = do_HEAD = forward

    def write_config(config):
        encoded = json.dumps(config).encode()
        (work / 'server.json').write_bytes(encoded)
        if args.image:
            # Operator-only initialization, never a product startup privilege.
            docker('run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                   '--mount', f'type=volume,src={volumes[0]},dst=/state',
                   '--mount', f'type=volume,src={volumes[1]},dst=/config',
                   '--entrypoint', 'sh', image_id, '-c',
                   'umask 077; cat > /config/server.json; '
                   'chown 10001:10001 /state /config /config/server.json; '
                   'chmod 0700 /state /config; chmod 0600 /config/server.json', input=encoded)

    def start(create=False):
        nonlocal child, log, container_created
        if args.image:
            docker('run', '--detach', '--name', container, '--user', '10001:10001',
                   '--read-only', '--memory', '1g', '--pids-limit', '128',
                   '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
                   '--tmpfs', '/tmp:rw,noexec,nosuid,size=64m',
                   '--publish', f'127.0.0.1:{backend_port}:4319',
                   '--env', 'RENTEMESTER_ORIGIN=' + origin,
                   '--env', 'RENTEMESTER_CONFIG=/config/server.json',
                   '--mount', f'type=volume,src={volumes[0]},dst=/state',
                   '--mount', f'type=volume,src={volumes[1]},dst=/config,readonly',
                   image_id, *(['--create'] if create else []))
            container_created = True
            actual = json.loads(docker('inspect', container))[0]
            host = actual['HostConfig']
            if (actual['Image'] != image_id or actual['Config']['User'] != '10001:10001'
                    or not host['ReadonlyRootfs'] or host['Memory'] != 1024 ** 3
                    or host['PidsLimit'] != 128 or host['CapDrop'] != ['ALL']
                    or 'no-new-privileges:true' not in host['SecurityOpt']
                    or host['PortBindings'].get('4319/tcp') != [
                        {'HostIp': '127.0.0.1', 'HostPort': str(backend_port)}]
                    or not any(m['Destination'] == '/config' and not m['RW']
                               for m in actual['Mounts'])):
                raise RuntimeError('Actual packaged container isolation/pin mismatch')
            report['containerIsolationVerified'] = True
        else:
            start_host(create)
        for _ in range(300):
            if args.image:
                state = json.loads(docker('inspect', container))[0]['State']
                if not state['Running']:
                    raise RuntimeError('Packaged server exited before readiness; private logs retained')
            elif child.poll() is not None:
                raise RuntimeError('Owned native server exited before readiness')
            conn = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=.3)
            try:
                conn.request('GET', '/api/health', headers={'host': authority})
                r = conn.getresponse()
                if r.status == 200 and json.loads(r.read()).get('workspaceReady'):
                    return
            except (OSError, ValueError, http.client.HTTPException):
                pass
            finally:
                conn.close()
            time.sleep(.1)
        raise RuntimeError('Owned server readiness deadline')

    def start_host(create):
        nonlocal child, log
        log = (work / 'server.log').open('ab')
        child = subprocess.Popen([str(binary), '--config', str(work / 'server.json'), '--web-dir', str(web), '--listen', f'127.0.0.1:{backend_port}', '--origin', origin] + (['--create'] if create else []), cwd=REPO, stdout=log, stderr=log)
        report.setdefault('serverPids', []).append(child.pid)

    def stop(remove=True):
        nonlocal child, log, container_created
        if args.image and container_created:
            state = json.loads(docker('inspect', container))[0]['State']
            if state['Running']:
                docker('stop', '--time', '60', container)
            with (work / 'server.log').open('ab') as output:
                output.write(docker('logs', container))
            state = json.loads(docker('inspect', container))[0]['State']
            report.setdefault('containerStops', []).append(
                {'exitCode': state['ExitCode'], 'oomKilled': state['OOMKilled']})
            if remove:
                if state['ExitCode'] != 0 or state['OOMKilled']:
                    raise RuntimeError('Packaged server did not stop cleanly')
                docker('rm', container)
                container_created = False
        if child is not None:
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
            child = None
        if log:
            log.close()
            log = None

    def phase(command, seconds=300):
        controller.stdin.write(json.dumps(command) + '\n')
        controller.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(controller.stdout, selectors.EVENT_READ)
            if not selector.select(seconds):
                raise RuntimeError('Bounded browser phase timeout: ' + command)
            line = controller.stdout.readline()
        try:
            value = json.loads(line)
        except ValueError:
            raise RuntimeError('Browser controller ended without sanitized receipt') from None
        if not value.get('ok'):
            report['failureStep'] = value.get('step', command)
            report['assertion'] = value.get('detail')
            report['browserErrorType'] = value.get('errorType')
            report['browserOperation'] = value.get('operation')
            report['pageState'] = value.get('pageState')
            report['authResponseTrace'] = [{**item, 'path': re.sub(r'^/api/auth/reset-password/[^/]+$', '/api/auth/reset-password/:token', item.get('path', ''))} for item in value.get('trace', [])]
            raise RuntimeError('Browser assertion failed at ' + report['failureStep'])
        return value['value']

    try:
        if args.image:
            image = json.loads(docker('image', 'inspect', args.image))[0]
            image_id = image['Id']
            labels = image['Config'].get('Labels') or {}
            report.update(imageId=image_id, architecture=image['Architecture'],
                          imageUser=image['Config']['User'], imageLabels=labels,
                          controllerPlatform='ordinary-macOS-DockerDesktop',
                          ownedContainer=container, ownedVolumes=volumes)
            if (not image_id.startswith('sha256:') or image.get('Os') != 'linux'
                    or image['Config']['User'] != '10001:10001'
                    or labels.get('org.rentemester.runtime') != 'rust'):
                raise RuntimeError('Expected pinned Linux Rust image with user 10001:10001')
            for volume in volumes:
                docker('volume', 'create', volume)
                created_volumes.append(volume)
        else:
            report.update(serverSha256=digest(binary), webFiles={
                str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()})
        # Independent local CA, exact localhost leaf; no system trust changes.
        tls_log = (work / 'openssl.log').open('wb')
        commands = [
            ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Auth Fixture CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
            ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(private), '-out', str(work / 'tls.csr'), '-subj', '/CN=localhost'],
        ]
        (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,DNS:host.docker.internal,IP:127.0.0.1\n')
        commands.append(['openssl', 'x509', '-req', '-in', str(work / 'tls.csr'), '-CA', str(ca), '-CAkey', str(work / 'ca.key'), '-CAcreateserial', '-out', str(cert), '-days', '1', '-extfile', str(work / 'tls.ext')])
        try:
            for command in commands:
                subprocess.run(command, check=True, stdout=tls_log, stderr=tls_log, timeout=30)
        finally:
            tls_log.close()
        public = subprocess.check_output(['openssl', 'x509', '-in', str(cert), '-pubkey', '-noout'], timeout=10)
        der = subprocess.check_output(['openssl', 'pkey', '-pubin', '-outform', 'DER'], input=public, timeout=10)
        spki = base64.b64encode(hashlib.sha256(der).digest()).decode()
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(cert, private)
        for port, handler in [(proxy_port, Proxy), (mail_port, Gateway)]:
            # Docker Desktop routes host.docker.internal to the host listener, not container loopback.
            bind = '0.0.0.0' if args.image and handler is Gateway else '127.0.0.1'
            server = QuietServer((bind, port), handler)
            server.socket = tls.wrap_socket(server.socket, server_side=True)
            threading.Thread(target=server.serve_forever, daemon=True).start()
            servers.append(server)
        key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
        config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-hosted-auth', 'companySlug': 'synthetic', 'companyName': 'Synthetic Hosted Authentication', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
        if args.image:
            config.update(runtimeProfile='dedicated-linux', root='/state/workspace', brokerUid=10001)
            config['authMail'].update(endpoint=f'https://host.docker.internal:{mail_port}/mail',
                                      trustedCaFile='/config/ca.pem')
            docker('run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                   '--mount', f'type=volume,src={volumes[1]},dst=/config',
                   '--entrypoint', 'sh', image_id, '-c',
                   'umask 077; cat > /config/ca.pem; chown 10001:10001 /config/ca.pem; '
                   'chmod 0600 /config/ca.pem', input=ca.read_bytes())
        write_config(config)
        start(True)
        if args.image:
            # One bounded diagnosis using the actual native TLS mail sender, no alternate DNS/platform.
            deadline = time.monotonic() + 12
            while not gateway_rows and time.monotonic() < deadline:
                time.sleep(.1)
            if not gateway_rows:
                report['hostGatewayDiagnosis'] = 'No native verification receipt at TLS host.docker.internal gateway; inspect preserved server.log for DNS/connect/TLS refusal. No retry or network workaround.'
                raise RuntimeError('Docker Desktop host TLS gateway receipt unavailable')
            report['hostGatewayReachableWithPrivateCa'] = True
        helper = work / 'browser.mjs'
        helper.write_text(BROWSER)
        env = {**os.environ, 'RM_AUTH_BASE': origin, 'RM_AUTH_WORK': str(work), 'RM_AUTH_SPKI': spki, 'RM_AUTH_PLAYWRIGHT': str(REPO / 'node_modules/@playwright/test/index.mjs')}
        controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(helper)], cwd=REPO, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=(work / 'browser-errors.log').open('wb'), text=True, start_new_session=True)
        report['browserControllerPid'] = controller.pid
        report.update(phase('setup'))
        stop()
        del config['firstOwner']
        write_config(config)
        start()
        report.update(phase('reopen', 120))
        # No challenge/cookie: the first three admissions fail before native MFA dispatch.
        # The fourth must hit the same TCP-peer bucket despite rotating claimed identities.
        # This is last, so no account or financial fixture is locked out.
        rate = []
        rate_started = time.monotonic()
        for n in range(4):
            connection = http.client.HTTPSConnection('localhost', proxy_port, timeout=10,
                                                    context=ssl.create_default_context(cafile=str(ca)))
            try:
                connection.request('POST', '/api/auth/two-factor/verify-totp',
                                   json.dumps({'code': '123456'}),
                                   {'host': authority, 'origin': origin, 'content-type': 'application/json',
                                    'x-forwarded-for': f'192.0.2.{n + 1}',
                                    'user-agent': f'synthetic-rate-{n}'})
                response = connection.getresponse()
                body = json.loads(response.read())
                expected = 401 if n < 3 else 429
                if (response.status != expected or response.getheader('set-cookie') is not None
                        or (n < 3 and response.getheader('retry-after') is not None)
                        or (n == 3 and (response.getheader('retry-after') != '60'
                                        or body.get('code') != 'request_refused'))):
                    raise RuntimeError('Fixed factor TCP-peer rate bucket assertion')
                rate.append({'status': response.status, 'retryAfter': response.getheader('retry-after')})
            finally:
                connection.close()
        if time.monotonic() - rate_started >= 10:
            raise RuntimeError('Factor rate proof exceeded fixed ten-second window')
        report['clientIpRate'] = {'sameTcpPeerDespiteRotatingForwardedIpAndUserAgent': True,
                                  'noAccountMfaAttempts': True, 'responses': rate}
        report['httpsProxyExactHost'] = True
        phase('finish', 20)
        controller.stdin.close()
        controller.wait(timeout=10)
        if controller.returncode != 0:
            raise RuntimeError('Browser controller nonzero terminal')
        with gateway_lock:
            report['gateway'] = {'https': True, 'trustedCaSha256': digest(ca), 'acceptedMessages': len(gateway_rows), 'requests': list(gateway_requests), 'timeoutAfterAcceptanceCount': sum(r['timeoutAfterAcceptance'] for r in gateway_requests)}
            if errors or sum(r['kind'] == 'password-reset' for r in gateway_rows.values()) != 1 or report['gateway']['timeoutAfterAcceptanceCount'] != 1:
                raise RuntimeError('Exact accepted-reset/idempotency gateway assertion')
        report['ok'] = True
        print('PASS: HTTPS ordinary owner/invitation/MFA, timeout-accepted password reset, refusal and restart')
    except Exception as error:
        report['failure'] = type(error).__name__
        if args.image and not gateway_rows:
            report['hostGatewayDiagnosis'] = ('No native verification receipt at controlled TLS '
                'host.docker.internal gateway; inspect preserved server.log for startup/DNS/connect/'
                'TLS refusal. No retry or alternate network attempted.')
        raise
    finally:
        if controller is not None and controller.poll() is None:
            os.killpg(controller.pid, signal.SIGTERM)
            try:
                controller.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(controller.pid, signal.SIGKILL)
                controller.wait(timeout=5)
        try:
            stop(remove=not args.image or report['ok'])
        except Exception as cleanup_error:
            report['stopFailure'] = type(cleanup_error).__name__
            report['ok'] = False
        if args.image and report['ok']:
            try:
                for volume in created_volumes:
                    docker('volume', 'rm', volume)
                report['ownedContainerAndVolumesRemoved'] = True
            except Exception as cleanup_error:
                report['cleanupFailure'] = type(cleanup_error).__name__
                report['ok'] = False
        if args.image and not report['ok']:
            report['privateFailureFixturePreserved'] = True
        for server in servers:
            server.shutdown()
            server.server_close()
        same = work.exists() and (work.stat().st_dev, work.stat().st_ino, work.stat().st_uid) == (identity.st_dev, identity.st_ino, os.getuid())
        if (not args.image or report['ok']) and same and work.parent == parent and work.name.startswith(PREFIX):
            shutil.rmtree(work)
            report['ownedRuntimeCleaned'] = True
        else:
            report['ownedRuntimeCleaned'] = False
        args.evidence.parent.mkdir(parents=True, exist_ok=True)
        with args.evidence.open('x') as output:
            json.dump(report, output, indent=2)
        if report.get('stopFailure') or report.get('cleanupFailure'):
            raise RuntimeError('Owned packaged cleanup failed; see sanitized evidence and preserved fixture')


if __name__ == '__main__':
    main()
