import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

/** Bounded extension of the existing normal-MFA runner; never retries a write. */
export function originalExtractionProof(d: any) {
  const { base, work, need, ownerPage, documents, read, prefix } = d;
  let issued: any, pending: Promise<any>, sequence = 0;
  const cookie = join(work, 'original-extraction.cookie'), key = join(work, 'original-extraction.key');
  const evidence: any[] = [];
  function record(value: any) {
    evidence.push(value);
    writeFileSync(join(work, 'original-extraction-observations.json'), JSON.stringify(evidence), { mode: 0o600 });
  }
  async function admin(path: string, body: any) {
    const r = await ownerPage.request.post(base + '/api/workspace/' + path, { headers: { origin: base }, data: body });
    const value = await r.json();
    need(r.status() === (path === 'service-principals' ? 201 : 200) && value?.ok === true, 'Original service administration ' + path + ' HTTP' + r.status());
    return value;
  }
  function invoke(kind: string, write: boolean, id: number, service = false, confirm: any = true) {
    const common = ['--server', base, '--allow-loopback-http', service ? '--service-key-file' : '--cookie-file', service ? key : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') argv = ['documents', write ? 'extract-invoice' : 'invoice-extraction', ...common, '--company', 'synthetic', '--document-id', String(id), '--json'];
    else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'original-extraction167', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: write ? 'documents_extract_invoice' : 'documents_invoice_extraction', arguments: { company: 'synthetic', documentId: id, ...(write && confirm !== null ? { confirm } : {}) } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(service ? key : cookie, 'utf8').trim();
    record({ sequence: ++sequence, kind, write, id, service, confirm: confirm ?? 'missing', exit: r.status, terminalError: r.error?.name });
    need(!r.error && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'Original extraction terminal/privacy');
    if (kind === 'cli') { const value = JSON.parse(r.stdout); record({ sequence, value }); need(r.status === 0, 'Original extraction CLI success exit'); return value; }
    need(r.status === 0, 'Original extraction MCP terminal');
    const reply = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    record({ sequence, reply });
    need(!reply?.error && isDeepStrictEqual(JSON.parse(reply.result.content[0].text), reply.result.structuredContent), 'MCP text equals structured content');
    const value = reply.result.structuredContent;
    if (write && confirm !== true) {
      need(reply.result.isError === true && value.ok === false && !Object.hasOwn(value, 'data') && JSON.stringify(value).includes('CONFIRM_REQUIRED'), 'Missing/false MCP confirm refused');
      return value;
    }
    need(reply.result.isError === false && value.ok === true && Object.keys(value.data).join() === 'extraction', 'Original MCP source envelope');
    return { ok: true, extraction: value.data.extraction };
  }
  async function cookies() { writeFileSync(cookie, (await ownerPage.context().cookies(base)).map((c: any) => `${c.name}=${c.value}`).join('; '), { mode: 0o600 }); }
  async function prepare() {
    await cookies();
    issued = await admin('service-principals', { displayName: 'Synthetic original extraction', operationId: crypto.randomUUID(), confirm: true });
    await admin('members/access', { userId: issued.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
    await admin('members/company', { userId: issued.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    writeFileSync(key, issued.secret + '\n', { mode: 0o600, flag: 'wx' });
    // No selected positive ID has extraction evidence yet. Both confirmations are local refusals.
    for (const service of [false, true]) for (const confirm of [null, false]) invoke('mcp', true, documents.good.id, service, confirm);
    for (const service of [false, true]) for (const kind of ['cli', 'mcp']) {
      const value = invoke(kind, false, Number.MAX_SAFE_INTEGER, service);
      need(isDeepStrictEqual(value, { ok: true, extraction: null }), 'Admitted missing original is exact flat null');
    }
    const good = invoke('cli', true, documents.good.id);
    need(isDeepStrictEqual(good, await read(d.path('good'))), 'Original CLI first extraction retained exactly');
    const conflict = invoke('mcp', true, documents.conflict.id, true);
    need(conflict.extraction.status === 'needs_resolution' && isDeepStrictEqual(conflict, await read(d.path('conflict'))), 'Service MCP first conflict is successful retained evidence');
    return { serviceAccountId: issued.serviceAccountId, originalCliFirstWrite: good, serviceMcpFirstWrite: conflict, observations: evidence };
  }
  async function clients() {
    await cookies();
    for (const service of [false, true]) for (const kind of ['cli', 'mcp']) {
      for (const name of d.kinds) need(isDeepStrictEqual(invoke(kind, false, documents[name].id, service), await read(d.path(name))), 'Exact original read projection ' + name);
      need(isDeepStrictEqual(invoke(kind, true, documents.good.id, service), await read(d.path('good'))), 'Current-authority original cache equality');
    }
    return { observations: evidence };
  }
  async function holdBegin() {
    const args = ['documents', 'extract-invoice', '--server', base, '--allow-loopback-http', '--service-key-file', key, '--company', 'synthetic', '--document-id', String(documents.held.id), '--json'];
    const child = spawn(join(work, 'rentemester-rust'), args, { stdio: ['ignore', 'pipe', 'pipe'] });
    pending = new Promise((resolve, reject) => {
      let out = '', err = '';
      const timer = setTimeout(() => { child.kill(); reject(Error('Held original CLI deadline; no retry')); }, 45000);
      child.stdout.on('data', b => out += b); child.stderr.on('data', b => err += b);
      child.on('error', reject); child.on('close', code => { clearTimeout(timer); resolve({ code, out, err }); });
    });
    return { serviceRequestStarted: true };
  }
  async function revoke() {
    await admin('service-principals/revoke', { serviceAccountId: issued.serviceAccountId, credentialId: issued.credentialId, operationId: crypto.randomUUID(), confirm: true });
    return { credentialRevokedBeforeProviderRelease: true };
  }
  async function holdFinish() {
    const r = await pending;
    const secret = readFileSync(key, 'utf8').trim();
    need(!r.out.includes(secret) && !r.err.includes(secret), 'Revocation diagnostics privacy');
    record({ held: r });
    need(r.code === 1, 'Held service extraction refused after credential revocation');
    const current = await read(d.path('held'));
    need(current.extraction.status === 'started' && current.extraction.fields.length === 0 && current.extraction.resolutions.length === 0, 'Only legitimate started evidence remains');
    return { refusal: JSON.parse(r.out), evidence: current };
  }
  return { prepare, clients, holdBegin, revoke, holdFinish };
}
