import { companyPaths } from "../../core/paths";
import { openLedgerReadOnly } from "../../core/ledger-inspection";
import { companyRootForSlug } from "../../core/workspace";
import { listChangesSince } from "../../core/audit-log";
import type { ServerConfig } from "../config";
import { okResponse } from "./_shared";

/** Cockpit-local projection of the existing append-only audit reader. It is
 * not an additional agent capability: MCP's audit_log_list remains the public
 * audit-read contract and this route only adds a browser cursor envelope. */
export function handleCompanyChangesSince(config: ServerConfig, slug: string, url: URL): Response {
  const raw = url.searchParams.get("after") ?? "0";
  const after = /^\d+$/.test(raw) ? Number(raw) : 0;
  const db = openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot, slug)).db);
  try {
    return okResponse({ changes: listChangesSince(db, after) });
  } finally { db.close(); }
}
