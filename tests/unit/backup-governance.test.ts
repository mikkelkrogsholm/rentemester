import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Database } from "bun:sqlite";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { createSystemBackup, packBackupArchive } from "../../src/core/system-backups";
import {
  addBackupDestination,
  confirmBackupPlacement,
  configureBackupLock,
  evaluateBackupLock,
  getBackupGovernanceStatus,
  isCompliantDestination,
  listBackupDestinations,
  loadBackupLockConfig,
  placeBackupArchive,
  removeBackupDestination,
  verifyRemoteBackupPlacement,
} from "../../src/core/backup-governance";
import type { RemoteBackupProviderAdapter } from "../../src/core/backup-remote-provider";
import { createHash } from "node:crypto";
import { createTar, readTar } from "../../src/core/tar";

function withCompany(fn: (db: Database, companyRoot: string) => void): void {
  const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-gov-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  try {
    migrate(db);
    fn(db, companyRoot);
  } finally {
    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  }
}

async function withCompanyAsync(fn: (db: Database, companyRoot: string) => Promise<void>): Promise<void> {
  const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-gov-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  try {
    migrate(db);
    await fn(db, companyRoot);
  } finally {
    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  }
}

function insertBankActivity(db: Database, date: string, ref: string): void {
  db.run(
    "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
    date,
    date,
    "Activity",
    500,
    ref,
    `batch-${ref}`,
    `hash-${ref}`,
    `tx-${ref}`,
  );
}

const DAY = 24 * 60 * 60 * 1000;

const COMPLIANT_DEST = {
  label: "EU Backup",
  kind: "dropbox",
  location: "/tmp/does-not-need-to-exist",
  inEeaOrEu: true,
  attestedBy: "user:mikkel",
  regionCountry: "DK",
  nonRelatedParty: true,
  itSecurityMeetsStandards: true,
  at: "2026-05-17T02:00:00.000Z",
};

describe("backup destinations", () => {
  test("adds a §4-compliant destination and persists it", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, COMPLIANT_DEST);
      expect(result.ok).toBe(true);
      expect(isCompliantDestination(result.destination!)).toBe(true);

      const listed = listBackupDestinations(companyRoot);
      expect(listed).toHaveLength(1);
      expect(listed[0]!.regionAttestation.attestedBy).toBe("user:mikkel");
    });
  });

  test("records the resolved actor as createdBy, distinct from the free-text attestedBy", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, {
        ...COMPLIANT_DEST,
        attestedBy: "Mikkel (ejer)",
        actor: "agent:claude-code/1.0",
      });
      expect(result.ok).toBe(true);
      expect(result.destination!.regionAttestation.attestedBy).toBe("Mikkel (ejer)");
      expect(result.destination!.createdBy).toBe("agent:claude-code/1.0");
    });
  });

  test("flags a non-EU destination as not §4-compliant", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, {
        ...COMPLIANT_DEST,
        label: "US Backup",
        inEeaOrEu: false,
      });
      expect(result.ok).toBe(true);
      expect(isCompliantDestination(result.destination!)).toBe(false);
    });
  });

  test("rejects a destination without a human attestation", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, attestedBy: "" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("attestedBy");
    });
  });

  test("accepts proton-drive as a first-class destination kind (#525)", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, {
        ...COMPLIANT_DEST,
        label: "Proton Drive Backup",
        kind: "proton-drive",
        regionCountry: "CH",
        regionNote:
          "Proton-hostet, men serverlokationen attesteres af mennesket — Protons primære infrastruktur ligger i Schweiz, ikke EU/EØS",
      });
      expect(result.ok).toBe(true);
      expect(result.destination!.kind).toBe("proton-drive");
      expect(isCompliantDestination(result.destination!)).toBe(true);
    });
  });

  test("rejects an unknown destination kind", () => {
    withCompany((db, companyRoot) => {
      const result = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "ftp" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("kind");
    });
  });

  test("removes a destination", () => {
    withCompany((db, companyRoot) => {
      const added = addBackupDestination(db, companyRoot, COMPLIANT_DEST);
      const removed = removeBackupDestination(db, companyRoot, added.destination!.id);
      expect(removed.ok).toBe(true);
      expect(listBackupDestinations(companyRoot)).toHaveLength(0);
    });
  });
});

describe("backup placement", () => {
  test("places an archive into a local folder and verifies it by re-read", () => {
    withCompany((db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });

      const folder = mkdtempSync(join(tmpdir(), "rentemester-dest-"));
      try {
        const dest = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, location: folder });
        const placed = placeBackupArchive(db, companyRoot, {
          archivePath: packed.archivePath!,
          destinationId: dest.destination!.id,
          actorKind: "human",
          at: "2026-05-17T03:00:00.000Z",
        });
        expect(placed.ok).toBe(true);
        expect(placed.placement!.verified).toBe(true);
        expect(placed.placement!.verifyMethod).toBe("sha256-reread");
        expect(placed.placement!.backupId).toBe(backup.backupId);

        const stored = listBackupDestinations(companyRoot)[0]!;
        expect(stored.placements).toHaveLength(1);
      } finally {
        rmSync(folder, { recursive: true, force: true });
      }
    });
  });

  test("confirms an agent placement as 'declared' when the location is unreadable", () => {
    withCompany((db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const dest = addBackupDestination(db, companyRoot, {
        ...COMPLIANT_DEST,
        kind: "ssh",
        location: "/no/such/remote/path/at/all",
      });
      const confirmed = confirmBackupPlacement(db, companyRoot, {
        destinationId: dest.destination!.id,
        backupId: backup.backupId!,
        archiveSha256: packed.archiveSha256!,
        actorKind: "agent",
        at: "2026-05-17T03:00:00.000Z",
      });
      expect(confirmed.ok).toBe(true);
      expect(confirmed.placement!.verified).toBe(false);
      expect(confirmed.placement!.verifyMethod).toBe("declared");
      expect(confirmed.placement!.actorKind).toBe("agent");
    });
  });

  test("rejects a confirmed placement whose digest matches nothing in a readable folder", () => {
    withCompany((db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const folder = mkdtempSync(join(tmpdir(), "rentemester-dest-"));
      try {
        // Put the real archive in the folder, then declare a wrong digest.
        placeBackupArchive(db, companyRoot, {
          archivePath: packed.archivePath!,
          destinationId: addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, location: folder }).destination!.id,
          at: "2026-05-17T03:00:00.000Z",
        });
        const dest = listBackupDestinations(companyRoot)[0]!;
        const confirmed = confirmBackupPlacement(db, companyRoot, {
          destinationId: dest.id,
          backupId: backup.backupId!,
          archiveSha256: "f".repeat(64),
          at: "2026-05-17T04:00:00.000Z",
        });
        expect(confirmed.ok).toBe(false);
        expect(confirmed.errors.join(" ")).toContain("could not be confirmed");
      } finally {
        rmSync(folder, { recursive: true, force: true });
      }
    });
  });

  test("persists checked remote evidence without upgrading declared placements", async () => {
    await withCompanyAsync(async (db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:00:00.000Z" });
      expect(backup.ok).toBe(true);
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      expect(packed.ok).toBe(true);
      const content = new Uint8Array(readFileSync(packed.archivePath!));
      const checksum = createHash("sha256").update(content).digest("hex");
      const adapter: RemoteBackupProviderAdapter = {
        provider: "google-drive",
        async getObject() {
          return {
            ok: true,
            metadata: {
              objectId: "file-547",
              name: `${backup.backupId}.tar`,
              parentId: "folder-547",
              sizeBytes: content.byteLength,
              checksumSha256: checksum,
              observedAt: new Date().toISOString(),
            },
          };
        },
        async readObjectContent() {
          return content;
        },
      };
      const destination = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "google-drive", location: "google-drive://folder-547" }).destination!;
      const result = await verifyRemoteBackupPlacement(db, companyRoot, {
        destinationId: destination.id,
        backupId: backup.backupId!,
        remoteObjectId: "file-547",
        at: "2026-05-17T03:00:00.000Z",
      }, adapter);
      expect(result.ok).toBe(true);
      expect(result.placement!.verifyMethod).toBe("remote-provider");
      expect(result.placement!.remoteEvidence?.objectId).toBe("file-547");
      expect(JSON.stringify(result.placement!.remoteEvidence)).not.toContain("remote-archive-547");
    });
  });

  test("rejects a tampered canonical tar even when its checksum sidecar is rewritten", async () => {
    await withCompanyAsync(async (db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:00:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const archive = packed.archivePath!;
      const bytes = Buffer.from(readFileSync(archive));
      bytes[Math.floor(bytes.length / 2)]! ^= 1;
      writeFileSync(archive, bytes);
      const hash = createHash("sha256").update(bytes).digest("hex");
      writeFileSync(`${archive}.sha256`, `${hash}  ${backup.backupId}.tar\n`);
      const destination = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "google-drive", location: "google-drive://folder-547" }).destination!;
      const adapter: RemoteBackupProviderAdapter = { provider: "google-drive", async getObject() { throw new Error("must not reach provider"); }, async readObjectContent() { throw new Error("must not reach provider"); } };
      const result = await verifyRemoteBackupPlacement(db, companyRoot, { destinationId: destination.id, backupId: backup.backupId!, remoteObjectId: "file-547" }, adapter);
      expect(result.ok).toBe(false);
      expect(destination.placements).toHaveLength(0);
      expect((db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'backup_placed'").get() as { n: number }).n).toBe(0);
    });
  });

  test("rejects a duplicate shadow ledger entry even when sidecar is rewritten", async () => {
    await withCompanyAsync(async (db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:00:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const archive = packed.archivePath!;
      const entries = readTar(readFileSync(archive));
      const ledger = entries.find((entry) => entry.path.endsWith("ledger.sqlite"))!;
      const tampered = Buffer.from(ledger.content); tampered[0]! ^= 1;
      const canonical = Buffer.from(readFileSync(archive));
      const duplicate = Buffer.concat([canonical.subarray(0, -1024), createTar([{ path: ledger.path, content: tampered }])]);
      writeFileSync(archive, duplicate);
      writeFileSync(`${archive}.sha256`, `${createHash("sha256").update(duplicate).digest("hex")}  ${backup.backupId}.tar\n`);
      const destination = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "google-drive", location: "google-drive://folder-547" }).destination!;
      const adapter: RemoteBackupProviderAdapter = { provider: "google-drive", async getObject() { throw new Error("must not reach provider"); }, async readObjectContent() { throw new Error("must not reach provider"); } };
      const result = await verifyRemoteBackupPlacement(db, companyRoot, { destinationId: destination.id, backupId: backup.backupId!, remoteObjectId: "file-547" }, adapter);
      expect(result.ok).toBe(false);
      expect(destination.placements).toHaveLength(0);
      expect((db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'backup_placed'").get() as { n: number }).n).toBe(0);
    });
  });

  test("rejects trailing bytes even when the archive sidecar is rewritten", async () => {
    await withCompanyAsync(async (db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:00:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const archive = packed.archivePath!;
      const tainted = Buffer.concat([readFileSync(archive), Buffer.from("unexpected trailing payload")]);
      writeFileSync(archive, tainted);
      writeFileSync(`${archive}.sha256`, `${createHash("sha256").update(tainted).digest("hex")}  ${backup.backupId}.tar\n`);
      const destination = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "google-drive", location: "google-drive://folder-547" }).destination!;
      const adapter: RemoteBackupProviderAdapter = { provider: "google-drive", async getObject() { throw new Error("must not reach provider"); }, async readObjectContent() { throw new Error("must not reach provider"); } };
      const result = await verifyRemoteBackupPlacement(db, companyRoot, { destinationId: destination.id, backupId: backup.backupId!, remoteObjectId: "file-547" }, adapter);
      expect(result.ok).toBe(false);
      expect(destination.placements).toHaveLength(0);
      expect((db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'backup_placed'").get() as { n: number }).n).toBe(0);
    });
  });

  test("rejects future-dated remote verification without placement mutation", async () => {
    await withCompanyAsync(async (db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:00:00.000Z" });
      packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const destination = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, kind: "google-drive", location: "google-drive://folder-547" }).destination!;
      const adapter: RemoteBackupProviderAdapter = { provider: "google-drive", async getObject() { throw new Error("must not reach provider"); }, async readObjectContent() { throw new Error("must not reach provider"); } };
      const result = await verifyRemoteBackupPlacement(db, companyRoot, { destinationId: destination.id, backupId: backup.backupId!, remoteObjectId: "file-547", at: "2099-01-01T00:00:00.000Z" }, adapter);
      expect(result.ok).toBe(false);
      expect(destination.placements).toHaveLength(0);
      expect((db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'backup_placed'").get() as { n: number }).n).toBe(0);
    });
  });
});

describe("backup lock", () => {
  test("never locks when enforcement is opt-out (default)", () => {
    withCompany((db, companyRoot) => {
      expect(loadBackupLockConfig(companyRoot).enforced).toBe(false);
      insertBankActivity(db, "2026-05-01", "a");
      const evaluation = evaluateBackupLock(db, companyRoot, new Date().toISOString());
      expect(evaluation.enforced).toBe(false);
      expect(evaluation.locked).toBe(false);
    });
  });

  test("locks when enforced and a weekly backup is overdue past grace", () => {
    withCompany((db, companyRoot) => {
      const oldBackupAt = new Date(Date.now() - 10 * DAY).toISOString();
      createSystemBackup(db, companyRoot, { createdAt: oldBackupAt });
      insertBankActivity(db, new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10), "late");
      configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0, at: "2026-05-17T00:00:00.000Z" });

      const evaluation = evaluateBackupLock(db, companyRoot, new Date().toISOString());
      expect(evaluation.enforced).toBe(true);
      expect(evaluation.backupDue).toBe(true);
      expect(evaluation.locked).toBe(true);
    });
  });

  test("does not lock while still inside the grace window", () => {
    withCompany((db, companyRoot) => {
      const oldBackupAt = new Date(Date.now() - 10 * DAY).toISOString();
      createSystemBackup(db, companyRoot, { createdAt: oldBackupAt });
      insertBankActivity(db, new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10), "late");
      configureBackupLock(db, companyRoot, { enforced: true, graceDays: 30, at: "2026-05-17T00:00:00.000Z" });

      const evaluation = evaluateBackupLock(db, companyRoot, new Date().toISOString());
      expect(evaluation.backupDue).toBe(true);
      expect(evaluation.locked).toBe(false);
    });
  });

  test("does not lock when nothing has been booked since the last backup", () => {
    withCompany((db, companyRoot) => {
      createSystemBackup(db, companyRoot, { createdAt: new Date(Date.now() - 10 * DAY).toISOString() });
      configureBackupLock(db, companyRoot, { enforced: true, at: "2026-05-17T00:00:00.000Z" });
      const evaluation = evaluateBackupLock(db, companyRoot, new Date().toISOString());
      expect(evaluation.backupDue).toBe(false);
      expect(evaluation.locked).toBe(false);
    });
  });

  test("locks a company that booked but never once backed up", () => {
    withCompany((db, companyRoot) => {
      insertBankActivity(db, new Date(Date.now() - 10 * DAY).toISOString().slice(0, 10), "first");
      configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0, at: "2026-05-17T00:00:00.000Z" });
      const evaluation = evaluateBackupLock(db, companyRoot, new Date().toISOString());
      expect(evaluation.backupDue).toBe(true);
      expect(evaluation.locked).toBe(true);
    });
  });
});

describe("backup governance status", () => {
  test("reports an offsite placement of the latest backup at a compliant destination", () => {
    withCompany((db, companyRoot) => {
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      const folder = mkdtempSync(join(tmpdir(), "rentemester-dest-"));
      try {
        const dest = addBackupDestination(db, companyRoot, { ...COMPLIANT_DEST, location: folder });
        placeBackupArchive(db, companyRoot, {
          archivePath: packed.archivePath!,
          destinationId: dest.destination!.id,
          at: "2026-05-17T03:00:00.000Z",
        });
        const historical = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T02:30:00.000Z");
        expect(historical.latestBackupPlacedOffsite).toBe(false);
        expect(historical.ok).toBe(false);
        const status = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T04:00:00.000Z");
        expect(status.hasCompliantDestination).toBe(true);
        expect(status.latestBackupPlacedOffsite).toBe(true);
        expect(status.latestBackupPlacementCount).toBe(1);
        expect(status.ok).toBe(true);
      } finally {
        rmSync(folder, { recursive: true, force: true });
      }
    });
  });

  test("is not ok when a backup exists but was never placed offsite", () => {
    withCompany((db, companyRoot) => {
      createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const status = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T04:00:00.000Z");
      expect(status.latestBackupPlacedOffsite).toBe(false);
      expect(status.ok).toBe(false);
    });
  });
});
