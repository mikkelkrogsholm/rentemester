// Page header — company name, dashboard date, backup-age label, CVR.

import {
  daysAgoLabel,
  escapeHtml,
  formatDateLong,
  type DashboardInput,
} from "./_shared";

export function header(input: DashboardInput): string {
  const company = input.company;
  const dateLong = formatDateLong(input.asOfDate);
  const backupDays = input.backup.daysSinceLatestBackup;
  const backupLabel = input.backup.backupsFound === 0
    ? "Backup: ingen registreret"
    : `Backup: ${daysAgoLabel(backupDays)}`;
  const cvrLine = company.cvr
    ? `<div class="cvr">CVR ${escapeHtml(company.cvr)}</div>`
    : "";
  return `<header class="header">
  <div>
    <h1>${escapeHtml(company.name)}</h1>
    <div class="meta">Dashboard · ${escapeHtml(dateLong)} · ${escapeHtml(backupLabel)}</div>
  </div>
  ${cvrLine}
</header>`;
}
