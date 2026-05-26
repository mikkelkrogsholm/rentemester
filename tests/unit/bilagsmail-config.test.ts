// Tests: src/core/bilagsmail.ts — IMAP config store + mail-alias (#348, #350).
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { mkdirSync } from "node:fs";
import { openDb, migrate } from "../../src/core/db";
import {
  deleteBilagsmailImapConfig,
  getCompanyMailAlias,
  loadBilagsmailImapConfig,
  saveBilagsmailImapConfig,
  setCompanyMailAlias,
} from "../../src/core/bilagsmail";

function freshCompany(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-bilagsmail-${label}-`));
  mkdirSync(join(root, "config"), { recursive: true });
  return root;
}

function freshLedger(label: string) {
  const root = freshCompany(label);
  const db = openDb(join(root, "ledger.sqlite"));
  migrate(db);
  // Seed minimal company-row så setCompanyMailAlias kan opdatere id=1.
  db.run(
    "INSERT INTO companies (id, name) VALUES (1, 'Acme ApS') ON CONFLICT(id) DO NOTHING",
  );
  return { root, db };
}

describe("#348 — IMAP config storage in config/imap.json", () => {
  test("save → load roundtrip with defaults applied", () => {
    const root = freshCompany("imap-save");
    try {
      saveBilagsmailImapConfig(root, {
        host: "imap.example.com",
        port: 993,
        username: "rentemester@example.com",
        password: "supersecret",
      });
      const loaded = loadBilagsmailImapConfig(root);
      expect(loaded).not.toBeNull();
      expect(loaded!.host).toBe("imap.example.com");
      expect(loaded!.port).toBe(993);
      expect(loaded!.username).toBe("rentemester@example.com");
      expect(loaded!.password).toBe("supersecret");
      // Defaults: secure=true, mailbox=INBOX
      expect(loaded!.secure).toBe(true);
      expect(loaded!.mailbox).toBe("INBOX");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("config file is written with 0600 permissions", () => {
    const root = freshCompany("imap-perms");
    try {
      const { path } = saveBilagsmailImapConfig(root, {
        host: "imap.example.com",
        port: 993,
        username: "user",
        password: "secret",
      });
      const stat = statSync(path);
      // mode includes file-type bits; mask to permission bits.
      expect(stat.mode & 0o777).toBe(0o600);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("loading without a config returns null", () => {
    const root = freshCompany("imap-empty");
    try {
      expect(loadBilagsmailImapConfig(root)).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("delete removes the file", () => {
    const root = freshCompany("imap-delete");
    try {
      saveBilagsmailImapConfig(root, {
        host: "h",
        port: 993,
        username: "u",
        password: "p",
      });
      expect(loadBilagsmailImapConfig(root)).not.toBeNull();
      const deleted = deleteBilagsmailImapConfig(root);
      expect(deleted).toBe(true);
      expect(loadBilagsmailImapConfig(root)).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("save validates required fields", () => {
    const root = freshCompany("imap-validate");
    try {
      expect(() =>
        saveBilagsmailImapConfig(root, {
          host: "",
          port: 993,
          username: "u",
          password: "p",
        }),
      ).toThrow(/host/);
      expect(() =>
        saveBilagsmailImapConfig(root, {
          host: "h",
          port: 0,
          username: "u",
          password: "p",
        }),
      ).toThrow(/port/);
      expect(() =>
        saveBilagsmailImapConfig(root, {
          host: "h",
          port: 993,
          username: "",
          password: "p",
        }),
      ).toThrow(/username/);
      expect(() =>
        saveBilagsmailImapConfig(root, {
          host: "h",
          port: 993,
          username: "u",
          password: "",
        }),
      ).toThrow(/password/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("#350 — per-company mail_alias on companies table", () => {
  test("set + get roundtrips a valid alias", () => {
    const { root, db } = freshLedger("alias-roundtrip");
    try {
      setCompanyMailAlias(db, "acme-aps");
      expect(getCompanyMailAlias(db)).toBe("acme-aps");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("alias normalizes to lowercase", () => {
    const { root, db } = freshLedger("alias-lc");
    try {
      setCompanyMailAlias(db, "Acme-ApS");
      expect(getCompanyMailAlias(db)).toBe("acme-aps");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("setCompanyMailAlias(null) clears the alias", () => {
    const { root, db } = freshLedger("alias-clear");
    try {
      setCompanyMailAlias(db, "acme");
      setCompanyMailAlias(db, null);
      expect(getCompanyMailAlias(db)).toBeNull();
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("invalid alias is refused", () => {
    const { root, db } = freshLedger("alias-bad");
    try {
      expect(() => setCompanyMailAlias(db, "ab")).toThrow();
      expect(() => setCompanyMailAlias(db, "-leading")).toThrow();
      expect(() => setCompanyMailAlias(db, "uppercase!")).toThrow();
      expect(() => setCompanyMailAlias(db, "spaces in name")).toThrow();
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});
