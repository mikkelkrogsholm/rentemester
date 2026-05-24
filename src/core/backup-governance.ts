// Backup governance: off-site destinations, placement evidence and the
// opt-in bookkeeping lock.
//
// Background — what the law actually requires (BEK nr. 205 af 04/03/2024,
// "digitale bogføringssystemer der ikke er registreret", § 4):
//   stk. 1  the company must take a FULL backup of all booked transactions
//           and vouchers AT LEAST WEEKLY (unless nothing was booked since
//           the last backup);
//   stk. 2  that backup must be kept with a NON-RELATED third party that is
//           presumed to meet recognised IT-security standards, ON A SERVER
//           IN AN EU/EEA COUNTRY.
// See also bogføringsloven (LOV 700/2022) § 12 (5-year safe retention) and
// § 15, stk. 1, nr. 2 (recognised IT-security + automatic backup).
//
// Rentemester cannot, by itself, know in which country a Dropbox or Drive
// folder is stored — only a human can attest that. So a destination carries
// an explicit, human-signed attestation; the agent transfers files, the
// human confirms the legal frame once. Nothing here talks to a cloud API:
// the agent uses its own tooling, we record and verify what landed.

import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { insertAuditLog } from "./actor";
import { writeFileAtomic } from "./atomic-file";
import { getBackupComplianceStatus } from "./system-backups";
import type { BackupComplianceStatus } from "./system-backups";
import { readTar } from "./tar";

const BACKUP_RULE_ID = "DK-BOOKKEEPING-BACKUP-001";
// § 4, stk. 2 of BEK 205/2024 — backup must live with a non-related party on
// an EU/EEA server. Enforced as a human-signed attestation in
// isCompliantDestination(), not as a machine-checkable predicate.
const BACKUP_DEST_RULE_ID = "DK-BOOKKEEPING-BACKUP-DEST-001";
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

export const BACKUP_DESTINATION_KINDS = [
  "local-folder",
  "dropbox",
  "google-drive",
  "ssh",
  "other",
] as const;
export type BackupDestinationKind = (typeof BACKUP_DESTINATION_KINDS)[number];

export type RegionAttestation = {
  // §4 stk.2: the server must sit in an EU/EEA country. Only a human can
  // truthfully assert this — Rentemester records the assertion.
  inEeaOrEu: boolean;
  country: string | null;
  attestedBy: string;
  attestedAt: string;
  note: string | null;
};

export type ItSecurityAttestation = {
  // §4 stk.2: the third party must be presumed to meet recognised
  // IT-security standards.
  meetsRecognisedStandards: boolean;
  attestedBy: string;
  attestedAt: string;
  note: string | null;
};

export type BackupPlacementActorKind = "human" | "agent";

export type BackupPlacement = {
  backupId: string;
  archiveSha256: string;
  archiveSizeBytes: number | null;
  placedAt: string;
  actor: string;
  actorKind: BackupPlacementActorKind;
  // verified = the archive was re-read at the destination and its sha256
  // matched. "declared" placements (agent pushed via a channel we cannot
  // read back) are recorded but not proven.
  verified: boolean;
  verifyMethod: "sha256-reread" | "declared";
  note: string | null;
};

export type BackupDestination = {
  id: string;
  label: string;
  kind: BackupDestinationKind;
  location: string;
  // §4 stk.2: "ikke nærtstående part".
  nonRelatedParty: boolean;
  regionAttestation: RegionAttestation;
  itSecurityAttestation: ItSecurityAttestation | null;
  createdAt: string;
  createdBy: string;
  placements: BackupPlacement[];
};

export type BackupDestinationsFile = {
  version: "backup-destinations-v1";
  destinations: BackupDestination[];
};

export type BackupLockConfig = {
  version: "backup-lock-v1";
  // Opt-in. When false, the lock never engages — only warnings are shown.
  enforced: boolean;
  // Extra days of leniency ON TOP of the 7-day legal window before the lock
  // actually blocks bookkeeping. 0 = lock exactly when the weekly window
  // lapses.
  graceDays: number;
  updatedAt: string | null;
  updatedBy: string | null;
};

function backupDestinationsPath(companyRoot: string): string {
  return join(companyPaths(companyRoot).config, "backup-destinations.json");
}

function backupLockPath(companyRoot: string): string {
  return join(companyPaths(companyRoot).config, "backup-lock.json");
}

export function loadBackupDestinations(companyRoot: string): BackupDestinationsFile {
  const path = backupDestinationsPath(companyRoot);
  if (!existsSync(path)) return { version: "backup-destinations-v1", destinations: [] };
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`backup-destinations.json is corrupt: ${String(error)}`);
  }
  const file = parsed as BackupDestinationsFile;
  if (!file || file.version !== "backup-destinations-v1" || !Array.isArray(file.destinations)) {
    throw new Error("backup-destinations.json has an unrecognised shape");
  }
  return file;
}

function saveBackupDestinations(companyRoot: string, file: BackupDestinationsFile): void {
  mkdirSync(companyPaths(companyRoot).config, { recursive: true });
  writeFileAtomic(backupDestinationsPath(companyRoot), `${JSON.stringify(file, null, 2)}\n`);
}

export function loadBackupLockConfig(companyRoot: string): BackupLockConfig {
  const path = backupLockPath(companyRoot);
  if (!existsSync(path)) {
    return { version: "backup-lock-v1", enforced: false, graceDays: 0, updatedAt: null, updatedBy: null };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`backup-lock.json is corrupt: ${String(error)}`);
  }
  const file = parsed as Partial<BackupLockConfig>;
  if (!file || file.version !== "backup-lock-v1") {
    throw new Error("backup-lock.json has an unrecognised shape");
  }
  return {
    version: "backup-lock-v1",
    enforced: file.enforced === true,
    graceDays: Number.isFinite(file.graceDays) && (file.graceDays as number) >= 0
      ? Math.trunc(file.graceDays as number)
      : 0,
    updatedAt: typeof file.updatedAt === "string" ? file.updatedAt : null,
    updatedBy: typeof file.updatedBy === "string" ? file.updatedBy : null,
  };
}

function resolveAt(value?: string): string | null {
  const at = value ?? new Date().toISOString();
  if (Number.isNaN(Date.parse(at))) return null;
  return new Date(at).toISOString();
}

function trimOrNull(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function destinationId(label: string, location: string, createdAt: string): string {
  const digest = createHash("sha256").update(`${label}|${location}|${createdAt}`).digest("hex");
  return `dest-${digest.slice(0, 12)}`;
}

// §4 stk.2 is satisfied by a destination only when all three conditions hold:
// EU/EEA server, non-related party, presumed recognised IT-security.
export function isCompliantDestination(destination: BackupDestination): boolean {
  return (
    destination.regionAttestation.inEeaOrEu === true &&
    destination.nonRelatedParty === true &&
    destination.itSecurityAttestation?.meetsRecognisedStandards === true
  );
}

export type AddBackupDestinationInput = {
  label: string;
  kind: string;
  location: string;
  inEeaOrEu: boolean;
  // Free-text human name behind the attestation (what the owner typed).
  attestedBy: string;
  // The resolved canonical actor id that actually ran the command
  // (user:… or agent:…). Recorded distinctly so the audit trail is honest
  // about whether a human or an agent created the attestation.
  actor?: string;
  regionCountry?: string;
  regionNote?: string;
  nonRelatedParty?: boolean;
  itSecurityMeetsStandards?: boolean;
  itSecurityNote?: string;
  at?: string;
};

export type AddBackupDestinationResult = {
  ok: boolean;
  destination?: BackupDestination;
  appliedRules: string[];
  errors: string[];
};

export function addBackupDestination(
  db: Database,
  companyRoot: string,
  input: AddBackupDestinationInput,
): AddBackupDestinationResult {
  const errors: string[] = [];
  const label = trimOrNull(input.label);
  const location = trimOrNull(input.location);
  const attestedBy = trimOrNull(input.attestedBy);
  if (!label) errors.push("label is required");
  if (!location) errors.push("location is required");
  if (!attestedBy) errors.push("attestedBy is required — a human must attest where this backup is stored");
  if (typeof input.inEeaOrEu !== "boolean") errors.push("inEeaOrEu must be explicitly true or false");
  const kind = trimOrNull(input.kind) as BackupDestinationKind | null;
  if (!kind || !BACKUP_DESTINATION_KINDS.includes(kind)) {
    errors.push(`kind must be one of: ${BACKUP_DESTINATION_KINDS.join(", ")}`);
  }
  const createdAt = resolveAt(input.at);
  if (!createdAt) errors.push("at must be a valid ISO-8601 datetime when provided");
  if (errors.length > 0) return { ok: false, appliedRules: [BACKUP_RULE_ID], errors };

  const file = loadBackupDestinations(companyRoot);
  const id = destinationId(label!, location!, createdAt!);
  if (file.destinations.some((d) => d.id === id)) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      errors: [`a destination with the same label, location and timestamp already exists: ${id}`],
    };
  }

  const itSecurity: ItSecurityAttestation | null =
    typeof input.itSecurityMeetsStandards === "boolean"
      ? {
          meetsRecognisedStandards: input.itSecurityMeetsStandards,
          attestedBy: attestedBy!,
          attestedAt: createdAt!,
          note: trimOrNull(input.itSecurityNote),
        }
      : null;

  const destination: BackupDestination = {
    id,
    label: label!,
    kind: kind!,
    location: location!,
    nonRelatedParty: input.nonRelatedParty !== false,
    regionAttestation: {
      inEeaOrEu: input.inEeaOrEu,
      country: trimOrNull(input.regionCountry),
      attestedBy: attestedBy!,
      attestedAt: createdAt!,
      note: trimOrNull(input.regionNote),
    },
    itSecurityAttestation: itSecurity,
    createdAt: createdAt!,
    createdBy: trimOrNull(input.actor) ?? attestedBy!,
    placements: [],
  };

  file.destinations.push(destination);
  file.destinations.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  saveBackupDestinations(companyRoot, file);

  insertAuditLog(db, {
    eventType: "backup_destination_added",
    entityType: "company",
    entityId: 1,
    message:
      `Added backup destination '${destination.label}' (${destination.kind}); ` +
      `EU/EØS=${destination.regionAttestation.inEeaOrEu}, non-related=${destination.nonRelatedParty}, ` +
      `attested §4-compliant=${isCompliantDestination(destination)} (human attestation, not independently verified); ` +
      `attestedBy='${attestedBy}', createdBy=${destination.createdBy}`,
  });

  // The weekly-backup duty (DK-BOOKKEEPING-BACKUP-001) and the
  // destination-attestation duty (DK-BOOKKEEPING-BACKUP-DEST-001 — BEK
  // 205/2024 § 4, stk. 2) both apply when a destination is registered.
  return { ok: true, destination, appliedRules: [BACKUP_RULE_ID, BACKUP_DEST_RULE_ID], errors: [] };
}

export function listBackupDestinations(companyRoot: string): BackupDestination[] {
  return loadBackupDestinations(companyRoot).destinations;
}

export function getBackupDestination(companyRoot: string, id: string): BackupDestination | undefined {
  return loadBackupDestinations(companyRoot).destinations.find((d) => d.id === id);
}

export type RemoveBackupDestinationResult = {
  ok: boolean;
  appliedRules: string[];
  errors: string[];
};

export function removeBackupDestination(
  db: Database,
  companyRoot: string,
  id: string,
): RemoveBackupDestinationResult {
  const file = loadBackupDestinations(companyRoot);
  const destination = file.destinations.find((d) => d.id === id);
  if (!destination) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`no backup destination with id: ${id}`] };
  }
  file.destinations = file.destinations.filter((d) => d.id !== id);
  saveBackupDestinations(companyRoot, file);
  insertAuditLog(db, {
    eventType: "backup_destination_removed",
    entityType: "company",
    entityId: 1,
    message: `Removed backup destination '${destination.label}' (${id})`,
  });
  return { ok: true, appliedRules: [BACKUP_RULE_ID], errors: [] };
}

function sha256File(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

// Reads the embedded manifest.json out of a .tar backup archive to recover
// the authoritative backupId — never trust the filename alone.
function backupIdFromArchive(archivePath: string): string | null {
  let entries: ReturnType<typeof readTar>;
  try {
    entries = readTar(readFileSync(archivePath));
  } catch {
    return null;
  }
  const manifest = entries.find((e) => e.path === "manifest.json");
  if (!manifest) return null;
  try {
    const parsed = JSON.parse(Buffer.from(manifest.content).toString("utf8")) as { backupId?: string };
    return typeof parsed.backupId === "string" ? parsed.backupId : null;
  } catch {
    return null;
  }
}

function appendPlacement(
  db: Database,
  companyRoot: string,
  destinationId: string,
  placement: BackupPlacement,
): { ok: boolean; errors: string[] } {
  const file = loadBackupDestinations(companyRoot);
  const destination = file.destinations.find((d) => d.id === destinationId);
  if (!destination) return { ok: false, errors: [`no backup destination with id: ${destinationId}`] };
  destination.placements.push(placement);
  destination.placements.sort((a, b) => (a.placedAt < b.placedAt ? -1 : a.placedAt > b.placedAt ? 1 : 0));
  saveBackupDestinations(companyRoot, file);
  insertAuditLog(db, {
    eventType: "backup_placed",
    entityType: "company",
    entityId: 1,
    message:
      `Recorded placement of backup ${placement.backupId} at destination '${destination.label}' ` +
      `(${destination.id}); verified=${placement.verified} (${placement.verifyMethod}), ` +
      `actor=${placement.actor} (${placement.actorKind}); ` +
      `destination attested §4-compliant=${isCompliantDestination(destination)} ` +
      "(human attestation — verified means the archive bytes were re-read, not that the server is in the EU/EEA)",
  });
  return { ok: true, errors: [] };
}

export type PlaceBackupArchiveInput = {
  archivePath: string;
  destinationId: string;
  actorKind?: BackupPlacementActorKind;
  actor?: string;
  at?: string;
  note?: string;
};

export type PlaceBackupArchiveResult = {
  ok: boolean;
  placement?: BackupPlacement;
  copiedTo?: string;
  appliedRules: string[];
  errors: string[];
};

// Copies a backup archive into a destination whose `location` is a local
// directory — which covers a Dropbox/Google-Drive *desktop sync folder* just
// as well as a plain folder. The copy is re-read and its sha256 verified, so
// the recorded placement is proven, not assumed. Destinations that are not a
// local directory (e.g. an SSH server) are the agent's job: it transfers the
// file with its own tooling and then calls confirmBackupPlacement.
export function placeBackupArchive(
  db: Database,
  companyRoot: string,
  input: PlaceBackupArchiveInput,
): PlaceBackupArchiveResult {
  const archivePath = trimOrNull(input.archivePath);
  if (!archivePath || !existsSync(archivePath) || !statSync(archivePath).isFile()) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`archive not found: ${input.archivePath}`] };
  }
  const placedAt = resolveAt(input.at);
  if (!placedAt) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: ["at must be a valid ISO-8601 datetime when provided"] };
  }
  const destination = getBackupDestination(companyRoot, input.destinationId);
  if (!destination) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`no backup destination with id: ${input.destinationId}`] };
  }

  const backupId = backupIdFromArchive(archivePath);
  if (!backupId) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      errors: ["archive has no readable manifest.json — not a valid backup archive"],
    };
  }

  const resolvedLocation = resolve(destination.location);
  if (!existsSync(resolvedLocation) || !statSync(resolvedLocation).isDirectory()) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      errors: [
        `destination location is not a local directory: ${destination.location}. ` +
          "Transfer the archive with the agent's own tooling, then record it with confirmBackupPlacement.",
      ],
    };
  }

  const expectedSha256 = sha256File(archivePath);
  const archiveSizeBytes = statSync(archivePath).size;
  const target = join(resolvedLocation, basename(archivePath));
  if (resolve(target) === resolve(archivePath)) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      errors: [
        "destination resolves to the archive's own location — a backup must be placed " +
          "in a separate off-site folder, not copied onto itself",
      ],
    };
  }
  try {
    copyFileSync(archivePath, target);
  } catch (error) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`failed to copy archive to destination: ${String(error)}`] };
  }

  const landedSha256 = sha256File(target);
  if (landedSha256 !== expectedSha256) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      errors: [`copied archive failed verification: expected ${expectedSha256}, got ${landedSha256}`],
    };
  }

  const placement: BackupPlacement = {
    backupId,
    archiveSha256: expectedSha256,
    archiveSizeBytes,
    placedAt,
    actor: trimOrNull(input.actor) ?? "system",
    actorKind: input.actorKind === "agent" ? "agent" : "human",
    verified: true,
    verifyMethod: "sha256-reread",
    note: trimOrNull(input.note),
  };
  const appended = appendPlacement(db, companyRoot, destination.id, placement);
  if (!appended.ok) return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: appended.errors };
  return { ok: true, placement, copiedTo: target, appliedRules: [BACKUP_RULE_ID], errors: [] };
}

export type ConfirmBackupPlacementInput = {
  destinationId: string;
  backupId: string;
  archiveSha256: string;
  archiveSizeBytes?: number;
  actorKind?: BackupPlacementActorKind;
  actor?: string;
  at?: string;
  note?: string;
};

export type ConfirmBackupPlacementResult = {
  ok: boolean;
  placement?: BackupPlacement;
  appliedRules: string[];
  errors: string[];
};

// Records a placement performed OUTSIDE Rentemester — typically the agent
// pushed the archive to Dropbox/Drive/SSH with its own credentials. If the
// destination location is a readable local directory and the archive is
// found there, its sha256 is re-verified; otherwise the placement is
// recorded as "declared" (unproven) so the evidence trail is honest about
// what could and could not be checked.
export function confirmBackupPlacement(
  db: Database,
  companyRoot: string,
  input: ConfirmBackupPlacementInput,
): ConfirmBackupPlacementResult {
  const errors: string[] = [];
  const backupId = trimOrNull(input.backupId);
  const archiveSha256 = trimOrNull(input.archiveSha256);
  if (!backupId) errors.push("backupId is required");
  if (!archiveSha256 || !/^[0-9a-f]{64}$/i.test(archiveSha256)) {
    errors.push("archiveSha256 must be a 64-character hex sha256 digest");
  }
  const placedAt = resolveAt(input.at);
  if (!placedAt) errors.push("at must be a valid ISO-8601 datetime when provided");
  if (errors.length > 0) return { ok: false, appliedRules: [BACKUP_RULE_ID], errors };

  const destination = getBackupDestination(companyRoot, input.destinationId);
  if (!destination) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`no backup destination with id: ${input.destinationId}`] };
  }

  let verified = false;
  let verifyMethod: BackupPlacement["verifyMethod"] = "declared";
  const resolvedLocation = resolve(destination.location);
  if (existsSync(resolvedLocation) && statSync(resolvedLocation).isDirectory()) {
    const tarFiles = readdirSync(resolvedLocation).filter((name) => name.endsWith(".tar"));
    const candidate = tarFiles
      .map((name) => join(resolvedLocation, name))
      .find((path) => {
        try {
          return sha256File(path) === archiveSha256!.toLowerCase();
        } catch {
          return false;
        }
      });
    if (candidate) {
      verified = true;
      verifyMethod = "sha256-reread";
    } else if (tarFiles.length > 0) {
      // The folder is readable and holds archives, but none match the
      // declared digest — refuse to record a false placement.
      return {
        ok: false,
        appliedRules: [BACKUP_RULE_ID],
        errors: [
          `no archive in ${destination.location} matches sha256 ${archiveSha256}; ` +
            "the declared placement could not be confirmed",
        ],
      };
    }
  }

  const placement: BackupPlacement = {
    backupId: backupId!,
    archiveSha256: archiveSha256!.toLowerCase(),
    archiveSizeBytes:
      Number.isFinite(input.archiveSizeBytes) && (input.archiveSizeBytes as number) >= 0
        ? Math.trunc(input.archiveSizeBytes as number)
        : null,
    placedAt: placedAt!,
    actor: trimOrNull(input.actor) ?? "system",
    actorKind: input.actorKind === "human" ? "human" : "agent",
    verified,
    verifyMethod,
    note: trimOrNull(input.note),
  };
  const appended = appendPlacement(db, companyRoot, destination.id, placement);
  if (!appended.ok) return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: appended.errors };
  return { ok: true, placement, appliedRules: [BACKUP_RULE_ID], errors: [] };
}

export type ConfigureBackupLockResult = {
  ok: boolean;
  config?: BackupLockConfig;
  appliedRules: string[];
  errors: string[];
};

export function configureBackupLock(
  db: Database,
  companyRoot: string,
  input: { enforced?: boolean; graceDays?: number; at?: string; actor?: string },
): ConfigureBackupLockResult {
  const updatedAt = resolveAt(input.at);
  if (!updatedAt) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: ["at must be a valid ISO-8601 datetime when provided"] };
  }
  if (
    input.graceDays !== undefined &&
    (!Number.isFinite(input.graceDays) || input.graceDays < 0 || !Number.isInteger(input.graceDays))
  ) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: ["graceDays must be a non-negative integer"] };
  }
  const current = loadBackupLockConfig(companyRoot);
  const next: BackupLockConfig = {
    version: "backup-lock-v1",
    enforced: input.enforced === undefined ? current.enforced : input.enforced === true,
    graceDays: input.graceDays === undefined ? current.graceDays : Math.trunc(input.graceDays),
    updatedAt,
    updatedBy: trimOrNull(input.actor) ?? "system",
  };
  mkdirSync(companyPaths(companyRoot).config, { recursive: true });
  writeFileAtomic(backupLockPath(companyRoot), `${JSON.stringify(next, null, 2)}\n`);
  insertAuditLog(db, {
    eventType: "backup_lock_configured",
    entityType: "company",
    entityId: 1,
    message: `Backup lock configured: enforced=${next.enforced}, graceDays=${next.graceDays}`,
  });
  return { ok: true, config: next, appliedRules: [BACKUP_RULE_ID], errors: [] };
}

// A bare YYYY-MM-DD is anchored to the START of that day. This is used only
// for the EARLIEST activity (the lock anchor for a company that never backed
// up): start-of-day is the most conservative reading — it makes the weekly
// window open as early as possible. (system-backups.ts anchors LATEST
// activity to end-of-day for the symmetric reason.)
function activityMoment(value: string): number {
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return new Date(`${value}T00:00:00.000Z`).getTime();
  return new Date(value).getTime();
}

// Earliest bookkeeping activity across journal entries, documents and bank
// imports. Used to anchor the lock deadline for a company that has done real
// bookkeeping but never once taken a backup.
function earliestActivityMs(db: Database): number | null {
  const rows = [
    (db.query("SELECT MIN(registration_datetime) AS v FROM journal_entries").get() as { v: string | null }).v,
    (db.query("SELECT MIN(upload_datetime) AS v FROM documents").get() as { v: string | null }).v,
    (db.query("SELECT MIN(COALESCE(booking_date, transaction_date)) AS v FROM bank_transactions").get() as { v: string | null }).v,
  ]
    .filter((v): v is string => Boolean(v))
    .map(activityMoment)
    .filter((v) => Number.isFinite(v));
  return rows.length > 0 ? Math.min(...rows) : null;
}

export type BackupLockEvaluation = {
  enforced: boolean;
  locked: boolean;
  backupDue: boolean;
  graceDays: number;
  latestBackupAt: string | null;
  daysSinceLatestBackup: number | null;
  // Instant at which the lock engages (7-day window + grace). Null when no
  // backup is due.
  lockAt: string | null;
  checkedAt: string;
  reason: string;
  errors: string[];
};

// Decides whether bookkeeping is locked. The lock engages only when the
// owner has opted in (config.enforced) AND a weekly backup is genuinely
// overdue past the grace window. A company with no activity since the last
// backup owes no backup (BEK 205/2024 §4 stk.1) and is never locked.
export function evaluateBackupLock(
  db: Database,
  companyRoot: string,
  asOf?: string,
): BackupLockEvaluation {
  const config = loadBackupLockConfig(companyRoot);
  const status = getBackupComplianceStatus(db, companyRoot, asOf);
  const checkedAt = status.checkedAt;

  if (status.errors.length > 0 || !checkedAt) {
    return {
      enforced: config.enforced,
      locked: false,
      backupDue: false,
      graceDays: config.graceDays,
      latestBackupAt: status.latestBackupAt,
      daysSinceLatestBackup: status.daysSinceLatestBackup,
      lockAt: null,
      checkedAt: checkedAt ?? "",
      reason: status.errors[0] ?? "backup status could not be evaluated",
      errors: status.errors,
    };
  }

  if (!config.enforced) {
    return {
      enforced: false,
      locked: false,
      backupDue: status.backupDue,
      graceDays: config.graceDays,
      latestBackupAt: status.latestBackupAt,
      daysSinceLatestBackup: status.daysSinceLatestBackup,
      lockAt: null,
      checkedAt,
      reason: "backup lock is not enforced (opt-in)",
      errors: [],
    };
  }

  if (!status.backupDue) {
    return {
      enforced: true,
      locked: false,
      backupDue: false,
      graceDays: config.graceDays,
      latestBackupAt: status.latestBackupAt,
      daysSinceLatestBackup: status.daysSinceLatestBackup,
      lockAt: null,
      checkedAt,
      reason: status.hasActivitySinceBackup
        ? "backup is recent enough"
        : "no bookkeeping activity since the last backup — no backup is due",
      errors: [],
    };
  }

  const checkedAtMs = new Date(checkedAt).getTime();
  const graceMs = config.graceDays * DAY_MS;
  // The lock deadline runs from the last backup, or — for a company that
  // never backed up — from its earliest bookkeeping activity. The null
  // branch is defensive only: backupDue is true here, which already implies
  // activity exists, so earliestActivityMs cannot realistically be null.
  let anchorMs: number | null = null;
  if (status.latestBackupAt) {
    anchorMs = new Date(status.latestBackupAt).getTime();
  } else {
    anchorMs = earliestActivityMs(db);
  }
  const lockAtMs = anchorMs === null ? checkedAtMs : anchorMs + WEEK_MS + graceMs;
  const locked = checkedAtMs >= lockAtMs;

  return {
    enforced: true,
    locked,
    backupDue: true,
    graceDays: config.graceDays,
    latestBackupAt: status.latestBackupAt,
    daysSinceLatestBackup: status.daysSinceLatestBackup,
    lockAt: new Date(lockAtMs).toISOString(),
    checkedAt,
    reason: locked
      ? "bookkeeping is locked: a weekly backup (BEK 205/2024 §4) is overdue past the grace window"
      : "a backup is due — bookkeeping will lock when the grace window lapses",
    errors: [],
  };
}

export type BackupGovernanceStatus = {
  ok: boolean;
  appliedRules: string[];
  compliance: BackupComplianceStatus;
  lock: BackupLockEvaluation;
  destinations: BackupDestination[];
  destinationCount: number;
  compliantDestinationCount: number;
  hasCompliantDestination: boolean;
  // Whether the most recent backup has a verified placement at a
  // §4-compliant (EU/EEA, non-related, IT-secure) destination.
  latestBackupPlacedOffsite: boolean;
  latestBackupPlacementCount: number;
  checkedAt: string;
  errors: string[];
};

export function getBackupGovernanceStatus(
  db: Database,
  companyRoot: string,
  asOf?: string,
): BackupGovernanceStatus {
  const compliance = getBackupComplianceStatus(db, companyRoot, asOf);
  const lock = evaluateBackupLock(db, companyRoot, asOf);
  const destinations = listBackupDestinations(companyRoot);
  const compliant = destinations.filter(isCompliantDestination);

  let latestBackupPlacementCount = 0;
  let latestBackupPlacedOffsite = false;
  if (compliance.latestBackupId) {
    for (const destination of destinations) {
      for (const placement of destination.placements) {
        if (placement.backupId !== compliance.latestBackupId) continue;
        latestBackupPlacementCount += 1;
        if (placement.verified && isCompliantDestination(destination)) {
          latestBackupPlacedOffsite = true;
        }
      }
    }
  }

  // "ok" means: no backup is overdue, AND if a backup exists it sits
  // verified at a §4-compliant destination.
  const ok =
    compliance.errors.length === 0 &&
    !compliance.backupDue &&
    (compliance.latestBackupId === null || latestBackupPlacedOffsite);

  return {
    ok,
    appliedRules: [BACKUP_RULE_ID],
    compliance,
    lock,
    destinations,
    destinationCount: destinations.length,
    compliantDestinationCount: compliant.length,
    hasCompliantDestination: compliant.length > 0,
    latestBackupPlacedOffsite,
    latestBackupPlacementCount,
    checkedAt: compliance.checkedAt,
    errors: compliance.errors,
  };
}
