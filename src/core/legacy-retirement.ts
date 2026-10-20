import { existsSync } from "node:fs";
import { join } from "node:path";
import { Database } from "bun:sqlite";

/** All ordinary legacy writers must stop after an explicit Rust adoption
 * retirement. The durable SQL triggers also fence already-open handles.
 * This startup check is not evidence that old network/filesystem work drained. */
export function assertLegacySourceActive(db: Database): void {
  if (db.query("SELECT 1 FROM sqlite_schema WHERE name = 'rm_legacy_retirement_v1' OR substr(name,1,18) = 'rm_legacy_retired_' LIMIT 1").get()) {
    throw new Error("legacy source retired for Rust adoption; use the adopted Rust workspace");
  }
}

/** Protect manifest/new-company mutations as well as existing ledgers. */
export function assertLegacyWorkspaceActive(root: string): void {
  const path=join(root,".rentemester","workspace-control.sqlite");
  if(!existsSync(path)) return;
  const db=new Database(path,{readonly:true});
  try { assertLegacySourceActive(db); } finally { db.close(); }
}
