import { documentAttr } from "../../design/document-html";
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
  return `<section ${documentAttr("section")}>
  <h2 ${documentAttr("h2")}>System-status</h2>
  <div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Backup-status</div>
      <div ${documentAttr("detail")}>${escapeHtml(backupSub)}${escapeHtml(activityNote)}</div>
    </div>
    <div>${backupPill.pill} <span ${documentAttr("muted")}>${escapeHtml(backupPill.detail)}</span></div>
  </div>
  <div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Audit-chain</div>
      <div ${documentAttr("detail")}>verificeret ved render</div>
    </div>
    <div>${auditStatusPill(audit.ok, audit.entryCount, audit.firstError)}</div>
  </div>
</section>`;
}
