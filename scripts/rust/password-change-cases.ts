/** Own password-change atomicity, current-credential fence, optional rotation and source crypto. */
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { Database } from "bun:sqlite";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import {
  hashPassword,
  verifyPassword,
} from "../../node_modules/better-auth/dist/crypto/password.mjs";
export async function verifyPasswordChange(c: any) {
  const { fixture, run, start, done, marker, sql, image, record, now, key } = c;
  const password = "Synthetic password 123!",
    replacement = "Changed synthetic password 456!",
    token = "password-change-private-session";
  const cookie = (
    await serializeSignedCookie(
      "__Secure-better-auth.session_token",
      token,
      key,
    )
  ).split(";")[0]!;
  const sensitive = [password, replacement, token, cookie],
    note = (id: string) => record(`password-change/${id}`);
  function q(f: any, s: string) {
    const d = new Database(f.control);
    try {
      return d.query(s).all() as any[];
    } finally {
      d.close();
    }
  }
  function fresh(id: string, mode = "delete", age = 1000) {
    const f = fixture(`password-change-${id}`, mode);
    const d = new Database(f.control);
    try {
      d.exec(
        "INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('foreign','Foreign','foreign@example.invalid',1,'2026-01-01','2026-01-01',1)",
      );
      const insert = d.query(
        "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
      );
      for (const [id, subject, t, created, expires] of [
        ["current", "u", token, now - age, now + 3600000],
        ["other", "u", "private-other-session", now - 2000, now + 3600000],
        ["expired", "u", "private-expired-session", now - 7200000, now - 1],
        [
          "foreign-session",
          "foreign",
          "private-foreign-session",
          now - 1000,
          now + 3600000,
        ],
      ] as const)
        insert.run(
          id,
          subject,
          t,
          new Date(created).toISOString(),
          new Date(created).toISOString(),
          new Date(expires).toISOString(),
        );
      for (const [id, identifier, value] of [
        ["own-reset", "reset-password:AbCdEfGhIjKlMnOpQrStUv01", "u"],
        ["foreign-reset", "reset-password:AbCdEfGhIjKlMnOpQrStUv02", "foreign"],
        ["unrelated", "unrelated:synthetic", "u"],
      ])
        d.query(
          "INSERT INTO verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
        ).run(
          id,
          identifier,
          value,
          new Date(now - 1000).toISOString(),
          new Date(now - 1000).toISOString(),
          new Date(now + 3600000).toISOString(),
        );
    } finally {
      d.close();
    }
    return f;
  }
  const request = (extra: any = {}) => ({
    mode: "password-change",
    cookie,
    newPassword: replacement,
    ...extra,
  });
  const change = (f: any, extra: any = {}) => run(f, request(extra));
  for (const mode of ["delete", "wal"])
    for (const rotate of [false, true]) {
      const f = fresh(`${mode}-${rotate}`, mode);
      const initial = run(f, { mode: "begin" });
      assert.equal(initial.pending, true);
      const counter = Buffer.alloc(8);
      counter.writeBigUInt64BE(BigInt(Math.floor(now / 30000)));
      const mac = createHmac("sha1", "12345678901234567890")
          .update(counter)
          .digest(),
        offset = mac[mac.length - 1]! & 15;
      const code = String(
        (mac.readUInt32BE(offset) & 0x7fffffff) % 1000000,
      ).padStart(6, "0");
      const enrolled = run(f, { cookie: initial.cookie, code });
      assert.equal(enrolled.session, true);
      const replay = q(
        f,
        "SELECT * FROM verification WHERE identifier LIKE 'rm-native-totp-step-v1:%'",
      );
      assert.equal(replay.length, 1);
      const pending = run(f, { mode: "begin" });
      assert.equal(pending.pending, true);
      const oldSessions = q(f, "SELECT * FROM session ORDER BY id"),
        factor = q(f, "SELECT * FROM twoFactor"),
        user = q(f, "SELECT * FROM user"),
        oldHash = q(f, "SELECT password FROM account WHERE id='a'")[0].password;
      const beforeEvents = q(f, "SELECT * FROM rm_workspace_auth_state_events"),
        r = change(f, { revokeOtherSessions: rotate });
      assert.equal(r.ok, true);
      assert.equal(r.user.id, "u");
      assert(!JSON.stringify(r.user).includes(token));
      assert.equal(Boolean(r.cookie), rotate);
      if (r.cookie) sensitive.push(r.cookie);
      const hash = q(f, "SELECT password FROM account WHERE id='a'")[0]
        .password;
      sensitive.push(hash, oldHash);
      assert.notEqual(hash, oldHash);
      assert(await verifyPassword({ hash, password: replacement }));
      assert(!(await verifyPassword({ hash, password })));
      note(
        `${mode}-${rotate}-source-KDF-verifies-new-only-bounded-own-receipt`,
      );
      assert.deepEqual(q(f, "SELECT * FROM twoFactor"), factor);
      assert.deepEqual(
        q(
          f,
          "SELECT * FROM verification WHERE identifier LIKE 'rm-native-totp-step-v1:%'",
        ),
        replay,
      );
      assert.deepEqual(q(f, "SELECT * FROM user"), user);
      const afterEvents = q(
        f,
        "SELECT state_transition FROM rm_workspace_auth_state_events ORDER BY id",
      )
        .slice(beforeEvents.length)
        .map((x) => x.state_transition);
      assert.equal(
        afterEvents.filter((x) => x === "credential_updated").length,
        1,
      );
      if (rotate) {
        assert.equal(
          q(f, "SELECT count(*) n FROM session WHERE userId='u'")[0].n,
          1,
        );
        assert.equal(
          afterEvents.filter((x) => x === "session_deleted").length,
          oldSessions.filter((x) => x.userId === "u").length,
        );
        assert.equal(
          afterEvents.filter((x) => x === "session_created").length,
          1,
        );
        assert.equal(
          q(f, "SELECT count(*) n FROM session WHERE id='foreign-session'")[0]
            .n,
          1,
        );
        assert(
          !q(f, "SELECT token FROM session WHERE userId='u'").some(
            (x) => x.token === token,
          ),
        );
      } else {
        assert.deepEqual(
          q(f, "SELECT * FROM session ORDER BY id"),
          oldSessions,
        );
        assert.deepEqual(afterEvents, ["credential_updated"]);
      }
      note(
        `${mode}-${rotate}-atomic-session-lifecycle-and-preserved-factor-user-replay`,
      );
      assert.deepEqual(
        q(
          f,
          "SELECT id FROM verification WHERE id IN ('own-reset','foreign-reset','unrelated') ORDER BY id",
        ).map((x) => x.id),
        ["foreign-reset", "unrelated"],
      );
      const audit = q(
        f,
        "SELECT event_type,entity_id,actor FROM workspace_audit WHERE entity_type='native_password_change_v1'",
      );
      assert.equal(audit.length, 1);
      assert.equal(audit[0].event_type, "password_changed");
      assert.equal(audit[0].actor, "user:u");
      const entity = JSON.parse(audit[0].entity_id);
      assert.equal(entity[0], "u");
      assert.equal(entity[1], "current");
      assert.equal(
        entity[4],
        rotate ? oldSessions.filter((x) => x.userId === "u").length : 0,
      );
      assert.equal(entity[5], 1);
      assert(!sensitive.some((v) => JSON.stringify(audit).includes(v)));
      note(
        `${mode}-${rotate}-own-reset-invalidation-and-secret-free-exact-audit`,
      );
      const before = image(f),
        stale = run(f, { cookie: pending.cookie, code: "000000" });
      assert.equal(stale.ok, false);
      assert.equal(image(f), before);
      note(
        `${mode}-${rotate}-credential-event-invalidates-earlier-pending-context`,
      );
      assert.equal(run(f, { mode: "begin", password }).ok, false);
      assert.equal(
        run(f, { mode: "begin", password: replacement }).pending,
        true,
      );
      note(
        `${mode}-${rotate}-old-login-refuses-new-password-continues-only-to-MFA`,
      );
    }
  for (const [id, mutate, extra, age] of [
    [
      "wrong-password",
      "",
      { password: "Wrong synthetic private password" },
      1000,
    ],
    ["new-too-short", "", { newPassword: "short" }, 1000],
    ["stale", "", {}, 600000],
    [
      "disabled",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
      {},
      1000,
    ],
    ["unverified-factor", "UPDATE twoFactor SET verified=0", {}, 1000],
    ["revoked", "DELETE FROM session WHERE id='current'", {}, 1000],
    [
      "bad-user-result",
      "UPDATE user SET image=char(10) WHERE id='u'",
      { revokeOtherSessions: true },
      1000,
    ],
    ["late-SQL", "", { revokeOtherSessions: true, failSql: true }, 1000],
    ["wrong-workspace", "", { proofWorkspace: "foreign" }, 1000],
    ["audit-version", "", { auditVersion: 9007199254740992 }, 1000],
  ] as const) {
    const f = fresh(id, "wal", age);
    if (mutate) sql(f, mutate);
    const before = image(f),
      r = change(f, extra);
    assert.equal(r.ok, false, id);
    assert.equal(r.stage, "NotCommitted");
    assert(!("cookie" in r));
    assert(!("user" in r));
    assert.equal(image(f), before);
    note(`${id}-no-credential-session-link-audit-or-receipt-effect`);
  }
  const alternate = await hashPassword("Concurrent private password 789!");
  sensitive.push(alternate);
  for (const point of ["change-hashed", "password:unlocked"])
    for (const mutation of ["credential", "session", "access"]) {
      const f = fresh(`race-${point}-${mutation}`);
      const p = start(f, request({ revokeOtherSessions: true }), point);
      await marker(p, f);
      const d = new Database(f.control);
      try {
        d.exec("BEGIN IMMEDIATE");
        if (mutation === "credential")
          d.query("UPDATE account SET password=? WHERE id='a'").run(alternate);
        else if (mutation === "session")
          d.exec("DELETE FROM session WHERE id='current'");
        else
          d.exec(
            "INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
          );
        d.exec("COMMIT");
      } finally {
        d.close();
      }
      const before = image(f);
      p.child.stdin.write("\n");
      assert.equal((await done(p)).ok, false);
      assert.equal(image(f), before);
      note(`${point}-${mutation}-unlocked-KDF-recheck-refuses-stale-authority`);
    }
  const concurrent = fresh("concurrent", "wal"),
    a = start(
      concurrent,
      request({ revokeOtherSessions: false }),
      "password:unlocked",
    );
  await marker(a, concurrent);
  const b = change(concurrent, {
    newPassword: "Concurrent replacement 789!",
    revokeOtherSessions: false,
  });
  assert.equal(b.ok, true);
  const before = image(concurrent);
  a.child.stdin.write("\n");
  assert.equal((await done(a)).ok, false);
  assert.equal(image(concurrent), before);
  note("concurrent-password-change-only-current-snapshot-can-commit");
  const locked = fresh("locked", "wal"),
    p = start(locked, request({ revokeOtherSessions: true }), "before-commit");
  await marker(p, locked);
  const blocked = change(locked, { revokeOtherSessions: false });
  assert.equal(blocked.ok, false);
  p.child.stdin.write("\n");
  assert.equal((await done(p)).ok, true);
  note("held-writer-excludes-second-password-change");
  const killed = fresh("killed", "wal"),
    prior = image(killed),
    k = start(killed, request({ revokeOtherSessions: true }), "before-commit");
  await marker(k, killed);
  k.child.kill();
  await k.child.exited;
  clearTimeout(k.timer);
  assert.equal(image(killed), prior);
  note(
    "killed-before-commit-rolls-back-credential-delete-newsession-resetlinks-and-audit",
  );
  for (const rotate of [false, true]) {
    const f = fresh(`unknown-${rotate}`),
      blocker = new Database(f.control);
    try {
      blocker.exec("BEGIN;SELECT * FROM session;");
      const before = image(f),
        r = change(f, { revokeOtherSessions: rotate });
      assert.equal(r.ok, false);
      assert.equal(r.stage, "OutcomeUnknown");
      assert.equal(r.reusable, false);
      assert(!("cookie" in r));
      assert(!("user" in r));
      assert.equal(image(f), before);
      note(
        `DELETE-${rotate}-actual-commit-busy-withholds-user-cookie-and-confirmation`,
      );
    } finally {
      blocker.exec("ROLLBACK");
      blocker.close();
    }
  }
  const unicode = fresh("unicode");
  const unicodePassword = "😀".repeat(6);
  sensitive.push(unicodePassword);
  assert.equal(change(unicode, { newPassword: unicodePassword }).ok, true);
  assert(
    await verifyPassword({
      hash: q(unicode, "SELECT password FROM account WHERE id='a'")[0].password,
      password: unicodePassword,
    }),
  );
  note("original-UTF16-twelve-units-and-source-Unicode-KDF-compatible-change");
  const interop = fresh("source-interop"),
    wall = Date.now();
  const live = new Database(interop.control);
  live
    .query("UPDATE session SET createdAt=?,updatedAt=?,expiresAt=?")
    .run(
      new Date(wall - 1000).toISOString(),
      new Date(wall - 1000).toISOString(),
      new Date(wall + 3600000).toISOString(),
    );
  live.close();
  const native = change(interop, { now: wall, revokeOtherSessions: true });
  assert.equal(native.ok, true);
  sensitive.push(native.cookie);
  const sourceDb = new Database(interop.control),
    auth = createBetterAuthRuntime(sourceDb, c.opts);
  const next = "Source changed private password 789!";
  sensitive.push(next);
  try {
    const session = await auth.handler(
      new Request("https://example.invalid/api/auth/get-session", {
        headers: { cookie: native.cookie },
      }),
    );
    assert.equal(session.status, 200);
    const view = await session.json();
    assert.equal(view.user.id, "u");
    sensitive.push(view.session.token);
    note(
      "actual-source-session-reader-accepts-native-password-rotation-cookie",
    );
    const result = await auth.handler(
      new Request("https://example.invalid/api/auth/change-password", {
        method: "POST",
        headers: {
          cookie: native.cookie,
          origin: "https://example.invalid",
          "content-type": "application/json",
        },
        body: JSON.stringify({
          currentPassword: replacement,
          newPassword: next,
          revokeOtherSessions: false,
        }),
      }),
    );
    assert.equal(result.status, 200);
    const value = await result.json();
    assert.equal(value.user.id, "u");
    assert.equal(value.token, null);
  } finally {
    sourceDb.close();
  }
  const again = change(interop, {
    now: Date.now(),
    password: next,
    cookie: native.cookie,
    newPassword: replacement,
    revokeOtherSessions: false,
  });
  assert.equal(again.ok, true);
  note(
    "native-current-credential-fence-accepts-actual-source-HTTP-password-change",
  );
  return sensitive;
}
