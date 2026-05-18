import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { createHash, createHmac, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign as cryptoSign } from "node:crypto";
import { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { insertAuditLog } from "./actor";

const BACKUP_RULE_ID = "DK-BOOKKEEPING-BACKUP-001";
const BACKUP_INTERVAL_MS = 7 * 24 * 60 * 60 * 1000;

export type CreateSystemBackupInput = {
  createdAt?: string;
  debugHoldMs?: number;
  signWithEd25519?: boolean;
};

export type CreateSystemBackupResult = {
  ok: boolean;
  backupId?: string;
  backupDir?: string;
  manifestPath?: string;
  dbSnapshotPath?: string;
  appliedRules: string[];
  errors: string[];
};

export type BackupComplianceStatus = {
  ok: boolean;
  appliedRules: string[];
  latestBackupAt: string | null;
  latestBackupId: string | null;
  backupDue: boolean;
  hasActivitySinceBackup: boolean;
  daysSinceLatestBackup: number | null;
  requiredBy: string | null;
  checkedAt: string;
  backupsFound: number;
  evidence: {
    latestJournalEntryAt: string | null;
    latestDocumentAt: string | null;
    latestBankImportAt: string | null;
  };
  errors: string[];
};

export type ManifestFile = { path: string; sha256: string; sizeBytes: number };

export type BackupAsymmetricSignature = {
  algorithm: "ed25519";
  publicKeyHint: string;
  publicKeyPath: string;
  signaturePath: string;
};

export type BackupManifest = {
  backupId: string;
  createdAt: string;
  ruleIds: string[];
  manifestSignature: {
    algorithm: "hmac-sha256";
    keyHint: string;
    signaturePath: string;
  };
  asymmetricSignature?: BackupAsymmetricSignature;
  dbSnapshot: ManifestFile;
  copiedFiles: {
    documentsOriginals: ManifestFile[];
    invoicesIssued: ManifestFile[];
    config: ManifestFile[];
  };
  ledgerStats: {
    journalEntries: number;
    documents: number;
    bankTransactions: number;
  };
};

function sha256File(path: string) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function backupManifestKeyPath(companyRoot: string) {
  return join(companyRoot, ".backup-manifest.key");
}

export function backupManifestSignaturePath(backupDir: string) {
  return join(backupDir, "manifest.json.hmac");
}

function readBackupManifestKey(companyRoot: string) {
  const path = backupManifestKeyPath(companyRoot);
  if (!existsSync(path)) return null;
  const hex = readFileSync(path, "utf8").trim();
  if (!/^[0-9a-f]{64}$/i.test(hex)) return null;
  return Buffer.from(hex, "hex");
}

function ensureBackupManifestKey(companyRoot: string) {
  const existing = readBackupManifestKey(companyRoot);
  if (existing) return existing;
  const key = randomBytes(32);
  writeFileSync(backupManifestKeyPath(companyRoot), `${key.toString("hex")}\n`, { mode: 0o600 });
  return key;
}

function backupManifestKeyHint(key: Buffer) {
  return createHash("sha256").update(key).digest("hex").slice(0, 16);
}

function signManifestText(manifestText: string, key: Buffer) {
  return createHmac("sha256", key).update(manifestText).digest("hex");
}

// --- Asymmetric (ed25519) signing helpers ----------------------------------
//
// HMAC stays the default. Ed25519 is opt-in via `signWithEd25519` and exists
// so that a 3rd-party (revisor/Skattestyrelsen) can verify the backup
// authenticity using ONLY a public key — without ever holding the secret
// that could forge new backups.

export function backupEd25519PrivateKeyPath(companyRoot: string) {
  // Private key lives next to the HMAC key. Same mode (0o600), same root
  // exclusion: not inside config/, not inside backups/.
  return join(companyRoot, ".backup-signing-key.pem");
}

export function backupEd25519PublicKeyPath(companyRoot: string) {
  // Public key lives in config/ so it is included in every backup via the
  // existing config copy. Distributable safely.
  return join(companyRoot, "config", "backup-manifest.pub");
}

export function backupAsymmetricSignaturePath(backupDir: string) {
  return join(backupDir, "manifest.json.ed25519.sig");
}

export function publicKeyHint(publicKeyPem: string) {
  return createHash("sha256").update(publicKeyPem.trim()).digest("hex").slice(0, 16);
}

export function ensureEd25519Keypair(companyRoot: string): {
  privateKeyPem: string;
  publicKeyPem: string;
  publicKeyPath: string;
  privateKeyPath: string;
} {
  const privPath = backupEd25519PrivateKeyPath(companyRoot);
  const pubPath = backupEd25519PublicKeyPath(companyRoot);
  if (existsSync(privPath) && existsSync(pubPath)) {
    return {
      privateKeyPem: readFileSync(privPath, "utf8"),
      publicKeyPem: readFileSync(pubPath, "utf8"),
      privateKeyPath: privPath,
      publicKeyPath: pubPath,
    };
  }
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const privateKeyPem = privateKey.export({ type: "pkcs8", format: "pem" }).toString();
  const publicKeyPem = publicKey.export({ type: "spki", format: "pem" }).toString();
  // Ensure config dir exists before writing public key.
  mkdirSync(join(companyRoot, "config"), { recursive: true });
  writeFileSync(privPath, privateKeyPem, { mode: 0o600 });
  writeFileSync(pubPath, publicKeyPem);
  return { privateKeyPem, publicKeyPem, privateKeyPath: privPath, publicKeyPath: pubPath };
}

function signManifestEd25519(manifestText: string, privateKeyPem: string): string {
  const key = createPrivateKey(privateKeyPem);
  const sig = cryptoSign(null, Buffer.from(manifestText, "utf8"), key);
  return sig.toString("base64");
}

export function exportBackupPublicKey(companyRoot: string, outPath: string): { ok: true; outPath: string; publicKeyHint: string } | { ok: false; error: string } {
  const pubPath = backupEd25519PublicKeyPath(companyRoot);
  if (!existsSync(pubPath)) {
    return { ok: false, error: `no ed25519 public key found at ${pubPath}; run "system backup --sign-with-ed25519" first to generate the keypair` };
  }
  const pem = readFileSync(pubPath, "utf8");
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, pem);
  return { ok: true, outPath, publicKeyHint: publicKeyHint(pem) };
}

function relativeBackupPath(backupDir: string, filePath: string) {
  return relative(backupDir, filePath).replaceAll("\\", "/");
}

function copyDirWithManifest(sourceDir: string, targetDir: string, backupDir: string) {
  const copied: ManifestFile[] = [];
  if (!existsSync(sourceDir)) return copied;
  mkdirSync(targetDir, { recursive: true });
  for (const entry of readdirSync(sourceDir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const sourcePath = join(sourceDir, entry.name);
    const targetPath = join(targetDir, entry.name);
    copyFileSync(sourcePath, targetPath);
    const stats = statSync(targetPath);
    copied.push({ path: relativeBackupPath(backupDir, targetPath), sha256: sha256File(targetPath), sizeBytes: stats.size });
  }
  return copied.sort((a, b) => a.path.localeCompare(b.path));
}

function sqlString(value: string) {
  return `'${value.replaceAll("'", "''")}'`;
}

function resolveCreatedAt(value?: string) {
  const createdAt = value ?? new Date().toISOString();
  if (Number.isNaN(Date.parse(createdAt))) return null;
  return new Date(createdAt).toISOString();
}

function backupIdFromIso(iso: string) {
  return `backup-${iso.replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z")}`;
}

function readBackupManifest(path: string): BackupManifest | null {
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, "utf8")) as BackupManifest;
  } catch {
    return null;
  }
}

function listBackupManifests(companyRoot: string) {
  const p = companyPaths(companyRoot);
  if (!existsSync(p.backups)) return [] as BackupManifest[];
  return readdirSync(p.backups, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => readBackupManifest(join(p.backups, entry.name, "manifest.json")))
    .filter((manifest): manifest is BackupManifest => Boolean(manifest))
    .sort((a, b) => a.createdAt.localeCompare(b.createdAt));
}

function latestActivityTimestamps(db: Database) {
  const latestJournalEntryAt = (db.query("SELECT MAX(registration_datetime) AS value FROM journal_entries").get() as { value: string | null }).value;
  const latestDocumentAt = (db.query("SELECT MAX(upload_datetime) AS value FROM documents").get() as { value: string | null }).value;
  const latestBankImportAt = (db.query("SELECT MAX(COALESCE(booking_date, transaction_date)) AS value FROM bank_transactions").get() as { value: string | null }).value;
  return { latestJournalEntryAt, latestDocumentAt, latestBankImportAt };
}

function parseActivityMoment(value: string) {
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return new Date(`${value}T23:59:59.999Z`).getTime();
  return new Date(value).getTime();
}

function sleepSync(ms: number) {
  if (ms <= 0) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function buildLockedSnapshot(db: Database, sourceDbPath: string, snapshotPath: string, holdMs = 0) {
  const snapshotDb = new Database(sourceDbPath);
  snapshotDb.exec("PRAGMA busy_timeout = 5000;");
  let began = false;
  try {
    db.exec("BEGIN IMMEDIATE;");
    began = true;
    sleepSync(holdMs);
    snapshotDb.exec(`VACUUM INTO ${sqlString(snapshotPath)}`);
    db.exec("COMMIT;");
    began = false;
  } catch (error) {
    if (began) db.exec("ROLLBACK;");
    throw error;
  } finally {
    snapshotDb.close();
  }
}

export function createSystemBackup(db: Database, companyRoot: string, input: CreateSystemBackupInput = {}): CreateSystemBackupResult {
  const createdAt = resolveCreatedAt(input.createdAt);
  if (!createdAt) return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: ["createdAt must be a valid ISO-8601 datetime when provided"] };
  if (input.debugHoldMs !== undefined && (!Number.isFinite(input.debugHoldMs) || input.debugHoldMs < 0)) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: ["debugHoldMs must be a non-negative number when provided"] };
  }

  const paths = companyPaths(companyRoot);
  mkdirSync(paths.backups, { recursive: true });
  const backupId = backupIdFromIso(createdAt);
  const backupDir = join(paths.backups, backupId);
  if (existsSync(backupDir)) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`backup already exists: ${backupId}`] };
  }

  mkdirSync(backupDir, { recursive: true });
  const dbSnapshotPath = join(backupDir, "ledger.sqlite");
  const documentsBackupDir = join(backupDir, "documents-originals");
  const invoicesBackupDir = join(backupDir, "invoices-issued");
  const configBackupDir = join(backupDir, "config");

  try {
    buildLockedSnapshot(db, paths.db, dbSnapshotPath, input.debugHoldMs ?? 0);
  } catch (error) {
    return { ok: false, appliedRules: [BACKUP_RULE_ID], errors: [`failed to create locked backup snapshot: ${String(error)}`] };
  }

  const snapshotDb = new Database(dbSnapshotPath, { readonly: true });
  const ledgerStats = {
    journalEntries: (snapshotDb.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n,
    documents: (snapshotDb.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n,
    bankTransactions: (snapshotDb.query("SELECT COUNT(*) AS n FROM bank_transactions").get() as { n: number }).n,
  };
  snapshotDb.close();

  const manifestKey = ensureBackupManifestKey(companyRoot);

  // If asymmetric signing is requested, generate the keypair BEFORE we copy
  // config/, so the freshly-created public key ends up inside the backup as
  // part of the standard config copy. The private key stays at
  // <companyRoot>/.backup-signing-key.pem (mode 0o600) and is never copied.
  let asymmetricKeypair: ReturnType<typeof ensureEd25519Keypair> | null = null;
  if (input.signWithEd25519) {
    asymmetricKeypair = ensureEd25519Keypair(companyRoot);
  }

  const copiedConfig = copyDirWithManifest(paths.config, configBackupDir, backupDir);

  const asymmetricSignature: BackupAsymmetricSignature | undefined = asymmetricKeypair
    ? {
        algorithm: "ed25519",
        publicKeyHint: publicKeyHint(asymmetricKeypair.publicKeyPem),
        publicKeyPath: relativeBackupPath(backupDir, join(configBackupDir, "backup-manifest.pub")),
        signaturePath: relativeBackupPath(backupDir, backupAsymmetricSignaturePath(backupDir)),
      }
    : undefined;

  const manifest: BackupManifest = {
    backupId,
    createdAt,
    ruleIds: [BACKUP_RULE_ID],
    manifestSignature: {
      algorithm: "hmac-sha256",
      keyHint: backupManifestKeyHint(manifestKey),
      signaturePath: relativeBackupPath(backupDir, backupManifestSignaturePath(backupDir)),
    },
    ...(asymmetricSignature ? { asymmetricSignature } : {}),
    dbSnapshot: {
      path: relativeBackupPath(backupDir, dbSnapshotPath),
      sha256: sha256File(dbSnapshotPath),
      sizeBytes: statSync(dbSnapshotPath).size,
    },
    copiedFiles: {
      documentsOriginals: copyDirWithManifest(paths.documentsOriginals, documentsBackupDir, backupDir),
      invoicesIssued: copyDirWithManifest(paths.invoicesIssued, invoicesBackupDir, backupDir),
      config: copiedConfig,
    },
    ledgerStats,
  };

  const manifestPath = join(backupDir, "manifest.json");
  const manifestText = `${JSON.stringify(manifest, null, 2)}\n`;
  writeFileSync(manifestPath, manifestText);
  writeFileSync(backupManifestSignaturePath(backupDir), `${signManifestText(manifestText, manifestKey)}\n`);
  if (asymmetricKeypair) {
    writeFileSync(backupAsymmetricSignaturePath(backupDir), `${signManifestEd25519(manifestText, asymmetricKeypair.privateKeyPem)}\n`);
  }

  insertAuditLog(db, {
    eventType: "system_backup",
    entityType: "company",
    entityId: 1,
    message: `Created full backup ${backupId}`,
    fallbackActor: { createdBy: "scheduled:system_backup", createdByProgram: "rentemester-cron" },
  });

  return { ok: true, backupId, backupDir, manifestPath, dbSnapshotPath, appliedRules: [BACKUP_RULE_ID], errors: [] };
}

export function getBackupComplianceStatus(db: Database, companyRoot: string, asOf?: string): BackupComplianceStatus {
  const checkedAt = resolveCreatedAt(asOf);
  if (!checkedAt) {
    return {
      ok: false,
      appliedRules: [BACKUP_RULE_ID],
      latestBackupAt: null,
      latestBackupId: null,
      backupDue: false,
      hasActivitySinceBackup: false,
      daysSinceLatestBackup: null,
      requiredBy: null,
      checkedAt: asOf ?? "",
      backupsFound: 0,
      evidence: { latestJournalEntryAt: null, latestDocumentAt: null, latestBankImportAt: null },
      errors: ["asOf must be a valid ISO-8601 datetime when provided"],
    };
  }

  const manifests = listBackupManifests(companyRoot);
  const latest = manifests.at(-1) ?? null;
  const evidence = latestActivityTimestamps(db);
  const latestBackupAt = latest?.createdAt ?? null;
  const latestBackupId = latest?.backupId ?? null;

  const activityMoments = [evidence.latestJournalEntryAt, evidence.latestDocumentAt, evidence.latestBankImportAt]
    .filter((value): value is string => Boolean(value))
    .map((value) => parseActivityMoment(value))
    .filter((value) => Number.isFinite(value));

  const latestBackupMs = latestBackupAt ? new Date(latestBackupAt).getTime() : null;
  const checkedAtMs = new Date(checkedAt).getTime();
  const latestActivityMs = activityMoments.length > 0 ? Math.max(...activityMoments) : null;
  const hasActivitySinceBackup = latestBackupMs === null ? activityMoments.length > 0 : latestActivityMs !== null && latestActivityMs > latestBackupMs;
  // money-allowed: days-since-backup math, not currency
  const daysSinceLatestBackup = latestBackupMs === null ? null : Number(((checkedAtMs - latestBackupMs) / (24 * 60 * 60 * 1000)).toFixed(2));
  const backupDue = latestBackupMs === null ? hasActivitySinceBackup : hasActivitySinceBackup && (checkedAtMs - latestBackupMs > BACKUP_INTERVAL_MS);
  const requiredBy = latestBackupMs === null ? (hasActivitySinceBackup ? checkedAt : null) : hasActivitySinceBackup ? new Date(latestBackupMs + BACKUP_INTERVAL_MS).toISOString() : null;

  return {
    ok: !backupDue,
    appliedRules: [BACKUP_RULE_ID],
    latestBackupAt,
    latestBackupId,
    backupDue,
    hasActivitySinceBackup,
    daysSinceLatestBackup,
    requiredBy,
    checkedAt,
    backupsFound: manifests.length,
    evidence,
    errors: [],
  };
}
