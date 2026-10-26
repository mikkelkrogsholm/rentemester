/**
 * Low-level, fail-closed storage for voucher originals.
 *
 * This module deliberately handles the source exactly once.  Calling code
 * must derive every content property (hash, MIME and scanner input) from the
 * returned snapshot, never by opening the caller-controlled path again.
 */

import type { Database } from "bun:sqlite";
import { createHash, randomBytes } from "node:crypto";
import { closeSync, constants, fstatSync, fsyncSync, linkSync, lstatSync, mkdirSync, openSync, readSync, realpathSync, unlinkSync, writeSync } from "node:fs";
import { basename, isAbsolute, join, relative, resolve } from "node:path";

export const MAX_DOCUMENT_BYTES = 25 * 1024 * 1024;
/** PDF parsing deliberately has a tighter cap than general voucher storage. */
export const MAX_PDF_PARSE_BYTES = 10 * 1024 * 1024;

export type DocumentSnapshot = {
  bytes: Buffer;
  sha256: string;
  filename: string;
};

export const DOCUMENT_EVIDENCE_MAX_BYTES = 50 * 1024 * 1024;

export type DocumentEvidenceFailure =
  | "invalid_metadata"
  | "invalid_path"
  | "unavailable"
  | "unsafe_store"
  | "unsafe_file"
  | "invalid_size"
  | "hash_mismatch";

/** Stable, path-free failure for every registered-document reader. */
export class DocumentEvidenceError extends Error {
  readonly code: string;

  constructor(readonly reason: DocumentEvidenceFailure) {
    const code = reason === "hash_mismatch"
      ? "DOCUMENT_EVIDENCE_INTEGRITY_MISMATCH"
      : reason === "invalid_path" || reason === "invalid_metadata"
        ? "DOCUMENT_EVIDENCE_INVALID"
        : "DOCUMENT_EVIDENCE_UNAVAILABLE";
    super(code);
    this.name = "DocumentEvidenceError";
    this.code = code;
  }
}

export type RegisteredDocumentEvidenceInput = {
  storedPath: string;
  expectedSha256: string;
  documentType: string;
  maxBytes?: number;
};

export type RegisteredDocumentSnapshot = DocumentSnapshot & { path: string };

export function isIssuedDocumentEvidence(documentType: string): boolean {
  return documentType === "issued_invoice" ||
    documentType === "issued_invoice_pdf" ||
    documentType === "credit_note";
}

/** The canonical store identity remains valid when evidence is rebased on restore. */
export function registeredDocumentEvidenceLocation(storedPath: string, documentType: string) {
  const relativeStore = isIssuedDocumentEvidence(documentType)
    ? (["invoices", "issued"] as const)
    : (["documents", "originals"] as const);
  const filename = evidencePathParts(storedPath, relativeStore).at(-1)!;
  return { relativeStore, filename };
}

type StableStat = { dev: number; ino: number; size: number; mtimeMs: number; ctimeMs: number };

function stableStat(fd: number, maxBytes = MAX_DOCUMENT_BYTES): StableStat {
  const stat = fstatSync(fd);
  if (!stat.isFile()) throw new Error("document source is not a regular file");
  if (stat.size <= 0) throw new Error("document source is empty");
  if (stat.size > maxBytes) throw new Error(`document source exceeds ${maxBytes} byte limit`);
  return { dev: stat.dev, ino: stat.ino, size: stat.size, mtimeMs: stat.mtimeMs, ctimeMs: stat.ctimeMs };
}

function sameStat(a: StableStat, b: StableStat): boolean {
  return a.dev === b.dev && a.ino === b.ino && a.size === b.size && a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs;
}

/** Open once with O_NOFOLLOW, fstat and snapshot the exact bounded bytes. */
export function snapshotDocumentSource(path: string, maxBytes = MAX_DOCUMENT_BYTES): DocumentSnapshot {
  let fd: number | undefined;
  try {
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    const before = stableStat(fd, maxBytes);
    const bytes = Buffer.allocUnsafe(before.size);
    let offset = 0;
    while (offset < bytes.length) {
      const count = readSync(fd, bytes, offset, bytes.length - offset, offset);
      if (count <= 0) throw new Error("document source changed while being read");
      offset += count;
    }
    const after = stableStat(fd, maxBytes);
    if (!sameStat(before, after)) throw new Error("document source changed while being read");
    return { bytes, sha256: createHash("sha256").update(bytes).digest("hex"), filename: basename(path) };
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && (error as { code?: string }).code === "ELOOP") {
      throw new Error("document source must not be a symbolic link");
    }
    if (error instanceof Error && /^(document source|document store|invalid document)/.test(error.message)) throw error;
    throw new Error("document source cannot be safely opened");
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

function contained(root: string, candidate: string): boolean {
  const value = relative(root, candidate);
  return value === "" ||
    (value !== ".." && !value.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) && !isAbsolute(value));
}

function evidencePathParts(storedPath: string, expectedStore: readonly [string, string]): string[] {
  if (!storedPath || storedPath !== storedPath.trim() || /[\0-\x1f\x7f]/.test(storedPath)) {
    throw new DocumentEvidenceError("invalid_path");
  }
  const normalized = storedPath.replaceAll("\\", "/");
  const withoutLeadingSlash = normalized.replace(/^\/+/, "");
  const parts = withoutLeadingSlash.split("/");
  if (parts.length < 3 || parts.some((part) => !part || part === "." || part === "..")) {
    throw new DocumentEvidenceError("invalid_path");
  }

  const knownStores: ReadonlyArray<readonly [string, string]> = [
    ["documents", "originals"],
    ["invoices", "issued"],
  ];
  const matches: Array<{ index: number; store: readonly [string, string] }> = [];
  for (let index = 0; index < parts.length - 1; index += 1) {
    for (const store of knownStores) {
      if (parts[index] === store[0] && parts[index + 1] === store[1]) {
        matches.push({ index, store });
      }
    }
  }
  if (
    matches.length !== 1 ||
    matches[0]!.index !== parts.length - 3 ||
    matches[0]!.store[0] !== expectedStore[0] ||
    matches[0]!.store[1] !== expectedStore[1]
  ) {
    throw new DocumentEvidenceError("invalid_path");
  }
  return parts;
}

/**
 * Resolve and snapshot immutable evidence inside the selected company only.
 *
 * A historical absolute path is metadata: the resolver trusts only one exact
 * trailing `documents/originals/<file>` or `invoices/issued/<file>` identity,
 * selected by document type, and rebases it below the current company root.
 * A basename by itself is deliberately insufficient and therefore rejected.
 */
export function snapshotRegisteredDocumentEvidence(
  companyRoot: string,
  input: RegisteredDocumentEvidenceInput,
): RegisteredDocumentSnapshot {
  const expectedSha256 = input.expectedSha256.trim().toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(expectedSha256) || !input.documentType) {
    throw new DocumentEvidenceError("invalid_metadata");
  }
  const { relativeStore, filename } = registeredDocumentEvidenceLocation(input.storedPath, input.documentType);
  const maxBytes = input.maxBytes ?? DOCUMENT_EVIDENCE_MAX_BYTES;
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) {
    throw new DocumentEvidenceError("invalid_metadata");
  }

  try {
    const lexicalRoot = resolve(companyRoot);
    const rootInfo = lstatSync(lexicalRoot);
    if (!rootInfo.isDirectory()) throw new DocumentEvidenceError("unsafe_store");
    const canonicalRoot = realpathSync(lexicalRoot);
    let lexicalStore = lexicalRoot;
    for (const segment of relativeStore) {
      lexicalStore = join(lexicalStore, segment);
      const info = lstatSync(lexicalStore);
      if (info.isSymbolicLink() || !info.isDirectory()) {
        throw new DocumentEvidenceError("unsafe_store");
      }
    }
    const canonicalStore = realpathSync(lexicalStore);
    if (!contained(canonicalRoot, canonicalStore)) {
      throw new DocumentEvidenceError("unsafe_store");
    }

    const candidate = join(canonicalStore, filename);
    const candidateInfo = lstatSync(candidate);
    if (candidateInfo.isSymbolicLink() || !candidateInfo.isFile()) {
      throw new DocumentEvidenceError("unsafe_file");
    }
    if (candidateInfo.size <= 0 || candidateInfo.size > maxBytes) {
      throw new DocumentEvidenceError("invalid_size");
    }
    const canonicalCandidate = realpathSync(candidate);
    if (!contained(canonicalStore, canonicalCandidate) || !contained(canonicalRoot, canonicalCandidate)) {
      throw new DocumentEvidenceError("unsafe_file");
    }

    let snapshot: DocumentSnapshot;
    try {
      snapshot = snapshotDocumentSource(candidate, maxBytes);
    } catch {
      throw new DocumentEvidenceError("unavailable");
    }
    if (snapshot.sha256 !== expectedSha256) {
      throw new DocumentEvidenceError("hash_mismatch");
    }
    return { ...snapshot, path: candidate };
  } catch (error) {
    if (error instanceof DocumentEvidenceError) throw error;
    throw new DocumentEvidenceError("unavailable");
  }
}

export function snapshotRegisteredDocument(
  db: Database,
  companyRoot: string,
  documentId: number,
  maxBytes = DOCUMENT_EVIDENCE_MAX_BYTES,
): RegisteredDocumentSnapshot & { mimeType: string | null; documentType: string } {
  const row = db.query(
    `SELECT stored_path AS storedPath, sha256_hash AS sha256Hash,
            mime_type AS mimeType, document_type AS documentType
       FROM documents WHERE id = ?`,
  ).get(documentId) as {
    storedPath: string | null;
    sha256Hash: string | null;
    mimeType: string | null;
    documentType: string;
  } | null;
  if (!row?.storedPath || !row.sha256Hash) {
    throw new DocumentEvidenceError("invalid_metadata");
  }
  return {
    ...snapshotRegisteredDocumentEvidence(companyRoot, {
      storedPath: row.storedPath,
      expectedSha256: row.sha256Hash,
      documentType: row.documentType,
      maxBytes,
    }),
    mimeType: row.mimeType,
    documentType: row.documentType,
  };
}

export function snapshotRegisteredPdfDocument(db: Database, companyRoot: string, documentId: number): DocumentSnapshot {
  const snapshot = snapshotRegisteredDocument(db, companyRoot, documentId, MAX_PDF_PARSE_BYTES);
  if (snapshot.mimeType !== "application/pdf") {
    throw new DocumentEvidenceError("invalid_metadata");
  }
  return snapshot;
}

/**
 * Build/check a private canonical store below a real company root.  Every
 * existing segment is lstat'ed before it is trusted; a symlink is never a
 * valid directory in the evidence path.
 */
export function ensureCanonicalDocumentStore(companyRoot: string, relativeStore: string): string {
  // Preserve the lexical root in stored_path for portable, existing callers
  // (macOS commonly spells the same volume as /var and /private/var), while
  // making every containment decision against canonical paths.
  const lexicalRoot = resolve(companyRoot);
  const root = realpathSync(lexicalRoot);
  const requested = resolve(lexicalRoot, relativeStore);
  if (!contained(lexicalRoot, requested)) throw new Error("document store escapes company root");
  const segments = relativeStore.split(/[\\/]+/).filter(Boolean);
  let current = lexicalRoot;
  for (const segment of segments) {
    if (segment === "." || segment === "..") throw new Error("invalid document store path");
    current = join(current, segment);
    try {
      const info = lstatSync(current);
      if (info.isSymbolicLink() || !info.isDirectory()) throw new Error("document store must be a non-symlink directory");
    } catch (error) {
      if (!(error && typeof error === "object" && "code" in error && (error as { code?: string }).code === "ENOENT")) throw error;
      mkdirSync(current, { mode: 0o700 });
      const info = lstatSync(current);
      if (info.isSymbolicLink() || !info.isDirectory()) throw new Error("document store must be a non-symlink directory");
    }
  }
  const canonical = realpathSync(current);
  if (!contained(root, canonical)) throw new Error("document store escapes company root");
  return requested;
}

export type PublishDocumentResult = { path: string; published: boolean };

/**
 * Atomically publish immutable bytes without ever replacing a target.  A
 * concurrent same-content writer may reuse the final file; a conflicting or
 * symlink destination is rejected.  Only this call's private temp is removed.
 */
export function publishDocumentSnapshot(store: string, filename: string, snapshot: DocumentSnapshot): PublishDocumentResult {
  const target = join(store, filename);
  const temp = join(store, `.${filename}.${randomBytes(16).toString("hex")}.tmp`);
  let fd: number | undefined;
  try {
    fd = openSync(temp, constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | constants.O_NOFOLLOW, 0o600);
    let offset = 0;
    while (offset < snapshot.bytes.length) offset += writeSync(fd, snapshot.bytes, offset, snapshot.bytes.length - offset);
    fsyncSync(fd);
    closeSync(fd); fd = undefined;
    try {
      linkSync(temp, target); // atomic create-if-absent; never replace
      unlinkSync(temp);
      return { path: target, published: true };
    } catch (error) {
      if (!(error && typeof error === "object" && "code" in error && (error as { code?: string }).code === "EEXIST")) throw error;
      const existing = lstatSync(target);
      if (existing.isSymbolicLink() || !existing.isFile()) throw new Error("document evidence destination is not a regular file");
      const actual = snapshotDocumentSource(target);
      if (actual.sha256 !== snapshot.sha256 || actual.bytes.length !== snapshot.bytes.length) {
        throw new Error("document evidence destination already exists with conflicting bytes");
      }
      return { path: target, published: false };
    }
  } finally {
    if (fd !== undefined) closeSync(fd);
    try { unlinkSync(temp); } catch { /* private temp may already be gone */ }
  }
}

/** Remove only a known final evidence file and only if its bytes still match. */
export function removePublishedSnapshot(path: string, snapshot: DocumentSnapshot): void {
  try {
    const info = lstatSync(path);
    if (info.isSymbolicLink() || !info.isFile()) return;
    if (snapshotDocumentSource(path).sha256 === snapshot.sha256) unlinkSync(path);
  } catch { /* cleanup is best effort; caller retains database consistency checks */ }
}
