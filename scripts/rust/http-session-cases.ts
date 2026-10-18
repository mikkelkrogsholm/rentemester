/** Real native sockets with source-signed current cookies and canonical synthetic controls. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
export async function verifyHttpSessions(c: any) {
  const { fixture, start, key, now, note } = c;
  const token = "http-sessions-private-current", otherToken = "http-sessions-private-other";
  const cookie = (await serializeSignedCookie("better-auth.session_token", token, key)).split(";")[0]!;
  const observations: string[] = [];
  function record(id: string) { observations.push(id); note(`session-control/${id}`); }
  function db<T>(f: any, fn: (d: Database) => T): T { const d = new Database(f.control); try { return fn(d); } finally { d.close(); } }
  function q(f: any, s: string) { return db(f, d => d.query(s).all() as any[]); }
  function snapshot(f: any) { return JSON.stringify(["session", "user", "account", "verification", "twoFactor", "workspace_audit", "rm_workspace_auth_state_events"].map(t => q(f, `SELECT * FROM ${t} ORDER BY rowid`))); }
  function fresh(id: string, mode = "DELETE", age = 1000) {
    const f = fixture(`sessions-${id}`, mode);
    db(f, d => {
      d.exec("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('foreign','Foreign','foreign@example.invalid',1,'2026-01-01','2026-01-01',1)");
      const insert = d.query("INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt,ipAddress,userAgent) VALUES(?,?,?,?,?,?,?,?)");
      for (const [id, user, bearer, created, expires, agent] of [
        ["current", "u", token, now-age, now+3600000, "Safari private raw agent"],
        ["other", "u", otherToken, now-2000, now+3600000, "Firefox private raw agent"],
        ["expired", "u", "private-expired-bearer", now-7200000, now-1, null],
        ["foreign-session", "foreign", "private-foreign-bearer", now-1000, now+3600000, "Chrome"],
      ] as const) insert.run(id, user, bearer, new Date(created).toISOString(), new Date(created).toISOString(), new Date(expires).toISOString(), null, agent);
    });
    return { ...f, outcomeLog: join(dirname(f.control), "session-outcomes.jsonl") };
  }
  function outcomes(f: any) { return existsSync(f.outcomeLog) ? readFileSync(f.outcomeLog, "utf8").trim().split("\n").filter(Boolean).map(l => JSON.parse(l)) : []; }
  async function request(s: any, path: string, body?: unknown, extra: Record<string, string> = {}, method?: string) {
    const r = await fetch(`${s.origin}/api/auth/${path}`, { method: method ?? (body === undefined ? "GET" : "POST"), headers: { cookie, ...(body === undefined ? {} : { origin: s.origin, "content-type": "application/json" }), ...extra }, body: body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body), redirect: "manual" });
    assert.equal(r.headers.get("cache-control"), "no-store");
    assert.equal(r.headers.get("referrer-policy"), "no-referrer");
    return r;
  }
  async function success(r: Response, clear = false) {
    assert.equal(r.status, 200); assert.deepEqual(await r.json(), { status: true });
    const cookies = r.headers.getSetCookie();
    if (clear) { assert.equal(cookies.length, 4); assert(cookies.every(c => c.includes("Max-Age=0"))); }
    else assert.deepEqual(cookies, []);
  }
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`${mode}-flow`, mode), s = await start(f, { sessions: true });
    try {
      for (let i=0;i<12;i++) assert.equal((await request(s, "list-sessions", undefined, {}, "HEAD")).status, 405);
      assert.equal(outcomes(f).length, 0); record(`${mode}-HEAD-no-dispatch-or-rate-charge`);
      const before = q(f, "SELECT count(*) n FROM workspace_audit")[0].n;
      const listed = await request(s, "list-sessions"); assert.equal(listed.status, 200); assert.equal(listed.headers.get("set-cookie"), null);
      const rows = await listed.json() as any[];
      assert.deepEqual(rows.map(r => [r.id, r.current]), [["current", true], ["other", false]]);
      for (const r of rows) assert.deepEqual(Object.keys(r).sort(), ["createdAt", "current", "expiresAt", "id", "ipAddress", "userAgent"].sort());
      assert(!JSON.stringify(rows).includes(token)); assert(!JSON.stringify(rows).includes(otherToken));
      assert.equal(q(f, "SELECT count(*) n FROM workspace_audit")[0].n, before+1);
      record(`${mode}-fresh-owned-active-token-free-stable-list-with-audit`);
      for (const id of ["foreign-session", "missing"]) await success(await request(s, "revoke-session", { sessionId: id }));
      assert.equal(q(f, "SELECT count(*) n FROM session WHERE id='foreign-session'")[0].n, 1);
      record(`${mode}-missing-and-foreign-targets-indistinguishable-audited-noop`);
      await success(await request(s, "revoke-session", { sessionId: "other" }));
      assert.equal(q(f, "SELECT count(*) n FROM session WHERE id='other'")[0].n, 0);
      record(`${mode}-one-owned-ID-deleted-without-cookie-clear`);
      await success(await request(s, "revoke-other-sessions", {}));
      assert.deepEqual(q(f, "SELECT id FROM session ORDER BY id").map(r=>r.id), ["current", "foreign-session"]);
      record(`${mode}-others-atomically-removes-expired-and-preserves-current-and-foreign`);
      await success(await request(s, "revoke-sessions", {}), true);
      assert.deepEqual(q(f, "SELECT id FROM session").map(r=>r.id), ["foreign-session"]);
      const audit = q(f, "SELECT event_type,entity_id FROM workspace_audit WHERE entity_type='native_session_control_v1'");
      assert.equal(audit.length, 6); assert(!JSON.stringify(audit).includes(token));
      record(`${mode}-all-clears-only-known-commit-and-exact-own-deletion-audit`);
      const refused = await request(s, "list-sessions"); assert.equal(refused.status, 401); assert.equal(refused.headers.get("set-cookie"), null);
      record(`${mode}-revoked-cookie-cannot-list-or-resurrect`);
    } finally { await s.stop(); }
    const own = fresh(`${mode}-own`, mode), owner = await start(own, { sessions: true });
    try { await success(await request(owner, "revoke-session", { sessionId: "current" }), true); assert.equal(q(own, "SELECT count(*) n FROM session WHERE id='current'")[0].n, 0); record(`${mode}-one-current-clears-only-after-deletion`); }
    finally { await owner.stop(); }
  }
  const invalid: { id: string, path: string, body?: any, headers?: any, method?: string, status: number }[] = [
    { id: "no-cookie", path: "list-sessions", headers: {cookie:"unrelated=1"}, status:401 },
    { id: "bad-signature", path:"revoke-session",body:{sessionId:"other"},headers:{cookie:"better-auth.session_token=invalid"},status:401 },
    { id:"bare-beside-valid", path:"list-sessions",headers:{cookie:`better-auth.session_token; ${cookie}`},status:401 },
    { id:"service",path:"list-sessions",headers:{authorization:"Bearer private-service"},status:401 },
    { id:"origin",path:"revoke-sessions",body:{},headers:{origin:"https://foreign.invalid"},status:403 },
    { id:"cross-site",path:"list-sessions",headers:{"sec-fetch-site":"cross-site"},status:403 },
    { id:"query",path:"list-sessions?userId=foreign",status:400 },
    { id:"bearer-target",path:"revoke-session",body:{token:otherToken},status:400 },
    { id:"duplicate-target",path:"revoke-session",body:'{"sessionId":"other","sessionId":"current"}',status:400 },
    { id:"extra-subject",path:"revoke-session",body:{sessionId:"other",userId:"foreign"},status:400 },
    { id:"blank-target",path:"revoke-session",body:{sessionId:" "},status:400 },
    { id:"control-target",path:"revoke-session",body:{sessionId:"other\n"},status:400 },
    { id:"long-target",path:"revoke-session",body:{sessionId:"x".repeat(4097)},status:400 },
    { id:"null-target",path:"revoke-session",body:{sessionId:null},status:400 },
    { id:"array-target",path:"revoke-session",body:["other"],status:400 },
    { id:"array-empty",path:"revoke-sessions",body:[],status:400 },
    { id:"unknown-empty",path:"revoke-other-sessions",body:{userId:"u"},status:400 },
    { id:"body-budget",path:"revoke-session",body:' '.repeat(8193),status:413 },
  ];
  for (const test of invalid) {
    const f=fresh(`invalid-${test.id}`),s=await start(f,{sessions:true});
    try { const before=snapshot(f),r=await request(s,test.path,test.body,test.headers,test.method); assert.equal(r.status,test.status,test.id); assert.equal(r.headers.get("set-cookie"),null); assert.equal(snapshot(f),before); assert.deepEqual(outcomes(f),[]); record(`${test.id}-refused-before-native-dispatch`); }
    finally { await s.stop(); }
  }
  for (const [id, mutate, expected] of [
    ["stale", "", 401], ["disabled", "INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')", 401],
    ["malformed-owned-display", "UPDATE session SET userAgent=char(10) WHERE id='other'", 503],
  ] as const) {
    const f=fresh(id,"WAL",id==="stale"?600000:1000); if(mutate)db(f,d=>d.exec(mutate)); const s=await start(f,{sessions:true});
    try {const before=snapshot(f),r=await request(s,"list-sessions");assert.equal(r.status,expected,id);assert.equal(r.headers.get("set-cookie"),null);assert.equal(snapshot(f),before);record(`${id}-withholds-list-and-headers-no-partial-audit`);}
    finally {await s.stop();}
  }
  for (const path of ["list-sessions", "revoke-sessions"]) {
    const f=fresh(`unknown-${path}`),s=await start(f,{sessions:true}); const blocker=new Database(f.control);
    try {blocker.exec("BEGIN;SELECT * FROM session;"); const before=snapshot(f),r=await request(s,path,path==="list-sessions"?undefined:{});assert.equal(r.status,503);assert.deepEqual(await r.json(),{message:"Authentication outcome could not be confirmed",code:"AUTH_UNCONFIRMED"});assert.equal(r.headers.get("set-cookie"),null);assert.equal(snapshot(f),before);assert.equal(outcomes(f).at(-1)?.stage,"OutcomeUnknown");record(`${path}-real-DELETE-commit-busy-withholds-data-success-and-cookies`);}
    finally {blocker.exec("ROLLBACK");blocker.close();await s.stop();}
  }
  const rate=fresh("rate"),s=await start(rate,{sessions:true});
  try {for(let i=0;i<10;i++)await success(await request(s,i%2?"revoke-session":"revoke-other-sessions",i%2?{sessionId:`missing-${i}`}:{},{"x-forwarded-for":`203.0.113.${i}`}));const before=snapshot(rate),r=await request(s,"revoke-sessions",{});assert.equal(r.status,429);assert.equal(r.headers.get("retry-after"),"60");assert.equal(r.headers.get("set-cookie"),null);assert.equal(snapshot(rate),before);assert.equal(outcomes(rate).length,10);record("revoke-family-shares-direct-peer-budget-ignores-forwarded-and-target-rotation");}
  finally {await s.stop();}
  const absent=fresh("disabled-router"),none=await start(absent);
  try {const before=snapshot(absent),r=await request(none,"list-sessions");assert.equal(r.status,404);assert.equal(snapshot(absent),before);record("missing-trusted-handler-fails-closed-no-legacy-fallback");}
  finally {await none.stop();}
  assert(![token,otherToken,key,cookie,"foreign@example.invalid"].some(secret=>JSON.stringify(observations).includes(secret)));
}
