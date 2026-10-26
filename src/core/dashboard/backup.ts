import { documentAttr } from "../../design/document-html";
// Backup-status pill — rendered in the System-status section.

import type { BackupComplianceStatus } from "../system-backups";
import { daysAgoLabel } from "./_shared";

export function backupStatusPill(backup: BackupComplianceStatus): { pill: string; detail: string } {
  const days = backup.daysSinceLatestBackup;
  if (backup.backupsFound === 0) {
    return { pill: `<span ${documentAttr("pillDanger")}>Ingen backup</span>`, detail: "ingen registreret" };
  }
  if (days === null || days > 7) {
    return { pill: `<span ${documentAttr("pillDanger")}>Forfalden</span>`, detail: daysAgoLabel(days) };
  }
  if (days >= 5) {
    return { pill: `<span ${documentAttr("pillWarning")}>Snart due</span>`, detail: daysAgoLabel(days) };
  }
  return { pill: `<span ${documentAttr("pillSuccess")}>✔ OK</span>`, detail: daysAgoLabel(days) };
}
