// Creditor card — open and overdue accounts-payable. Symmetric to the
// existing open-invoices (debitor) table.

import type { PayablesListResult } from "../payables";
import { escapeHtml, formatDateShort, formatDkk } from "./_shared";

/**
 * Creditor card — open and overdue accounts-payable. Symmetric to the
 * existing open-invoices (debitor) table.
 */
export function payablesSection(payables: PayablesListResult): string {
  if (payables.count === 0 || payables.rows.length === 0) {
    return `<div class="empty-state">Ingen åbne kreditorposter</div>`;
  }
  const maxRows = 10;
  // buildPayablesList already sorts most-overdue first.
  const visible = payables.rows.slice(0, maxRows);
  const overflow = payables.rows.length - visible.length;
  const rows = visible.map((row) => {
    const supplier = row.supplierName ?? "—";
    const pill = row.isOverdue
      ? `<span class="pill danger">forfalden${row.overdueDays > 0 ? ` (${row.overdueDays} d)` : ""}</span>`
      : `<span class="pill success">åben</span>`;
    return `<tr>
  <td class="mono">${escapeHtml(row.billNo ?? `#${row.payableId}`)}</td>
  <td>${escapeHtml(supplier)}</td>
  <td class="amount">${escapeHtml(formatDkk(row.openBalance))}</td>
  <td class="amount mono">${escapeHtml(formatDateShort(row.dueDate))}</td>
  <td class="center">${pill}</td>
</tr>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div class="muted" style="margin-top: var(--space-xs); font-size: 13px;">… og ${overflow} yderligere</div>`
    : "";
  const summary =
    `<div class="muted" style="margin-bottom: var(--space-sm); font-size: 13px;">` +
    `Åben kreditorgæld i alt <span class="mono">${escapeHtml(formatDkk(payables.totalOpenBalance))}</span>` +
    (payables.overdueOpenBalance > 0
      ? ` · heraf overforfalden <span class="mono">${escapeHtml(formatDkk(payables.overdueOpenBalance))}</span>`
      : "") +
    `</div>`;
  return `${summary}<table class="dash-table">
  <thead>
    <tr>
      <th>Bilagsnr.</th>
      <th>Leverandør</th>
      <th class="amount">Åben saldo</th>
      <th class="amount">Forfald</th>
      <th class="center">Status</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
${overflowRow}`;
}
