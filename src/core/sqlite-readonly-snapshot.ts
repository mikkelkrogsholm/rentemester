import { createHash, randomUUID } from "node:crypto";
import {
  lstatSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database } from "bun:sqlite";

type SourceFile = { suffix: string; bytes: Buffer; sha256: string };

function readSourceFile(path: string, suffix: string, required: boolean): SourceFile | null {
  const sourcePath = `${path}${suffix}`;
  let stat: ReturnType<typeof lstatSync>;
  try {
    stat = lstatSync(sourcePath);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (!required && code === "ENOENT") return null;
    throw error;
  }
  if (stat.isSymbolicLink()) throw new Error(`sqlite source${suffix || " database"} must not be a symbolic link`);
  if (!stat.isFile()) throw new Error(`sqlite source${suffix || " database"} must be a regular file`);
  const bytes = readFileSync(sourcePath);
  return { suffix, bytes, sha256: createHash("sha256").update(bytes).digest("hex") };
}

function readSourceState(path: string): SourceFile[] {
  return [
    readSourceFile(path, "", true),
    readSourceFile(path, "-wal", false),
    readSourceFile(path, "-shm", false),
    readSourceFile(path, "-journal", false),
  ].filter((entry): entry is SourceFile => entry !== null);
}

function sameState(left: SourceFile[], right: SourceFile[]): boolean {
  return left.length === right.length && left.every((entry, index) => {
    const other = right[index];
    return other?.suffix === entry.suffix && other.sha256 === entry.sha256;
  });
}

/**
 * Opens a consistent disposable copy of an existing SQLite database.
 *
 * SQLite read-only connections can still update an existing WAL `-shm` file.
 * Reading the source through a stable copy keeps every original DB/WAL/SHM/
 * journal byte untouched while retaining committed WAL frames. The bounded
 * retry fails closed when a writer changes the source during capture.
 */
function openSqliteSnapshot(path: string, readonly: boolean): Database {
  let captured: SourceFile[] | null = null;
  for (let attempt = 0; attempt < 4; attempt += 1) {
    const before = readSourceState(path);
    const after = readSourceState(path);
    if (sameState(before, after)) {
      captured = before;
      break;
    }
  }
  if (!captured) throw new Error("sqlite source changed during read-only snapshot");

  const snapshotRoot = mkdtempSync(join(tmpdir(), "rentemester-sqlite-readonly-"));
  const snapshotPath = join(snapshotRoot, `snapshot-${randomUUID()}.sqlite`);
  try {
    for (const source of captured) {
      // SQLite rebuilds shared-memory coordination for the private copy.
      if (source.suffix === "-shm") continue;
      writeFileSync(`${snapshotPath}${source.suffix}`, source.bytes, { mode: 0o600 });
    }

    if (captured.some((source) => source.suffix === "-wal" || source.suffix === "-journal")) {
      const recovery = new Database(snapshotPath);
      try {
        recovery.query("SELECT 1 FROM sqlite_master LIMIT 1").get();
        recovery.run("PRAGMA wal_checkpoint(TRUNCATE)");
      } finally {
        (recovery as Database & { clearQueryCache(): void }).clearQueryCache();
        recovery.close();
      }
    }

    const db = readonly
      ? new Database(snapshotPath, { readonly: true })
      : new Database(snapshotPath);
    if (readonly) db.run("PRAGMA query_only = ON");
    db.run("PRAGMA foreign_keys = ON");
    const close = db.close.bind(db);
    let closed = false;
    Object.defineProperty(db, "close", {
      value: () => {
        if (closed) return;
        (db as Database & { clearQueryCache(): void }).clearQueryCache();
        try {
          close();
          closed = true;
        } finally {
          rmSync(snapshotRoot, { recursive: true, force: true });
        }
      },
    });
    return db;
  } catch (error) {
    rmSync(snapshotRoot, { recursive: true, force: true });
    throw error;
  }
}

export function openSqliteReadOnlySnapshot(path: string): Database {
  return openSqliteSnapshot(path, true);
}

/** Writable only inside its disposable copy; the source stays byte-identical. */
export function openSqliteDisposableSnapshot(path: string): Database {
  return openSqliteSnapshot(path, false);
}
