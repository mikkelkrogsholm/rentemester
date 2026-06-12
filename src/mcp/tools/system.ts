/**
 * MCP-tools for system-niveau-operationer.
 *
 *  - `system_healthcheck` (read)
 *  - `system_backup_status` (read)
 *  - `system_backup` (write-irreversible)
 *  - `system_backup_archive` (write-irreversible)
 *  - `system_backup_governance` (read)
 *  - `system_backup_destination_list` (read)
 *  - `system_backup_destination_add` (write-irreversible)
 *  - `system_backup_destination_remove` (write-irreversible)
 *  - `system_backup_place` (write-irreversible)
 *  - `system_backup_confirm_placement` (write-irreversible)
 *  - `system_backup_lock` (write-irreversible)
 *  - `system_export_authority` (write-irreversible)
 *  - `system_restore_backup` (DESTRUCTIVE — kræver confirmText)
 *
 * Implementation note: the 13 tool registrations used to live inline in this
 * file. They were split into per-sub-domain helpers in `./system/`. Each helper
 * is called below in the same order the tools were originally registered —
 * MCP clients may rely on `tools/list` returning the tools in this exact
 * sequence (same pattern as the earlier `./invoice/` split).
 *
 * The lock tool lives in its own file (`./system/backup-lock.ts`) instead of
 * inside `./system/backup.ts` so the destination tools can be registered
 * between the create-backup tools and the lock tool — matching the original
 * registration order verbatim.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerSystemHealthcheckTools } from "./system/healthcheck";
import { registerSystemBackupTools } from "./system/backup";
import { registerSystemBackupDestinationTools } from "./system/backup-destinations";
import { registerSystemBackupLockTools } from "./system/backup-lock";
import { registerSystemAuthorityTools } from "./system/authority";
import { registerSystemRestoreTools } from "./system/restore";

export function registerSystemTools(server: McpServer): void {
  // Order is load-bearing — see the file-level comment.
  registerSystemHealthcheckTools(server);
  registerSystemBackupTools(server);
  registerSystemBackupDestinationTools(server);
  registerSystemBackupLockTools(server);
  registerSystemAuthorityTools(server);
  registerSystemRestoreTools(server);
}
