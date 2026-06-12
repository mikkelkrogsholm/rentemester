// System-status section — backup-status + audit-chain rows.

import { auditStatusPill } from "./audit";
import { backupStatusPill } from "./backup";
import { escapeHtml, formatTimestampShort, type DashboardInput } from "./_shared";

export function statusSection(input: DashboardInput): string {
  const backupPill = backupStatusPill(input.backup);
  const backupSub = input.backup.latestBackupAt
    ? formatTimestampShort(input.backup.latestBackupAt)
    : "—";
  const activityNote = input.backup.hasActivitySinceBackup && (input.backup.daysSinceLatestBackup ?? 0) > 0
    ? " (ændringer siden seneste backup)"
    : "";
  const audit = input.audit;
  return `<section class="section">
  <h2>System-status</h2>
  <div class="status-row">
    <div>
      <div class="label">Backup-status</div>
      <div class="detail">${escapeHtml(backupSub)}${escapeHtml(activityNote)}</div>
    </div>
    <div>${backupPill.pill} <span class="muted">${escapeHtml(backupPill.detail)}</span></div>
  </div>
  <div class="status-row">
    <div>
      <div class="label">Audit-chain</div>
      <div class="detail">verificeret ved render</div>
    </div>
    <div>${auditStatusPill(audit.ok, audit.entryCount, audit.firstError)}</div>
  </div>
</section>`;
}
