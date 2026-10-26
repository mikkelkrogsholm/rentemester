/**
 * Evidence writer for the deliberately unsafe offline VIES demo fixture.
 *
 * Only disposable demo/smoke flows call this immediately after `init`. The
 * seed script independently verifies its root, filesystem binding, audit
 * fingerprint and absence of business state before it writes VIES evidence.
 */
import type { Database } from "bun:sqlite";
import { createHash, randomUUID } from "node:crypto";
import {
  chmodSync,
  closeSync,
  lstatSync,
  openSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { companyPaths } from "./paths";
import { openLedgerReadOnly } from "./ledger-inspection";

export const OFFLINE_VIES_DEMO_MARKER_FILENAME = ".rentemester-agent-demo-vies-seed-v1.json";
export const OFFLINE_VIES_DEMO_MARKER_FORMAT = "rentemester-agent-demo-vies-seed/v1";

function auditTrailSha256(db: Database) {
  const rows = db.query(
    "SELECT id, event_type, entity_type, entity_id, message, actor FROM audit_log ORDER BY id",
  ).all();
  return createHash("sha256").update(JSON.stringify(rows)).digest("hex");
}

/** Atomically create the read-only marker for one freshly initialized ledger. */
export function writeOfflineViesDemoMarker(companyRoot: string) {
  const root = realpathSync(resolve(companyRoot));
  const ledgerPath = companyPaths(root).db;
  const ledger = lstatSync(ledgerPath);
  if (!ledger.isFile() || ledger.isSymbolicLink() || ledger.nlink !== 1) {
    throw new Error("fresh demo ledger is not a regular unlinked file");
  }
  const target = join(root, OFFLINE_VIES_DEMO_MARKER_FILENAME);
  const temporary = `${target}.${randomUUID()}.tmp`;
  let fd: number | undefined;
  try {
    fd = openSync(temporary, "wx", 0o600);
    const db = openLedgerReadOnly(ledgerPath);
    let auditTrailSha256: string;
    try {
      auditTrailSha256 = auditTrailSha256For(db);
    } finally {
      db.close();
    }
    writeFileSync(fd, `${JSON.stringify({
      format: OFFLINE_VIES_DEMO_MARKER_FORMAT,
      purpose: "offline-vies-seed",
      companyRoot: root,
      ledger: { dev: ledger.dev, ino: ledger.ino },
      auditTrailSha256,
      nonce: randomUUID(),
    })}\n`);
    closeSync(fd);
    fd = undefined;
    renameSync(temporary, target);
    chmodSync(target, 0o400);
    return target;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

// Named wrapper keeps the local variable in the writer unambiguous.
function auditTrailSha256For(db: Database) {
  return auditTrailSha256(db);
}
