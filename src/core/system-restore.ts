import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, dirname, isAbsolute, join, normalize, resolve } from "node:path";
import { createHash, createHmac, createPublicKey, timingSafeEqual, verify as cryptoVerify } from "node:crypto";
import { openDb } from "./db";
import { verifyAuditChain } from "./ledger";
import { companyPaths, ensureCompanyDirs } from "./paths";
import { backupAsymmetricSignaturePath, backupManifestKeyPath, backupManifestSignaturePath } from "./system-backups";
import type { BackupManifest, ManifestFile } from "./system-backups";
import { insertAuditLog } from "./actor";

const RULE_ID = "DK-BOOKKEEPING-RESTORE-001";

export type RestoreSystemBackupInput = {
  backupDir: string;
  targetCompanyRoot: string;
  verificationKeyPath?: string;
  publicKeyPath?: string;
};

export type RestoreSystemBackupResult = {
  ok: boolean;
  backupId?: string;
  restoredAt?: string;
  targetCompanyRoot?: string;
  restoredDbPath?: string;
  restoredFiles?: {
    documentsOriginals: number;
    invoicesIssued: number;
    config: number;
  };
  appliedRules: string[];
  errors: string[];
};

function sha256File(path: string) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function readManifestText(backupDir: string) {
  const manifestPath = join(backupDir, "manifest.json");
  if (!existsSync(manifestPath)) return null;
  try {
    return readFileSync(manifestPath, "utf8");
  } catch {
    return null;
  }
}

function readManifest(backupDir: string): BackupManifest | null {
  const manifestText = readManifestText(backupDir);
  if (!manifestText) return null;
  try {
    return JSON.parse(manifestText) as BackupManifest;
  } catch {
    return null;
  }
}

function resolveManifestPath(backupDir: string, manifestPath: string) {
  const resolvedBackupDir = resolve(backupDir);
  const candidate = isAbsolute(manifestPath) ? resolve(manifestPath) : resolve(resolvedBackupDir, manifestPath);
  const normalizedRoot = `${resolvedBackupDir}${resolvedBackupDir.endsWith("/") ? "" : "/"}`;
  const normalizedCandidate = normalize(candidate);
  if (normalizedCandidate !== resolvedBackupDir && !normalizedCandidate.startsWith(normalizedRoot)) return null;
  return normalizedCandidate;
}

function ensureMatches(backupDir: string, file: ManifestFile) {
  const resolvedPath = resolveManifestPath(backupDir, file.path);
  if (!resolvedPath) return `manifest path escapes backup dir: ${file.path}`;
  if (!existsSync(resolvedPath)) return `missing backup file: ${file.path}`;
  const actualSize = statSync(resolvedPath).size;
  if (actualSize !== file.sizeBytes) return `size mismatch for ${file.path}`;
  const actualHash = sha256File(resolvedPath);
  if (actualHash !== file.sha256) return `sha256 mismatch for ${file.path}`;
  return null;
}

function inferVerificationKeyPath(backupDir: string) {
  const resolvedBackupDir = resolve(backupDir);
  const backupsDir = dirname(resolvedBackupDir);
  if (basename(backupsDir) !== "backups") return null;
  return backupManifestKeyPath(dirname(backupsDir));
}

function manifestHmac(manifestText: string, keyHex: string) {
  return createHmac("sha256", Buffer.from(keyHex, "hex")).update(manifestText).digest("hex");
}

function verifyManifestHmac(backupDir: string, manifestText: string, verificationKeyPath?: string) {
  const signaturePath = backupManifestSignaturePath(backupDir);
  if (!existsSync(signaturePath)) return "missing backup manifest signature: manifest.json.hmac";
  const signature = readFileSync(signaturePath, "utf8").trim();
  if (!/^[0-9a-f]{64}$/i.test(signature)) return "invalid backup manifest signature format";

  const keyPath = verificationKeyPath ?? inferVerificationKeyPath(backupDir);
  if (!keyPath) return "backup authenticity key not found; pass verificationKeyPath or restore from the original company backups directory";
  if (!existsSync(keyPath)) return `backup authenticity key not found: ${keyPath}`;
  const keyHex = readFileSync(keyPath, "utf8").trim();
  if (!/^[0-9a-f]{64}$/i.test(keyHex)) return `backup authenticity key is invalid: ${keyPath}`;

  const expected = Buffer.from(manifestHmac(manifestText, keyHex), "hex");
  const actual = Buffer.from(signature, "hex");
  if (expected.length !== actual.length || !timingSafeEqual(expected, actual)) return "backup manifest authenticity check failed";
  return null;
}

function verifyManifestEd25519(
  backupDir: string,
  manifest: BackupManifest,
  manifestText: string,
  overridePublicKeyPath?: string,
): string | null {
  if (!manifest.asymmetricSignature) return null; // nothing to verify
  const sigPath = backupAsymmetricSignaturePath(backupDir);
  if (!existsSync(sigPath)) return "missing ed25519 backup manifest signature: manifest.json.ed25519.sig";
  const sigBase64 = readFileSync(sigPath, "utf8").trim();
  let sigBytes: Buffer;
  try {
    sigBytes = Buffer.from(sigBase64, "base64");
  } catch {
    return "invalid ed25519 signature encoding";
  }
  if (sigBytes.length !== 64) return `invalid ed25519 signature length: ${sigBytes.length} (expected 64)`;

  // Resolve public key: explicit override > path embedded in manifest > <backupDir>/config/backup-manifest.pub
  let publicKeyPath: string | null = null;
  if (overridePublicKeyPath) {
    publicKeyPath = overridePublicKeyPath;
  } else {
    const declared = resolveManifestPath(backupDir, manifest.asymmetricSignature.publicKeyPath);
    if (declared && existsSync(declared)) {
      publicKeyPath = declared;
    } else {
      const fallback = join(backupDir, "config", "backup-manifest.pub");
      if (existsSync(fallback)) publicKeyPath = fallback;
    }
  }
  if (!publicKeyPath || !existsSync(publicKeyPath)) {
    return "ed25519 public key not found; pass publicKeyPath or restore from a backup that ships the key under config/backup-manifest.pub";
  }
  const pem = readFileSync(publicKeyPath, "utf8");
  let key;
  try {
    key = createPublicKey(pem);
  } catch (error) {
    return `failed to parse ed25519 public key: ${String(error)}`;
  }
  const ok = cryptoVerify(null, Buffer.from(manifestText, "utf8"), key, sigBytes);
  if (!ok) return "ed25519 manifest signature verification failed";
  return null;
}

function verifyManifestAuthenticity(
  backupDir: string,
  manifest: BackupManifest,
  manifestText: string,
  verificationKeyPath?: string,
  publicKeyPath?: string,
) {
  const hmacError = verifyManifestHmac(backupDir, manifestText, verificationKeyPath);
  if (hmacError) return hmacError;
  // If the manifest advertises an ed25519 signature, it MUST also verify.
  // This means: opting in to asymmetric signing strengthens the guarantee
  // (both HMAC and ed25519 must agree); it never weakens HMAC.
  const ed25519Error = verifyManifestEd25519(backupDir, manifest, manifestText, publicKeyPath);
  if (ed25519Error) return ed25519Error;
  return null;
}

function companyLooksEmpty(targetCompanyRoot: string) {
  if (!existsSync(targetCompanyRoot)) return true;
  return readdirSync(targetCompanyRoot).length === 0;
}

function restoreFiles(backupDir: string, files: ManifestFile[], targetDir: string) {
  mkdirSync(targetDir, { recursive: true });
  for (const file of files) {
    const sourcePath = resolveManifestPath(backupDir, file.path);
    if (!sourcePath) throw new Error(`manifest path escapes backup dir: ${file.path}`);
    copyFileSync(sourcePath, join(targetDir, basename(sourcePath)));
  }
  return files.length;
}

function validateRestoredDb(dbPath: string, manifest: BackupManifest) {
  const db = openDb(dbPath);
  try {
    const integrity = db.query("PRAGMA integrity_check").all() as Array<{ integrity_check?: string }>;
    if (integrity.length !== 1 || integrity[0]?.integrity_check !== "ok") {
      return { ok: false, error: `restored database failed integrity check: ${JSON.stringify(integrity)}` };
    }

    const fkErrors = db.query("PRAGMA foreign_key_check").all() as any[];
    if (fkErrors.length > 0) {
      return { ok: false, error: `restored database has FK violations: ${JSON.stringify(fkErrors)}` };
    }

    const audit = verifyAuditChain(db);
    if (!audit.ok) {
      return { ok: false, error: `restored database has broken audit chain: ${audit.errors.join(", ")}` };
    }

    const stats = {
      journalEntries: (db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n,
      documents: (db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n,
      bankTransactions: (db.query("SELECT COUNT(*) AS n FROM bank_transactions").get() as { n: number }).n,
    };
    if (JSON.stringify(stats) !== JSON.stringify(manifest.ledgerStats)) {
      return { ok: false, error: `restored stats ${JSON.stringify(stats)} differ from manifest ${JSON.stringify(manifest.ledgerStats)}` };
    }

    return { ok: true as const };
  } finally {
    db.close();
  }
}

export type VerifyBackupSignatureInput = {
  backupDir: string;
  verificationKeyPath?: string;
  publicKeyPath?: string;
};

export type VerifyBackupSignatureResult = {
  ok: boolean;
  backupId?: string;
  algorithms: string[];
  publicKeyHint?: string;
  hmacKeyHint?: string;
  errors: string[];
};

// Standalone verification — no restore, no DB writes, no target directory
// required. Designed for 3rd-party use (revisor/Skattestyrelsen) where the
// verifier holds only the backup directory and (optionally) a public key
// file received out-of-band.
export function verifyBackupSignature(input: VerifyBackupSignatureInput): VerifyBackupSignatureResult {
  if (!input.backupDir || !existsSync(input.backupDir)) {
    return { ok: false, algorithms: [], errors: [`backupDir does not exist: ${input.backupDir}`] };
  }
  const manifestText = readManifestText(input.backupDir);
  if (!manifestText) return { ok: false, algorithms: [], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const manifest = readManifest(input.backupDir);
  if (!manifest) return { ok: false, algorithms: [], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };

  const algorithms: string[] = [];
  const errors: string[] = [];

  // Ed25519 is the 3rd-party-friendly path. Try it first, but only if the
  // manifest advertises it. If the verifier only has a public key (no HMAC
  // key) and the manifest has no ed25519 signature, we cannot verify.
  if (manifest.asymmetricSignature) {
    const ed25519Error = verifyManifestEd25519(input.backupDir, manifest, manifestText, input.publicKeyPath);
    if (ed25519Error) {
      errors.push(ed25519Error);
    } else {
      algorithms.push("ed25519");
    }
  }

  // HMAC is verified when a key path is supplied or one can be inferred
  // (i.e. we are next to the source company root). Skipped silently for
  // pure 3rd-party verification where only the public key is held.
  const hasInferableHmacKey = input.verificationKeyPath || inferVerificationKeyPath(input.backupDir);
  if (hasInferableHmacKey) {
    const hmacError = verifyManifestHmac(input.backupDir, manifestText, input.verificationKeyPath);
    if (hmacError) {
      errors.push(hmacError);
    } else {
      algorithms.push("hmac-sha256");
    }
  }

  if (algorithms.length === 0 && errors.length === 0) {
    errors.push(
      "no verifiable signature found: manifest has no asymmetricSignature block and no HMAC key was provided or inferable",
    );
  }

  // ALSO verify manifest file integrity claims (sha256/size) — a valid
  // signature over a manifest whose file hashes no longer match the on-disk
  // bytes is still a tamper-detection failure.
  const fileErrors = [
    ensureMatches(input.backupDir, manifest.dbSnapshot),
    ...manifest.copiedFiles.documentsOriginals.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.invoicesIssued.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.config.map((file) => ensureMatches(input.backupDir, file)),
  ].filter((value): value is string => Boolean(value));
  errors.push(...fileErrors);

  return {
    ok: errors.length === 0 && algorithms.length > 0,
    backupId: manifest.backupId,
    algorithms,
    publicKeyHint: manifest.asymmetricSignature?.publicKeyHint,
    hmacKeyHint: manifest.manifestSignature?.keyHint,
    errors,
  };
}

export function restoreSystemBackup(input: RestoreSystemBackupInput): RestoreSystemBackupResult {
  const errors: string[] = [];
  if (!input.backupDir || !existsSync(input.backupDir)) errors.push(`backupDir does not exist: ${input.backupDir}`);
  if (!input.targetCompanyRoot) errors.push("targetCompanyRoot is required");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const manifestText = readManifestText(input.backupDir);
  if (!manifestText) return { ok: false, appliedRules: [RULE_ID], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const manifest = readManifest(input.backupDir);
  if (!manifest) return { ok: false, appliedRules: [RULE_ID], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const authenticityError = verifyManifestAuthenticity(input.backupDir, manifest, manifestText, input.verificationKeyPath, input.publicKeyPath);
  if (authenticityError) return { ok: false, appliedRules: [RULE_ID], errors: [authenticityError] };

  const manifestErrors = [
    ensureMatches(input.backupDir, manifest.dbSnapshot),
    ...manifest.copiedFiles.documentsOriginals.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.invoicesIssued.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.config.map((file) => ensureMatches(input.backupDir, file)),
  ].filter((value): value is string => Boolean(value));
  if (manifestErrors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors: manifestErrors };

  if (!companyLooksEmpty(input.targetCompanyRoot)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`targetCompanyRoot must be empty or absent: ${input.targetCompanyRoot}`] };
  }

  const paths = ensureCompanyDirs(input.targetCompanyRoot);
  const snapshotPath = resolveManifestPath(input.backupDir, manifest.dbSnapshot.path);
  if (!snapshotPath) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`manifest path escapes backup dir: ${manifest.dbSnapshot.path}`] };
  }

  copyFileSync(snapshotPath, paths.db);
  const restoredFiles = {
    documentsOriginals: restoreFiles(input.backupDir, manifest.copiedFiles.documentsOriginals, paths.documentsOriginals),
    invoicesIssued: restoreFiles(input.backupDir, manifest.copiedFiles.invoicesIssued, paths.invoicesIssued),
    config: restoreFiles(input.backupDir, manifest.copiedFiles.config, paths.config),
  };

  const restoredAt = new Date().toISOString();
  const validation = validateRestoredDb(paths.db, manifest);
  if (!validation.ok) {
    return { ok: false, appliedRules: [RULE_ID], errors: [validation.error] };
  }

  const db = openDb(paths.db);
  try {
    insertAuditLog(db, {
      eventType: "system_restore",
      entityType: "company",
      entityId: 1,
      message: `Restored from backup ${manifest.backupId} (created ${manifest.createdAt}) at ${restoredAt}`,
    });
  } finally {
    db.close();
  }

  return {
    ok: true,
    backupId: manifest.backupId,
    restoredAt,
    targetCompanyRoot: input.targetCompanyRoot,
    restoredDbPath: companyPaths(input.targetCompanyRoot).db,
    restoredFiles,
    appliedRules: [RULE_ID],
    errors: [],
  };
}
