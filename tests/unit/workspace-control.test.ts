import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { insertWorkspaceAudit, openWorkspaceControlDb, readWorkspaceControlMigrations, validateWorkspaceControlMigrationHistory, WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM, WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME, CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION, insertWorkspaceAuthorizationAudit, insertWorkspaceDocumentAccessAudit, workspaceControlPaths, supportedWorkspaceControlMigrations } from "../../src/core/workspace-control";

function tempWorkspace() {
  return mkdtempSync(join(tmpdir(), "rentemester-workspace-control-"));
}

describe("workspace control database", () => {
  test("creates a private, checksummed control database without touching a company ledger", () => {
    const workspace = tempWorkspace();
    const companyLedger = join(workspace, "synthetic-company", "data", "ledger.sqlite");
    try {
      mkdirSync(join(workspace, "synthetic-company", "data"), { recursive: true });
      const ledger = new Database(companyLedger, { create: true });
      ledger.exec(
        "CREATE TABLE sentinel (id INTEGER PRIMARY KEY, value TEXT NOT NULL); INSERT INTO sentinel VALUES (1, 'unchanged');",
      );
      ledger.close();
      const before = createHash("sha256").update(readFileSync(companyLedger)).digest("hex");

      const db = openWorkspaceControlDb(workspace);
      expect(readWorkspaceControlMigrations(db).map(({ id, name, checksum }) => ({ id, name, checksum }))).toEqual(supportedWorkspaceControlMigrations());;
      expect(
        db.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'workspace_audit'").get(),
      ).not.toBeNull();
      db.close();

      expect(createHash("sha256").update(readFileSync(companyLedger)).digest("hex")).toBe(before);
      const reopenedLedger = new Database(companyLedger);
      expect(reopenedLedger.query("SELECT value FROM sentinel").get()).toEqual({ value: "unchanged" });
      reopenedLedger.close();
      const mode = lstatSync(workspaceControlPaths(workspace).db).mode & 0o777;
      expect(mode).toBe(0o600);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("is idempotent, transactional, and protects workspace audit records", () => {
    const workspace = tempWorkspace();
    try {
      const first = openWorkspaceControlDb(workspace);
      insertWorkspaceAudit(first, {
        eventType: "workspace_control_opened",
        entityType: "workspace",
        entityId: "synthetic",
        createdBy: "agent:test",
        createdByProgram: "unit-test",
      });
      expect(() => first.run("UPDATE workspace_audit SET event_type = 'changed'")).toThrow("append-only");
      expect(() => first.run("DELETE FROM workspace_audit")).toThrow("append-only");
      first.close();

      const second = openWorkspaceControlDb(workspace);
      expect(readWorkspaceControlMigrations(second)).toHaveLength(CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION);
      expect(second.query("SELECT event_type, actor FROM workspace_audit").get()).toEqual({
        event_type: "workspace_control_opened",
        actor: "agent:test via unit-test",
      });
      second.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("serializes immediate writers so a retried workspace audit event is not lost", () => {
    const workspace = tempWorkspace();
    try {
      const first = openWorkspaceControlDb(workspace);
      const second = openWorkspaceControlDb(workspace);
      first.exec("BEGIN IMMEDIATE");
      insertWorkspaceAudit(first, {
        eventType: "first_writer",
        entityType: "workspace",
        createdBy: "agent:first",
        createdByProgram: "unit-test",
      });
      expect(() => insertWorkspaceAudit(second, {
        eventType: "second_writer",
        entityType: "workspace",
        createdBy: "agent:second",
        createdByProgram: "unit-test",
      })).toThrow(/locked|busy/i);
      first.exec("COMMIT");

      insertWorkspaceAudit(second, {
        eventType: "second_writer",
        entityType: "workspace",
        createdBy: "agent:second",
        createdByProgram: "unit-test",
      });
      expect(
        second.query("SELECT event_type FROM workspace_audit ORDER BY id").all(),
      ).toEqual([{ event_type: "first_writer" }, { event_type: "second_writer" }]);
      first.close();
      second.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("rejects a same-name group table whose reviewed constraints were replaced", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      const initialized = openWorkspaceControlDb(workspace);
      initialized.close();

      const tampered = new Database(paths.db);
      tampered.exec(`
        DROP TRIGGER rm_group_manifest_events_no_update;
        DROP TRIGGER rm_group_manifest_events_no_delete;
        DROP TABLE rm_group_manifest_events;
        CREATE TABLE rm_group_manifest_events (
          id INTEGER PRIMARY KEY,
          manifest_hash TEXT NOT NULL UNIQUE,
          previous_hash TEXT,
          canonical_manifest TEXT NOT NULL,
          actor TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TRIGGER rm_group_manifest_events_no_update BEFORE UPDATE ON rm_group_manifest_events
          BEGIN SELECT RAISE(ABORT, 'group manifest events are append-only'); END;
        CREATE TRIGGER rm_group_manifest_events_no_delete BEFORE DELETE ON rm_group_manifest_events
          BEGIN SELECT RAISE(ABORT, 'group manifest events are append-only'); END;
      `);
      tampered.close();

      expect(() => openWorkspaceControlDb(workspace)).toThrow("unsupported constraints");
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("rejects a weakened intercompany event table despite same names and append-only triggers", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      const initialized = openWorkspaceControlDb(workspace);
      initialized.close();
      const tampered = new Database(paths.db);
      tampered.exec(`
        DROP TRIGGER rm_intercompany_mapping_events_no_update;
        DROP TRIGGER rm_intercompany_mapping_events_no_delete;
        DROP TABLE rm_intercompany_mapping_events;
        CREATE TABLE rm_intercompany_mapping_events (
          id INTEGER PRIMARY KEY, mapping_id TEXT NOT NULL, event_type TEXT NOT NULL,
          mapping_hash TEXT NOT NULL, canonical_mapping TEXT NOT NULL, previous_hash TEXT,
          event_hash TEXT NOT NULL, actor TEXT NOT NULL, created_at TEXT NOT NULL
        );
        CREATE TRIGGER rm_intercompany_mapping_events_no_update BEFORE UPDATE ON rm_intercompany_mapping_events BEGIN SELECT RAISE(ABORT, 'intercompany mapping events are append-only'); END;
        CREATE TRIGGER rm_intercompany_mapping_events_no_delete BEFORE DELETE ON rm_intercompany_mapping_events BEGIN SELECT RAISE(ABORT, 'intercompany mapping events are append-only'); END;
      `);
      tampered.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("intercompany mapping table has unsupported constraints");
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("rejects unknown, newer, missing, and tampered migration history", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      const db = openWorkspaceControlDb(workspace);
      db.close();

      const tampered = new Database(paths.db);
      tampered.exec(
        "DROP TRIGGER workspace_schema_migrations_no_update; DROP TRIGGER workspace_schema_migrations_no_delete;",
      );
      tampered.run("UPDATE workspace_schema_migrations SET checksum = 'tampered' WHERE id = 1");
      tampered.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("checksum mismatch");

      expect(() => validateWorkspaceControlMigrationHistory([
        {
          id: 1,
          name: WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME,
          checksum: null,
        },
      ])).toThrow("missing checksum");

      rmSync(paths.db, { force: true });
      const missing = openWorkspaceControlDb(workspace);
      missing.close();
      const missingHistory = new Database(paths.db);
      missingHistory.exec(
        "DROP TRIGGER workspace_schema_migrations_no_delete; DELETE FROM workspace_schema_migrations;",
      );
      missingHistory.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("without its baseline migration record");

      rmSync(paths.db, { force: true });
      const nonContiguous = openWorkspaceControlDb(workspace);
      nonContiguous.close();
      const nonContiguousHistory = new Database(paths.db);
      nonContiguousHistory.run(
        "INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version) VALUES (0, 'invalid-prefix', 'invalid-checksum', '0.1.0')",
      );
      nonContiguousHistory.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("complete append-only prefix");

      rmSync(paths.db, { force: true });
      const future = new Database(paths.db);
      future.exec(`
        CREATE TABLE workspace_schema_migrations (
          id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, checksum TEXT NOT NULL,
          applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, applied_by_version TEXT NOT NULL,
          applied_by_commit TEXT
        );
        INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version)
        VALUES (${CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION + 1}, 'future', 'future-checksum', '0.2.0');
      `);
      future.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("newer than supported");

      rmSync(paths.db, { force: true });
      const unknown = new Database(paths.db);
      unknown.exec("CREATE TABLE unexpected_control_state (id INTEGER PRIMARY KEY);");
      unknown.close();
      expect(() => openWorkspaceControlDb(workspace)).toThrow("refusing to adopt unknown state");
      const afterFailedOpen = new Database(paths.db);
      expect(
        afterFailedOpen.query(
          "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('workspace_schema_migrations', 'workspace_audit')",
        ).all(),
      ).toEqual([]);
      afterFailedOpen.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("binds the migration checksum to its immutable artifact", () => {
    const artifact = join(
      import.meta.dir,
      "..",
      "..",
      "src",
      "core",
      "workspace-migrations",
      "0001-workspace-control-baseline.json",
    );
    expect(WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM).toBe(
      createHash("sha256").update(readFileSync(artifact)).digest("hex"),
    );
    const betterAuthArtifact = join(
      import.meta.dir,
      "..",
      "..",
      "src",
      "core",
      "workspace-migrations",
      "0002-better-auth-foundation.json",
    );
    expect(supportedWorkspaceControlMigrations()[1]!.checksum).toBe(
      createHash("sha256").update(readFileSync(betterAuthArtifact)).digest("hex"),
    );
    const accessArtifact = join(
      import.meta.dir,
      "..",
      "..",
      "src",
      "core",
      "workspace-migrations",
      "0003-workspace-access-events.json",
    );
    expect(supportedWorkspaceControlMigrations()[2]!.checksum).toBe(
      createHash("sha256").update(readFileSync(accessArtifact)).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[3]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0004-workspace-bootstrap-saga-v4.json",
      ))).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[4]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0005-session-assurance-v5.json",
      ))).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[5]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0006-auth-audit-v6.json",
      ))).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[7]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0008-document-access-audit-v8.json",
      ))).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[11]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0012-auth-recovery-telemetry-v12.json",
      ))).digest("hex"),
    );
    expect(supportedWorkspaceControlMigrations()[12]!.checksum).toBe(
      createHash("sha256").update(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations", "0013-authorization-denial-audit-v13.json",
      ))).digest("hex"),
    );
  });

  test("v6 records only authoritative auth state transitions append-only", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      db.run(`INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES ('u','U','u@example.test',0,'2026-01-01','2026-01-01',0)`);
      db.run(`UPDATE "user" SET emailVerified = 1 WHERE id = 'u'`);
      db.run(`INSERT INTO "account" (id,issuer,accountId,providerId,userId,password,createdAt,updatedAt) VALUES ('a','credential','u','credential','u','old','2026-01-01','2026-01-01')`);
      db.run(`UPDATE "account" SET password = 'new' WHERE id = 'a'`);
      db.run(`INSERT INTO "session" (id,expiresAt,token,createdAt,updatedAt,userId) VALUES ('s','2027-01-01','opaque','2026-01-01','2026-01-01','u')`);
      db.run(`DELETE FROM "session" WHERE id = 's'`);
      expect(db.query("SELECT state_transition FROM rm_workspace_auth_state_events ORDER BY id").all()).toEqual([{ state_transition: "email_verified" }, { state_transition: "credential_updated" }, { state_transition: "session_created" }, { state_transition: "session_deleted" }]);
      expect(() => db.run("UPDATE rm_workspace_auth_state_events SET state_transition = 'mfa_enabled'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_auth_state_events")).toThrow("append-only");
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("v8 records bounded secret-free document access evidence append-only", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      insertWorkspaceDocumentAccessAudit(db, {
        actor: "user:test-user",
        companySlug: "synthetic-company",
        resourceType: "document_file",
        resourceId: 7,
        outcome: "served",
        reasonCode: "authorized",
        requestId: "request-42",
      });
      expect(db.query(`SELECT actor, company_slug, resource_type, resource_id, outcome, reason_code, request_id
                       FROM rm_workspace_document_access_events`).get()).toEqual({
        actor: "user:test-user", company_slug: "synthetic-company", resource_type: "document_file",
        resource_id: 7, outcome: "served", reason_code: "authorized", request_id: "request-42",
      });
      expect(() => db.run("UPDATE rm_workspace_document_access_events SET outcome = 'denied'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_document_access_events")).toThrow("append-only");
      expect(() => db.run(`INSERT INTO rm_workspace_document_access_events
        (actor, company_slug, resource_type, resource_id, outcome, reason_code, request_id)
        VALUES ('user:reader', 'synthetic-company', 'document_file', 7, 'served', 'authorized', 'ok\nsecret')`)).toThrow();
      expect(() => insertWorkspaceDocumentAccessAudit(db, {
        actor: "user:test-user", companySlug: "synthetic-company", resourceType: "document_file",
        resourceId: 7, outcome: "served", reasonCode: "authorization_denied", requestId: "secret@example.test",
      })).toThrow("invalid document access audit outcome");
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("v12 preserves auth telemetry and permits only secret-free recovery endpoint evidence", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      db.query(`INSERT INTO rm_workspace_auth_telemetry_events
        (endpoint, outcome, identity_hash, identity_key_version)
        VALUES ('two-factor-verify-backup-code', 'accepted', NULL, 1)`).run();
      expect(db.query("SELECT endpoint, outcome, identity_hash FROM rm_workspace_auth_telemetry_events").all()).toEqual([
        { endpoint: "two-factor-verify-backup-code", outcome: "accepted", identity_hash: null },
      ]);
      expect(() => db.run("UPDATE rm_workspace_auth_telemetry_events SET outcome = 'rejected'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_auth_telemetry_events")).toThrow("append-only");
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("v13 records bounded authorization denials without raw request data", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      insertWorkspaceAuthorizationAudit(db, {
        actor: "user:synthetic-reader",
        method: "POST",
        routeTemplate: "/api/companies/:slug/periods/close",
        permission: "company.period.manage",
        companySlug: "other-company",
        requestId: "request-13",
      });
      expect(db.query(`SELECT actor, method, route_template, permission, company_slug,
        outcome, reason_code, request_id FROM rm_workspace_authorization_events`).get()).toEqual({
        actor: "user:synthetic-reader", method: "POST",
        route_template: "/api/companies/:slug/periods/close",
        permission: "company.period.manage", company_slug: "other-company",
        outcome: "denied", reason_code: "authorization_denied", request_id: "request-13",
      });
      expect(() => db.run("UPDATE rm_workspace_authorization_events SET outcome = 'denied'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_authorization_events")).toThrow("append-only");
      expect(() => insertWorkspaceAuthorizationAudit(db, {
        actor: "user:synthetic-reader", method: "GET",
        routeTemplate: "/api/companies/:slug/dashboard?secret=leak",
        permission: "company.read", companySlug: "other-company",
      })).toThrow("invalid authorization audit route");
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("v14 provides append-only, replay-protected invitation evidence", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      expect(db.query(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'rm_workspace_invitation_events_token_uidx'",
      ).get()).not.toBeNull();
      db.query(`INSERT INTO rm_workspace_invitation_events
        (invitation_id,event_type,token_hash,token_key_version,canonical_email,email_hash,
         workspace_role,company_slug,company_role,expires_at,actor)
        VALUES (?,?,?,?,?,?,?,?,?,?,?)`).run(
        "00000000-0000-4000-8000-000000000014", "issued", "a".repeat(64), 1,
        "invitee@example.test", "b".repeat(64), "member", "synthetic-company", "reader",
        "2026-09-01T00:00:00.000Z", "user:synthetic-owner",
      );
      expect(() => db.run("UPDATE rm_workspace_invitation_events SET event_type='cancelled'"))
        .toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_invitation_events")).toThrow("append-only");
      expect(() => db.query(`INSERT INTO rm_workspace_invitation_events
        (invitation_id,event_type,token_hash,token_key_version,canonical_email,email_hash,
         workspace_role,company_slug,company_role,expires_at,actor)
        VALUES (?,?,?,?,?,?,?,?,?,?,?)`).run(
        "00000000-0000-4000-8000-000000000015", "issued", "a".repeat(64), 1,
        "other@example.test", "c".repeat(64), "member", "synthetic-company", "reader",
        "2026-09-01T00:00:00.000Z", "user:synthetic-owner",
      )).toThrow(/UNIQUE/i);
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("upgrades a checksummed v1 control database to the reviewed Better Auth v2 schema", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      mkdirSync(paths.directory, { recursive: true });
      const v1 = new Database(paths.db);
      const baseline = JSON.parse(readFileSync(join(
        import.meta.dir, "..", "..", "src", "core", "workspace-migrations",
        "0001-workspace-control-baseline.json",
      ), "utf8")) as { sql: string };
      v1.exec(baseline.sql);
      v1.query(
        "INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version) VALUES (1, ?, ?, 'test')",
      ).run(WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME, WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM);
      v1.close();

      const upgraded = openWorkspaceControlDb(workspace);
      expect(readWorkspaceControlMigrations(upgraded)).toHaveLength(CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION);
      const expectedColumns: Record<string, string[]> = {
        user: ["id", "name", "email", "emailVerified", "image", "createdAt", "updatedAt", "twoFactorEnabled"],
        session: ["id", "expiresAt", "token", "createdAt", "updatedAt", "ipAddress", "userAgent", "userId"],
        account: ["id", "issuer", "accountId", "providerId", "userId", "accessToken", "refreshToken", "idToken", "accessTokenExpiresAt", "refreshTokenExpiresAt", "scope", "password", "createdAt", "updatedAt"],
        verification: ["id", "identifier", "value", "expiresAt", "createdAt", "updatedAt"],
        twoFactor: ["id", "secret", "backupCodes", "userId", "verified", "failedVerificationCount", "lockedUntil"],
        rateLimit: ["id", "key", "count", "lastRequest"],
      };
      for (const [table, columns] of Object.entries(expectedColumns)) {
        expect((upgraded.query(`PRAGMA table_info('${table}')`).all() as { name: string }[]).map((row) => row.name)).toEqual(columns);
      }
      expect((upgraded.query("PRAGMA table_info('twoFactor')").all() as { name: string }[]).map((row) => row.name)).toEqual([
        "id", "secret", "backupCodes", "userId", "verified", "failedVerificationCount", "lockedUntil",
      ]);
      expect((upgraded.query("PRAGMA table_info('rateLimit')").all() as { name: string }[]).map((row) => row.name)).toEqual([
        "id", "key", "count", "lastRequest",
      ]);
      upgraded.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("upgrades a checksummed v2 control database to the append-only access v3 schema", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      mkdirSync(paths.directory, { recursive: true });
      const v2 = new Database(paths.db);
      for (const filename of [
        "0001-workspace-control-baseline.json",
        "0002-better-auth-foundation.json",
      ]) {
        const artifact = JSON.parse(readFileSync(join(
          import.meta.dir, "..", "..", "src", "core", "workspace-migrations", filename,
        ), "utf8")) as { sql: string };
        v2.exec(artifact.sql);
      }
      v2.query(
        "INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version) VALUES (1, ?, ?, 'test')",
      ).run(WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME, WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM);
      v2.query(
        "INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version) VALUES (2, ?, ?, 'test')",
      ).run(supportedWorkspaceControlMigrations()[1]!.name, supportedWorkspaceControlMigrations()[1]!.checksum);
      v2.close();

      const upgraded = openWorkspaceControlDb(workspace);
      expect(readWorkspaceControlMigrations(upgraded)).toHaveLength(CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION);
      for (const table of [
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
        "rm_workspace_security_events",
      ]) {
        expect(upgraded.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?").get(table)).not.toBeNull();
      }
      upgraded.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("upgrades v4 with a fail-closed MFA session boundary", () => {
    const workspace = tempWorkspace();
    const paths = workspaceControlPaths(workspace);
    try {
      mkdirSync(paths.directory, { recursive: true });
      const v4 = new Database(paths.db);
      for (const filename of [
        "0001-workspace-control-baseline.json",
        "0002-better-auth-foundation.json",
        "0003-workspace-access-events.json",
        "0004-workspace-bootstrap-saga-v4.json",
      ]) {
        const artifact = JSON.parse(readFileSync(join(
          import.meta.dir, "..", "..", "src", "core", "workspace-migrations", filename,
        ), "utf8")) as { sql: string };
        v4.exec(artifact.sql);
      }
      const migrationRows = [
        [1, WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME, WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM],
        [2, supportedWorkspaceControlMigrations()[1]!.name, supportedWorkspaceControlMigrations()[1]!.checksum],
        [3, supportedWorkspaceControlMigrations()[2]!.name, supportedWorkspaceControlMigrations()[2]!.checksum],
        [4, supportedWorkspaceControlMigrations()[3]!.name, supportedWorkspaceControlMigrations()[3]!.checksum],
      ] as const;
      for (const [id, name, checksum] of migrationRows) {
        v4.query(
          "INSERT INTO workspace_schema_migrations (id, name, checksum, applied_by_version) VALUES (?, ?, ?, 'test')",
        ).run(id, name, checksum);
      }
      v4.query(
        `INSERT INTO "user" (id, name, email, emailVerified, createdAt, updatedAt, twoFactorEnabled)
         VALUES ('mfa-user', 'MFA User', 'mfa@example.test', 1, '2026-01-01', '2026-01-01', 1)`,
      ).run();
      v4.query(
        `INSERT INTO "session" (id, expiresAt, token, createdAt, updatedAt, userId)
         VALUES ('legacy-session', '2027-01-01', 'opaque', '2026-01-01', '2026-01-01', 'mfa-user')`,
      ).run();
      v4.close();

      const upgraded = openWorkspaceControlDb(workspace);
      expect(readWorkspaceControlMigrations(upgraded)).toHaveLength(CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION);
      expect(upgraded.query('SELECT COUNT(*) AS count FROM "session"').get()).toEqual({ count: 0 });
      expect(upgraded.query(
        "SELECT user_id, event_type, actor FROM rm_workspace_mfa_events",
      ).get()).toEqual({ user_id: "mfa-user", event_type: "mfa_enabled", actor: "system:better-auth" });
      expect(() => upgraded.run("UPDATE rm_workspace_mfa_events SET event_type = 'mfa_enabled'")).toThrow("append-only");
      expect(() => upgraded.run("DELETE FROM rm_workspace_mfa_events")).toThrow("append-only");
      upgraded.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

});
