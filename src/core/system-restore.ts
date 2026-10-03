import { copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, relative, resolve, win32 } from "node:path";
import { createHash, createHmac, createPublicKey, randomBytes, timingSafeEqual, verify as cryptoVerify } from "node:crypto";
import { Database } from "bun:sqlite";
import { verifyAuditChain } from "./ledger";
import { companyPaths, ensureCompanyDirs } from "./paths";
import { assertBackupDocumentEvidence, backupAsymmetricSignaturePath, backupManifestKeyPath, backupManifestSignaturePath } from "./system-backups";
import type { BackupManifest, ManifestFile } from "./system-backups";
import { isReleaseProvenance } from "./release-provenance";
import { extractTar } from "./tar";
import { insertAuditLog } from "./actor";
import { removePathWithRetry, renamePathWithRetry } from "./fs-cleanup";
import { writeFileAtomic } from "./atomic-file";
import { assertSchemaCompatibility } from "./schema-version";
import { migrate } from "./db";

const RULE_ID = "DK-BOOKKEEPING-RESTORE-001";

export type RestoreSystemBackupInput = {
  backupDir: string;
  targetCompanyRoot: string;
  verificationKeyPath?: string;
  publicKeyPath?: string;
  // Optional out-of-band public-key hint. When set, an ed25519 public key
  // resolved from inside the backup must match it or restore fails closed
  // (issue #132). Backward-compatible: omitted -> previous behaviour.
  publicKeyHint?: string;
  // KODE-8: explicit opt-in to overwrite a target directory that already exists
  // and is NOT empty (but holds no ledger db). Without it, restore refuses to
  // recursively delete a non-empty ledger-less directory — it might be the
  // user's own files, pointed at by mistake. An empty or non-existent target is
  // always fine and needs no flag.
  allowNonEmptyTarget?: boolean;
  /** Internal portable-workspace mode: no HMAC secret ships; Ed25519 must verify. */
  credentialFreePortableMode?: boolean;
  createdBy?: string;
  createdByProgram?: string;
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

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isManifestFile(value: unknown): value is ManifestFile {
  if (!isObject(value)) return false;
  return (
    typeof value.path === "string" &&
    typeof value.sha256 === "string" &&
    /^[0-9a-f]{64}$/i.test(value.sha256) &&
    typeof value.sizeBytes === "number" &&
    Number.isSafeInteger(value.sizeBytes) &&
    value.sizeBytes >= 0
  );
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** Parse and validate the versioned contract before signature/file handling. */
export function parseBackupManifestText(manifestText: string): BackupManifest | null {
  try {
    const value = JSON.parse(manifestText) as unknown;
    if (!isObject(value)) return null;
    const version = value.manifestVersion;
    if (version !== undefined && version !== 1 && version !== 2) return null;
    if (version === 2 && !isReleaseProvenance(value.provenance)) return null;
    if (
      typeof value.backupId !== "string" ||
      typeof value.createdAt !== "string" ||
      !Array.isArray(value.ruleIds) ||
      !value.ruleIds.every((rule) => typeof rule === "string") ||
      !isObject(value.manifestSignature) ||
      value.manifestSignature.algorithm !== "hmac-sha256" ||
      typeof value.manifestSignature.keyHint !== "string" ||
      typeof value.manifestSignature.signaturePath !== "string" ||
      !isManifestFile(value.dbSnapshot) ||
      !isObject(value.copiedFiles) ||
      !Array.isArray(value.copiedFiles.documentsOriginals) ||
      !value.copiedFiles.documentsOriginals.every(isManifestFile) ||
      !Array.isArray(value.copiedFiles.invoicesIssued) ||
      !value.copiedFiles.invoicesIssued.every(isManifestFile) ||
      !Array.isArray(value.copiedFiles.config) ||
      !value.copiedFiles.config.every(isManifestFile) ||
      !isObject(value.ledgerStats) ||
      !isNonNegativeInteger(value.ledgerStats.journalEntries) ||
      !isNonNegativeInteger(value.ledgerStats.documents) ||
      !isNonNegativeInteger(value.ledgerStats.bankTransactions)
    ) return null;
    if (value.asymmetricSignature !== undefined) {
      const signature = value.asymmetricSignature;
      if (
        !isObject(signature) ||
        signature.algorithm !== "ed25519" ||
        typeof signature.publicKeyHint !== "string" ||
        typeof signature.publicKeyPath !== "string" ||
        typeof signature.signaturePath !== "string"
      ) return null;
    }
    return value as BackupManifest;
  } catch {
    return null;
  }
}

function readManifest(backupDir: string): BackupManifest | null {
  const manifestText = readManifestText(backupDir);
  return manifestText ? parseBackupManifestText(manifestText) : null;
}

export function resolveManifestPath(backupDir: string, manifestPath: string) {
  // A manifest can be created on Windows and restored elsewhere. Treat both
  // separators as separators, and reject drive/UNC paths on every host.
  const useWindowsPaths = /^[A-Za-z]:[\\/]|^\\\\/.test(backupDir);
  const pathApi = useWindowsPaths ? win32 : { resolve, relative, isAbsolute };
  if (isAbsolute(manifestPath) || win32.isAbsolute(manifestPath)) return null;
  const normalizedManifestPath = useWindowsPaths
    ? manifestPath.replace(/\//g, "\\")
    : manifestPath.replace(/\\/g, "/");
  const resolvedBackupDir = pathApi.resolve(backupDir);
  const candidate = pathApi.resolve(resolvedBackupDir, normalizedManifestPath);
  const fromRoot = pathApi.relative(resolvedBackupDir, candidate);
  if (fromRoot === ".." || fromRoot.startsWith(`..${useWindowsPaths ? "\\" : "/"}`) || pathApi.isAbsolute(fromRoot)) return null;
  return candidate;
}

function isSymlinkSafeManifestPath(backupDir: string, candidate: string) {
  try {
    const fromRoot = relative(realpathSync(backupDir), realpathSync(candidate));
    return fromRoot !== ".." && !fromRoot.startsWith("../") && !isAbsolute(fromRoot) && !lstatSync(candidate).isSymbolicLink();
  } catch {
    return false;
  }
}

function ensureMatches(backupDir: string, file: ManifestFile) {
  const resolvedPath = resolveManifestPath(backupDir, file.path);
  if (!resolvedPath) return `manifest path escapes backup dir: ${file.path}`;
  if (!existsSync(resolvedPath)) return `missing backup file: ${file.path}`;
  if (!isSymlinkSafeManifestPath(backupDir, resolvedPath)) return `manifest path escapes backup dir through symlink: ${file.path}`;
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

function publicKeyHint(publicKeyPem: string) {
  return createHash("sha256").update(publicKeyPem.trim()).digest("hex").slice(0, 16);
}

type Ed25519VerifyResult =
  | { error: string }
  // keySource distinguishes a key the verifier supplied out-of-band (genuine
  // 3rd-party authenticity) from one resolved inside the backup itself. An
  // in-backup key only proves the backup is internally self-consistent — a
  // local actor who re-signs with a fresh keypair would also pass — so it is
  // INTEGRITY-ONLY, never authenticity (issue #132).
  | { ok: true; keySource: "out-of-band" | "in-backup"; publicKeyHint: string };

function verifyManifestEd25519(
  backupDir: string,
  manifest: BackupManifest,
  manifestText: string,
  overridePublicKeyPath?: string,
  expectedPublicKeyHint?: string,
): Ed25519VerifyResult | null {
  if (!manifest.asymmetricSignature) return null; // nothing to verify
  const sigPath = backupAsymmetricSignaturePath(backupDir);
  if (!existsSync(sigPath)) return { error: "missing ed25519 backup manifest signature: manifest.json.ed25519.sig" };
  const sigBase64 = readFileSync(sigPath, "utf8").trim();
  let sigBytes: Buffer;
  try {
    sigBytes = Buffer.from(sigBase64, "base64");
  } catch {
    return { error: "invalid ed25519 signature encoding" };
  }
  if (sigBytes.length !== 64) return { error: `invalid ed25519 signature length: ${sigBytes.length} (expected 64)` };

  // Resolve public key: explicit override (out-of-band) > path embedded in
  // manifest > <backupDir>/config/backup-manifest.pub. Only the override is
  // trusted for authenticity; manifest-declared / config keys ship INSIDE the
  // backup and are integrity-only.
  let publicKeyPath: string | null = null;
  let keySource: "out-of-band" | "in-backup" = "in-backup";
  if (overridePublicKeyPath) {
    publicKeyPath = overridePublicKeyPath;
    keySource = "out-of-band";
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
    return { error: "ed25519 public key not found; pass publicKeyPath or restore from a backup that ships the key under config/backup-manifest.pub" };
  }
  const pem = readFileSync(publicKeyPath, "utf8");
  let key: ReturnType<typeof createPublicKey>;
  try {
    key = createPublicKey(pem);
  } catch (error) {
    return { error: `failed to parse ed25519 public key: ${String(error)}` };
  }

  // Fail-closed hint check (issue #132): if the verifier supplied a public-key
  // hint out-of-band, the key actually used MUST match it. This defeats an
  // attacker who deletes the keypair, re-signs a tampered backup with a fresh
  // keypair, and ships the forged public key inside the backup.
  const resolvedHint = publicKeyHint(pem);
  if (expectedPublicKeyHint && resolvedHint !== expectedPublicKeyHint) {
    return { error: `ed25519 public key hint mismatch: resolved ${resolvedHint}, expected ${expectedPublicKeyHint}` };
  }

  const ok = cryptoVerify(null, Buffer.from(manifestText, "utf8"), key, sigBytes);
  if (!ok) return { error: "ed25519 manifest signature verification failed" };
  return { ok: true, keySource, publicKeyHint: resolvedHint };
}

function verifyManifestAuthenticity(
  backupDir: string,
  manifest: BackupManifest,
  manifestText: string,
  verificationKeyPath?: string,
  publicKeyPath?: string,
  publicKeyHintExpected?: string,
  credentialFreePortableMode = false,
) {
  if (credentialFreePortableMode) {
    if (!manifest.asymmetricSignature) return "credential-free portable restore requires an ed25519 signature";
    const ed25519 = verifyManifestEd25519(
      backupDir, manifest, manifestText, publicKeyPath, publicKeyHintExpected,
    );
    if (!ed25519 || "error" in ed25519) {
      return ed25519 && "error" in ed25519 ? ed25519.error : "ed25519 verification failed";
    }
    return null;
  }
  const hmacError = verifyManifestHmac(backupDir, manifestText, verificationKeyPath);
  if (hmacError) return hmacError;
  // If the manifest advertises an ed25519 signature, it MUST also verify.
  // This means: opting in to asymmetric signing strengthens the guarantee
  // (both HMAC and ed25519 must agree); it never weakens HMAC. Verification
  // is fail-closed: any non-null result that is not {ok:true} blocks restore.
  const ed25519 = verifyManifestEd25519(backupDir, manifest, manifestText, publicKeyPath, publicKeyHintExpected);
  if (ed25519 && "error" in ed25519) return ed25519.error;
  return null;
}

// A restore target is "safe to write into" when it does not already hold a
// live company ledger. Issue #139: the previous readdirSync(root).length === 0
// check is TOCTOU-racy and also rejects benign empty-but-not-empty dirs. The
// real thing we must never clobber is an existing ledger database, so we test
// for that explicit company marker instead.
function targetHoldsLiveCompany(targetCompanyRoot: string) {
  if (!existsSync(targetCompanyRoot)) return false;
  return existsSync(companyPaths(targetCompanyRoot).db);
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

function validateRestoredDb(
  db: Database,
  manifest: BackupManifest,
  restoredCompanyRoot: string,
) {
  try {
    assertSchemaCompatibility(db);
  } catch (error) {
    return {
      ok: false as const,
      error: `restored database schema is incompatible: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
  const integrity = db.query("PRAGMA integrity_check").all() as Array<{ integrity_check?: string }>;
  if (integrity.length !== 1 || integrity[0]?.integrity_check !== "ok") {
    return { ok: false as const, error: `restored database failed integrity check: ${JSON.stringify(integrity)}` };
  }

  const fkErrors = db.query("PRAGMA foreign_key_check").all() as any[];
  if (fkErrors.length > 0) {
    return { ok: false as const, error: `restored database has FK violations: ${JSON.stringify(fkErrors)}` };
  }

  const audit = verifyAuditChain(db, { companyRoot: restoredCompanyRoot });
  if (!audit.ok) {
    return { ok: false as const, error: `restored database has broken audit chain: ${audit.errors.join(", ")}` };
  }

  const stats = {
    journalEntries: (db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n,
    documents: (db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n,
    bankTransactions: (db.query("SELECT COUNT(*) AS n FROM bank_transactions").get() as { n: number }).n,
  };
  if (JSON.stringify(stats) !== JSON.stringify(manifest.ledgerStats)) {
    return { ok: false as const, error: `restored stats ${JSON.stringify(stats)} differ from manifest ${JSON.stringify(manifest.ledgerStats)}` };
  }

  try {
    const documents = db.query("SELECT document_no, stored_path, document_type, sha256_hash FROM documents").all() as Array<{ document_no: string | null; stored_path: string | null; document_type: string; sha256_hash: string }>;
    assertBackupDocumentEvidence(documents, manifest);
  } catch (error) {
    return { ok: false as const, error: `restored document evidence verification failed: ${error instanceof Error ? error.message : String(error)}` };
  }

  return { ok: true as const };
}

function migrateValidateAndStampRestoredDb(
  dbPath: string,
  manifest: BackupManifest,
  restoredCompanyRoot: string,
  restoredAt: string,
  createdBy?: string,
  createdByProgram?: string,
) {
  // Bun can keep file-backed SQLite handles alive on Windows after close(),
  // especially when cached statements were used. Deserialize into an in-memory
  // database so migration, validation, and the restore audit write never open
  // the staging file itself; serialize the verified image back before the
  // atomic swap. This deliberately happens only after authenticity and every
  // declared file hash has been verified and the snapshot was copied into the
  // isolated staging company. Thus a failed legacy upgrade can never mutate
  // either the signed source snapshot or the requested restore target.
  const db = Database.deserialize(readFileSync(dbPath));
  try {
    db.exec("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 30000;");
    migrate(db);
    const validation = validateRestoredDb(db, manifest, restoredCompanyRoot);
    if (!validation.ok) return validation;

    insertAuditLog(db, {
      eventType: "system_restore",
      entityType: "company",
      entityId: 1,
      message: `Restored from backup ${manifest.backupId} (created ${manifest.createdAt}) at ${restoredAt}`,
      createdBy,
      createdByProgram,
    });
    return { ok: true as const, image: db.serialize() };
  } finally {
    db.close();
  }
}

function cleanupRestoreStaging(stagingRoot: string): string[] {
  if (!existsSync(stagingRoot)) return [];
  try {
    removePathWithRetry(stagingRoot);
    return [];
  } catch (error) {
    // Cleanup must never replace the primary restore/validation error. The
    // staging directory is isolated and cannot overwrite the requested target.
    return [`restore staging cleanup failed: ${String(error)}`];
  }
}

export type VerifyBackupSignatureInput = {
  backupDir: string;
  verificationKeyPath?: string;
  publicKeyPath?: string;
  // Out-of-band public-key hint. When set, an ed25519 key resolved from inside
  // the backup must match it or verification fails closed (issue #132).
  publicKeyHint?: string;
};

// trustLevel makes the integrity-vs-authenticity distinction explicit
// (issues #131/#132):
//  - "third-party-authenticity": an ed25519 signature verified against a
//    public key supplied OUT-OF-BAND. A 3rd party can rely on this.
//  - "integrity-only": something verified, but only with key material that
//    ships alongside the backup (the symmetric HMAC key, or an in-backup
//    ed25519 public key). This catches accidental corruption and tampering
//    by an actor without the key, but NOT a forge by a local actor who can
//    read/rewrite the co-located key.
//  - "none": nothing could be verified.
export type BackupTrustLevel = "third-party-authenticity" | "integrity-only" | "none";

export type VerifyBackupSignatureResult = {
  ok: boolean;
  backupId?: string;
  algorithms: string[];
  trustLevel: BackupTrustLevel;
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
    return { ok: false, algorithms: [], trustLevel: "none", errors: [`backupDir does not exist: ${input.backupDir}`] };
  }
  const manifestText = readManifestText(input.backupDir);
  if (!manifestText) return { ok: false, algorithms: [], trustLevel: "none", errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const manifest = readManifest(input.backupDir);
  if (!manifest) return { ok: false, algorithms: [], trustLevel: "none", errors: [`invalid or missing backup manifest in ${input.backupDir}`] };

  const algorithms: string[] = [];
  const errors: string[] = [];
  let sawOutOfBandAuthenticity = false;
  let sawIntegrity = false;

  // Ed25519 is the 3rd-party-friendly path. Try it first, but only if the
  // manifest advertises it. If the verifier only has a public key (no HMAC
  // key) and the manifest has no ed25519 signature, we cannot verify.
  if (manifest.asymmetricSignature) {
    const ed25519 = verifyManifestEd25519(input.backupDir, manifest, manifestText, input.publicKeyPath, input.publicKeyHint);
    if (ed25519 && "error" in ed25519) {
      errors.push(ed25519.error);
    } else if (ed25519) {
      algorithms.push("ed25519");
      // An ed25519 signature is third-party authenticity ONLY when the key
      // came from out-of-band. An in-backup key proves integrity only.
      if (ed25519.keySource === "out-of-band") sawOutOfBandAuthenticity = true;
      else sawIntegrity = true;
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
      // HMAC is symmetric — the key can re-sign — so it is integrity-only,
      // never third-party authenticity (issue #131).
      sawIntegrity = true;
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

  const ok = errors.length === 0 && algorithms.length > 0;
  // trustLevel reflects the STRONGEST guarantee that actually held. If
  // anything failed, the backup is not trustworthy at all -> "none".
  const trustLevel: BackupTrustLevel = !ok
    ? "none"
    : sawOutOfBandAuthenticity
      ? "third-party-authenticity"
      : sawIntegrity
        ? "integrity-only"
        : "none";

  return {
    ok,
    backupId: manifest.backupId,
    algorithms,
    trustLevel,
    publicKeyHint: manifest.asymmetricSignature?.publicKeyHint,
    hmacKeyHint: manifest.manifestSignature?.keyHint,
    errors,
  };
}

// Public entry point. `backupDir` may point at either a backup *directory*
// or a single-file backup *archive* (.tar produced by packBackupArchive). An
// archive is extracted into a throwaway temp directory and restored from
// there — note that HMAC-key inference only works for a directory still
// sitting in its original `backups/` parent, so archive restores generally
// need an explicit `verificationKeyPath` (or an ed25519 public key).
export function restoreSystemBackup(input: RestoreSystemBackupInput): RestoreSystemBackupResult {
  const source = input.backupDir;
  if (source && existsSync(source) && statSync(source).isFile()) {
    let extractDir: string;
    try {
      extractDir = mkdtempSync(join(tmpdir(), "rentemester-restore-archive-"));
    } catch (error) {
      return { ok: false, appliedRules: [RULE_ID], errors: [`failed to stage archive extraction: ${String(error)}`] };
    }
    try {
      extractTar(readFileSync(source), extractDir);
    } catch (error) {
      removePathWithRetry(extractDir);
      return { ok: false, appliedRules: [RULE_ID], errors: [`failed to extract backup archive: ${String(error)}`] };
    }
    try {
      return restoreFromBackupDir({ ...input, backupDir: extractDir });
    } finally {
      removePathWithRetry(extractDir);
    }
  }
  return restoreFromBackupDir(input);
}

function restoreFromBackupDir(input: RestoreSystemBackupInput): RestoreSystemBackupResult {
  const errors: string[] = [];
  if (!input.backupDir || !existsSync(input.backupDir)) errors.push(`backupDir does not exist: ${input.backupDir}`);
  if (!input.targetCompanyRoot) errors.push("targetCompanyRoot is required");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const manifestText = readManifestText(input.backupDir);
  if (!manifestText) return { ok: false, appliedRules: [RULE_ID], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const manifest = readManifest(input.backupDir);
  if (!manifest) return { ok: false, appliedRules: [RULE_ID], errors: [`invalid or missing backup manifest in ${input.backupDir}`] };
  const authenticityError = verifyManifestAuthenticity(
    input.backupDir,
    manifest,
    manifestText,
    input.verificationKeyPath,
    input.publicKeyPath,
    input.publicKeyHint,
    input.credentialFreePortableMode === true,
  );
  if (authenticityError) return { ok: false, appliedRules: [RULE_ID], errors: [authenticityError] };

  const manifestErrors = [
    ensureMatches(input.backupDir, manifest.dbSnapshot),
    ...manifest.copiedFiles.documentsOriginals.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.invoicesIssued.map((file) => ensureMatches(input.backupDir, file)),
    ...manifest.copiedFiles.config.map((file) => ensureMatches(input.backupDir, file)),
  ].filter((value): value is string => Boolean(value));
  if (manifestErrors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors: manifestErrors };

  if (targetHoldsLiveCompany(input.targetCompanyRoot)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`targetCompanyRoot already contains a company ledger; refusing to overwrite: ${input.targetCompanyRoot}`] };
  }

  const snapshotPath = resolveManifestPath(input.backupDir, manifest.dbSnapshot.path);
  if (!snapshotPath) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`manifest path escapes backup dir: ${manifest.dbSnapshot.path}`] };
  }

  // Issue #139: build the entire restored company inside a temp staging
  // directory, validate the database THERE, and only swap it into the target
  // once validation passes. A backup that passes file-hash checks but fails
  // validateRestoredDb (e.g. a broken audit chain) therefore never leaves a
  // half-restored, unrecoverable target behind.
  const resolvedTarget = resolve(input.targetCompanyRoot);
  const stagingRoot = join(
    dirname(resolvedTarget),
    `.restore-${basename(resolvedTarget)}.${process.pid}.${Date.now()}.${randomBytes(8).toString("hex")}.tmp`,
  );
  if (existsSync(stagingRoot)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`restore staging directory already exists: ${stagingRoot}`] };
  }

  const restoredAt = new Date().toISOString();
  let restoredFiles: { documentsOriginals: number; invoicesIssued: number; config: number };
  try {
    const stagingPaths = ensureCompanyDirs(stagingRoot);
    copyFileSync(snapshotPath, stagingPaths.db);
    restoredFiles = {
      documentsOriginals: restoreFiles(input.backupDir, manifest.copiedFiles.documentsOriginals, stagingPaths.documentsOriginals),
      invoicesIssued: restoreFiles(input.backupDir, manifest.copiedFiles.invoicesIssued, stagingPaths.invoicesIssued),
      config: restoreFiles(input.backupDir, manifest.copiedFiles.config, stagingPaths.config),
    };

    const preparedDb = migrateValidateAndStampRestoredDb(
      stagingPaths.db,
      manifest,
      stagingRoot,
      restoredAt,
      input.createdBy,
      input.createdByProgram,
    );
    if (!preparedDb.ok) {
      return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [preparedDb.error, ...cleanupRestoreStaging(stagingRoot)],
      };
    }
    // Persist the verified/stamped image before promoting the entire staging
    // directory. writeFileAtomic fsyncs the bytes and directory entry, so a
    // crash after the directory swap cannot expose a partial live ledger.
    writeFileAtomic(stagingPaths.db, preparedDb.image);

    // Atomic swap into place. The target must not yet hold a live company
    // (checked above); if an empty placeholder dir exists, drop it so the
    // rename lands cleanly.
    if (existsSync(resolvedTarget)) {
      // KODE-8: re-check for a live company IMMEDIATELY before the destructive
      // rmSync — the earlier check ran before the (slow) staging build, and a
      // ledger could have been created in the target during that window. Never
      // recursively wipe a directory that now holds a ledger.
      if (targetHoldsLiveCompany(resolvedTarget)) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [
            `targetCompanyRoot already contains a company ledger; refusing to overwrite: ${input.targetCompanyRoot}`,
            ...cleanupRestoreStaging(stagingRoot),
          ],
        };
      }
      // KODE-8: refuse to recursively delete a NON-EMPTY ledger-less directory
      // unless the caller explicitly opted in. The target might be the user's
      // own files pointed at by mistake; an empty placeholder is safe to drop.
      const entries = readdirSync(resolvedTarget);
      if (entries.length > 0 && !input.allowNonEmptyTarget) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [
            `targetCompanyRoot exists and is not empty (no ledger db): ${input.targetCompanyRoot}. ` +
              `Refusing to recursively delete it — restore into an empty/new directory, or set allowNonEmptyTarget to overwrite deliberately.`,
            ...cleanupRestoreStaging(stagingRoot),
          ],
        };
      }
      removePathWithRetry(resolvedTarget);
    } else {
      mkdirSync(dirname(resolvedTarget), { recursive: true });
    }
    renamePathWithRetry(stagingRoot, resolvedTarget);
  } catch (error) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`restore failed: ${String(error)}`, ...cleanupRestoreStaging(stagingRoot)],
    };
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
