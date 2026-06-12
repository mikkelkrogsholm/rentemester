import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Database } from "bun:sqlite";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { createSystemBackup, packBackupArchive } from "../../src/core/system-backups";
import { cleanupDir } from "../helpers/cleanup";
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
} from "../../src/core/backup-governance";

function withCompany(fn: (db: Database, companyRoot: string) => void): void {
  const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-gov-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  try {
    migrate(db);
    fn(db, companyRoot);
  } finally {
    db.close();
    cleanupDir(companyRoot);
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
        cleanupDir(folder);
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
        cleanupDir(folder);
      }
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
        const status = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T04:00:00.000Z");
        expect(status.hasCompliantDestination).toBe(true);
        expect(status.latestBackupPlacedOffsite).toBe(true);
        expect(status.latestBackupPlacementCount).toBe(1);
        expect(status.ok).toBe(true);
      } finally {
        cleanupDir(folder);
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
