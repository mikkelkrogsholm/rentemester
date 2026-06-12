// Backup-status pill — rendered in the System-status section.

import type { BackupComplianceStatus } from "../system-backups";
import { daysAgoLabel } from "./_shared";

export function backupStatusPill(backup: BackupComplianceStatus): { pill: string; detail: string } {
  const days = backup.daysSinceLatestBackup;
  if (backup.backupsFound === 0) {
    return { pill: `<span class="pill danger">Ingen backup</span>`, detail: "ingen registreret" };
  }
  if (days === null || days > 7) {
    return { pill: `<span class="pill danger">Forfalden</span>`, detail: daysAgoLabel(days) };
  }
  if (days >= 5) {
    return { pill: `<span class="pill warning">Snart due</span>`, detail: daysAgoLabel(days) };
  }
  return { pill: `<span class="pill success">✔ OK</span>`, detail: daysAgoLabel(days) };
}
